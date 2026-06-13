use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which shell tanaterm launches inside the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Bash,
    /// zsh is the macOS default since Catalina.
    #[default]
    Zsh,
}

impl Shell {
    /// Absolute path of the shell executable.
    pub fn program(self) -> &'static str {
        match self {
            Shell::Bash => "/bin/bash",
            Shell::Zsh => "/bin/zsh",
        }
    }
}

/// 初回起動時に書き出す config.toml のテンプレート。
///
/// `shell` / `font_size` は「初回起動時の既定値」であり、UI で変更した値が優先されることを
/// コメントで明示する。`zsh_path` / `bash_path` はコメントアウト済みの例として提示する。
const DEFAULT_CONFIG_TOML: &str = r#"# tanaterm 設定ファイル
#
# shell / font_size は「初回起動時の既定値」です。起動後に UI（シェル pill、⌘+/−）で
# 変更した値が優先され、以後この設定より UI での変更が勝ちます。
shell = "zsh"
font_size = 14.0
login_shell = true

# シェル実行ファイルのパス上書き（Homebrew 等のシェルを使う場合に指定）。
# 未指定なら /bin/zsh, /bin/bash を使います。
# zsh_path = "/opt/homebrew/bin/zsh"
# bash_path = "/opt/homebrew/bin/bash"
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Shell to run: `bash` or `zsh`.
    pub shell: Shell,
    /// Terminal font size in points.
    pub font_size: f32,
    /// Start the shell as a login shell (loads ~/.zprofile, ~/.bash_profile, ...).
    pub login_shell: bool,
    /// zsh 実行ファイルのパス上書き。指定かつ実在する場合のみ使用する。
    #[serde(default)]
    pub zsh_path: Option<String>,
    /// bash 実行ファイルのパス上書き。指定かつ実在する場合のみ使用する。
    #[serde(default)]
    pub bash_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: Shell::default(),
            font_size: 14.0,
            login_shell: true,
            zsh_path: None,
            bash_path: None,
        }
    }
}

impl Config {
    /// Location of the config file (e.g. `~/Library/Application Support/tanaterm/config.toml`).
    pub fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "tanaterm")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Load the config, falling back to defaults. Writes a default file the
    /// first time so users have something to edit.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };

        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => config,
                Err(err) => {
                    eprintln!(
                        "tanaterm: failed to parse {}: {err}; using defaults",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(_) => {
                let config = Self::default();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                // 初回生成はコメント付きテンプレートで書き出す（toml::to_string_pretty は
                // コメントを出力できないため静的文字列を使う）。
                let _ = std::fs::write(&path, DEFAULT_CONFIG_TOML);
                eprintln!("tanaterm: wrote default config to {}", path.display());
                config
            }
        }
    }

    /// 選択シェルの実行パスを返す。
    ///
    /// `zsh_path` / `bash_path` の上書き指定があり、かつそのファイルが実在する場合のみ
    /// 上書きパスを使う。実在しない場合は `eprintln` で警告して既定パス（`Shell::program()`）
    /// にフォールバックする。
    pub fn shell_program(&self, shell: Shell) -> PathBuf {
        self.shell_program_with_check(shell, std::path::Path::is_file)
    }

    /// パス存在チェック関数を注入できる内部実装（テスト容易性のため分離）。
    fn shell_program_with_check(
        &self,
        shell: Shell,
        is_file: impl Fn(&std::path::Path) -> bool,
    ) -> PathBuf {
        let override_path = match shell {
            Shell::Zsh => self.zsh_path.as_deref(),
            Shell::Bash => self.bash_path.as_deref(),
        };
        if let Some(p) = override_path {
            let path = PathBuf::from(p);
            if is_file(&path) {
                return path;
            }
            eprintln!(
                "tanaterm: {} は実在しません。既定パス {} を使います",
                p,
                shell.program()
            );
        }
        PathBuf::from(shell.program())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEFAULT_CONFIG_TOML が Config としてパースでき、
    /// default() の shell / font_size / login_shell と一致すること。
    #[test]
    fn default_config_toml_parses_to_default_values() {
        let parsed: Config =
            toml::from_str(DEFAULT_CONFIG_TOML).expect("DEFAULT_CONFIG_TOML のパースに失敗");
        let def = Config::default();
        assert_eq!(parsed.shell, def.shell, "shell が一致する");
        assert!(
            (parsed.font_size - def.font_size).abs() < f32::EPSILON,
            "font_size が一致する"
        );
        assert_eq!(
            parsed.login_shell, def.login_shell,
            "login_shell が一致する"
        );
    }

    /// zsh_path に実在するパスを指定した場合、そのパスが返る。
    #[test]
    fn shell_program_uses_override_when_file_exists() {
        let config = Config {
            zsh_path: Some("/bin/sh".to_string()),
            ..Default::default()
        };
        // テスト環境で必ず存在するパスとして /bin/sh を使う。
        let result =
            config.shell_program_with_check(Shell::Zsh, |p| p == std::path::Path::new("/bin/sh"));
        assert_eq!(result, PathBuf::from("/bin/sh"), "上書きパスが返る");
    }

    /// zsh_path に実在しないパスを指定した場合、既定の /bin/zsh にフォールバックする。
    #[test]
    fn shell_program_falls_back_to_default_when_file_missing() {
        let config = Config {
            zsh_path: Some("/nonexistent/zsh".to_string()),
            ..Default::default()
        };
        let result = config.shell_program_with_check(Shell::Zsh, |_| false);
        assert_eq!(
            result,
            PathBuf::from("/bin/zsh"),
            "実在しないパスは既定にフォールバックする"
        );
    }
}
