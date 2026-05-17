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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Shell to run: `bash` or `zsh`.
    pub shell: Shell,
    /// Terminal font size in points.
    pub font_size: f32,
    /// Start the shell as a login shell (loads ~/.zprofile, ~/.bash_profile, ...).
    pub login_shell: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: Shell::default(),
            font_size: 14.0,
            login_shell: true,
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
                if let Ok(serialized) = toml::to_string_pretty(&config) {
                    let _ = std::fs::write(&path, serialized);
                }
                eprintln!("tanaterm: wrote default config to {}", path.display());
                config
            }
        }
    }
}
