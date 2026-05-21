//! shell integration（OSC 133 / OSC 7）の注入（Phase 2）。
//!
//! `docs/block-boundary-spike.md` Decision Log の方式 **(c) 自前ラッパで強制注入** を実装する。
//! ユーザの `~/.zshrc` / `~/.bash_profile` は**書き換えない**。代わりに一時ディレクトリへ
//! ラッパ rc を生成し、その中でユーザ本来の rc を `source` した上で hook を足す。
//!
//! - **zsh**: `ZDOTDIR` を一時ディレクトリへ向ける。`.zshenv`/`.zprofile`/`.zshrc`/`.zlogin`
//!   の各ラッパがユーザの `$HOME` 配下を source し、末尾で `ZDOTDIR` を一時 dir に再固定して
//!   次のファイルも一時 dir から読ませる。integration 本体は `.zshrc` に入れる。
//! - **bash**: `--rcfile` で一時 rc を渡し、その中でユーザの profile / bashrc を source した上で
//!   `PROMPT_COMMAND`（D/A/OSC7）と `DEBUG` trap（C）を足す。bash は best-effort 扱い。
//!
//! 一時 dir は [`Integration`] の `Drop` で削除する（外部 crate に依存しない）。

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::Shell;

/// 注入準備の結果。`CommandBuilder` に渡す env / args と、後始末用の一時 dir を持つ。
pub struct Integration {
    dir: PathBuf,
    /// shell に設定する追加環境変数。
    pub env: Vec<(String, String)>,
    /// shell に追加するコマンドライン引数。
    pub args: Vec<String>,
    /// `true` の時はログインシェル化（`-l`）を抑止する（bash は `--rcfile` と両立しないため）。
    pub suppress_login: bool,
}

