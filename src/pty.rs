use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use eframe::egui;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

use crate::config::Config;

/// A running shell attached to a pseudo-terminal, plus the terminal-state
/// parser that turns raw shell output into a screen grid.
pub struct PtySession {
    pub parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    rows: u16,
    cols: u16,
}

impl PtySession {
    /// Spawn the configured shell on a PTY sized `rows` x `cols`. The reader
    /// thread repaints the egui context whenever new output arrives.
    pub fn spawn(
        config: &Config,
        rows: u16,
        cols: u16,
        ctx: egui::Context,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(config.shell.program());
        if config.login_shell {
            cmd.arg("-l");
        }
        cmd.env("TERM", "xterm-256color");
        if let Ok(home) = std::env::var("HOME") {
            cmd.cwd(home);
        }

        let child = pair.slave.spawn_command(cmd)?;
        // The slave handle is held by the child; we don't need our copy.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));

        {
            let parser = Arc::clone(&parser);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut parser) = parser.lock() {
                                parser.process(&buf[..n]);
                            }
                            ctx.request_repaint();
                        }
                    }
                }
            });
        }

        Ok(Self {
            parser,
            master: pair.master,
            writer,
            child,
            rows,
            cols,
        })
    }

    /// Resize the PTY and parser. No-op if the size is unchanged.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut parser) = self.parser.lock() {
            parser.set_size(rows, cols);
        }
    }

    /// Forward bytes (keystrokes) to the shell.
    pub fn send(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