impl Drop for Integration {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 一時 dir 名の衝突回避用カウンタ。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 指定シェル向けの integration を用意する。失敗時は `Ok(None)`（＝integration なしで起動）。
pub fn prepare(shell: Shell) -> io::Result<Integration> {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tanaterm-int-{}-{}", std::process::id(), seq));
    std::fs::create_dir_all(&dir)?;

    match shell {
        Shell::Zsh => prepare_zsh(dir),
        Shell::Bash => prepare_bash(dir),
    }
}

fn prepare_zsh(dir: PathBuf) -> io::Result<Integration> {
    let dir_str = dir.to_string_lossy().into_owned();
    let reassert = format!("export ZDOTDIR={}\n", shell_quote(&dir_str));

    // ユーザの $HOME 配下の同名ファイルを source してから ZDOTDIR を一時 dir に戻す。
    let wrap = |user_file: &str| {
        format!(
            "[ -f \"$HOME/{user_file}\" ] && source \"$HOME/{user_file}\"\n{reassert}",
            user_file = user_file,
            reassert = reassert,
        )
    };

    std::fs::write(dir.join(".zshenv"), wrap(".zshenv"))?;
    std::fs::write(dir.join(".zprofile"), wrap(".zprofile"))?;
    std::fs::write(dir.join(".zlogin"), wrap(".zlogin"))?;
    // .zshrc はユーザ rc を source した後に integration 本体を足す。
    let zshrc = format!(
        "[ -f \"$HOME/.zshrc\" ] && source \"$HOME/.zshrc\"\n{}",
        ZSH_INTEGRATION
    );
    std::fs::write(dir.join(".zshrc"), zshrc)?;

    Ok(Integration {
        dir,
        env: vec![
            ("ZDOTDIR".into(), dir_str),
            ("TERM_PROGRAM".into(), "tanaterm".into()),
        ],
        args: Vec::new(),
        suppress_login: false,
    })
}

fn prepare_bash(dir: PathBuf) -> io::Result<Integration> {
    let rc = dir.join("bashrc");
    let body = format!("{}\n{}", BASH_SOURCE_USER, BASH_INTEGRATION);
    std::fs::write(&rc, body)?;

    Ok(Integration {
        dir,
        env: vec![("TERM_PROGRAM".into(), "tanaterm".into())],
        // --rcfile はログインシェルでは無視されるため suppress_login で -l を外す。
        args: vec!["--rcfile".into(), rc.to_string_lossy().into_owned()],
        suppress_login: true,
    })
}

/// シングルクォートで安全に囲む（中の `'` をエスケープ）。
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// zsh の precmd/preexec hook 本体。OSC 133 A/C/D と OSC 7 を発行する。
const ZSH_INTEGRATION: &str = r#"# --- tanaterm shell integration (OSC 133 / OSC 7) ---
if [[ -o interactive ]]; then
  autoload -Uz add-zsh-hook 2>/dev/null
  __tanaterm_osc7() { printf '\e]7;file://%s%s\a' "${HOST}" "${PWD}"; }
  __tanaterm_precmd() {
    local __exit=$?
    printf '\e]133;D;%s\a' "$__exit"
    printf '\e]133;A\a'
    __tanaterm_osc7
  }
  __tanaterm_preexec() { printf '\e]133;C\a'; }
  add-zsh-hook precmd __tanaterm_precmd
  add-zsh-hook preexec __tanaterm_preexec
fi
"#;

/// bash でユーザの login / interactive 起動ファイルを再現的に source する。
const BASH_SOURCE_USER: &str = r#"if [ -f "$HOME/.bash_profile" ]; then source "$HOME/.bash_profile";
elif [ -f "$HOME/.bash_login" ]; then source "$HOME/.bash_login";
elif [ -f "$HOME/.profile" ]; then source "$HOME/.profile"; fi
[ -f "$HOME/.bashrc" ] && source "$HOME/.bashrc""#;

/// bash の PROMPT_COMMAND / DEBUG trap による OSC 133 / OSC 7 発行（best-effort）。
const BASH_INTEGRATION: &str = r#"# --- tanaterm shell integration (OSC 133 / OSC 7) ---
__tanaterm_osc7() { printf '\e]7;file://%s%s\a' "${HOSTNAME}" "${PWD}"; }
__tanaterm_precmd() {
  local __exit=$?
  printf '\e]133;D;%s\a' "$__exit"
  printf '\e]133;A\a'
  __tanaterm_osc7
  __tanaterm_at_prompt=1
}
__tanaterm_preexec() {
  [ -n "$__tanaterm_at_prompt" ] || return
  unset __tanaterm_at_prompt
  printf '\e]133;C\a'
}
case "$PROMPT_COMMAND" in
  *__tanaterm_precmd*) ;;
  *) PROMPT_COMMAND="__tanaterm_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac
trap '__tanaterm_preexec' DEBUG
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_prepare_writes_rc_files_and_sets_zdotdir() {
        let int = prepare(Shell::Zsh).expect("prepare zsh");
        assert!(int.dir.join(".zshrc").exists());
        assert!(int.dir.join(".zshenv").exists());
        let zdotdir = int.env.iter().find(|(k, _)| k == "ZDOTDIR");
        assert!(zdotdir.is_some(), "ZDOTDIR が設定される");
        let zshrc = std::fs::read_to_string(int.dir.join(".zshrc")).unwrap();
        assert!(zshrc.contains("133;C"), "preexec で C を出す");
        assert!(zshrc.contains("source \"$HOME/.zshrc\""), "ユーザ rc を source");
        assert!(!int.suppress_login);
    }

    #[test]
    fn bash_prepare_uses_rcfile_and_suppresses_login() {
        let int = prepare(Shell::Bash).expect("prepare bash");
        assert!(int.suppress_login, "bash は --rcfile のため login を外す");
        assert_eq!(int.args[0], "--rcfile");
        let rc = std::fs::read_to_string(&int.args[1]).unwrap();
        assert!(rc.contains("PROMPT_COMMAND"));
        assert!(rc.contains("source \"$HOME/.bashrc\""));
    }

    #[test]
    fn drop_removes_temp_dir() {
        let dir = {
            let int = prepare(Shell::Zsh).expect("prepare");
            int.dir.clone()
        };
        assert!(!dir.exists(), "Drop で一時 dir が消える");
    }
}
