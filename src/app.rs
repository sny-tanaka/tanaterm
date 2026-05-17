use eframe::egui;

use crate::config::Config;
use crate::pty::PtySession;

pub struct TanaTermApp {
    config: Config,
    session: Option<PtySession>,
}

impl TanaTermApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        Self {
            config,
            session: None,
        }
    }
}

impl eframe::App for TanaTermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let font_id = egui::FontId::monospace(self.config.font_size);
        let (char_w, line_h) =
            ctx.fonts(|fonts| (fonts.glyph_width(&font_id, 'M'), fonts.row_height(&font_id)));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(0x0d, 0x0d, 0x0d)))
            .show(ctx, |ui| {
                let avail = ui.available_size();
                let cols = ((avail.x / char_w).floor() as u16).max(1);
                let rows = ((avail.y / line_h).floor() as u16).max(1);

                // Lazily spawn the shell once we know the widget size, then
                // keep the PTY in sync with the window.
                match self.session.as_mut() {
                    Some(session) => session.resize(rows, cols),
                    None => match PtySession::spawn(&self.config, rows, cols, ctx.clone()) {
                        Ok(session) => self.session = Some(session),
                        Err(err) => {
                            ui.colored_label(
                                egui::Color32::LIGHT_RED,
                                format!("Failed to start shell: {err}"),
                            );
                            return;
                        }
                    },
                }

                let input = collect_input(ctx);
                if !input.is_empty() {
                    if let Some(session) = self.session.as_mut() {
                        session.send(&input);
                    }
                }

                self.paint_screen(ui, &font_id, char_w, line_h);
            });
    }
}

impl TanaTermApp {
    fn paint_screen(
        &mut self,
        ui: &mut egui::Ui,
        font_id: &egui::FontId,
        char_w: f32,
        line_h: f32,
    ) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Ok(parser) = session.parser.lock() else {
            return;
        };
        let screen = parser.screen();
        let (rows, cols) = screen.size();

        let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::hover());
        let origin = response.rect.min;

        for row in 0..rows {
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else {
                    continue;
                };
                let pos = egui::pos2(
                    origin.x + col as f32 * char_w,
                    origin.y + row as f32 * line_h,
                );

                let bg = to_color(cell.bgcolor());
                if let Some(bg) = bg {
                    painter.rect_filled(
                        egui::Rect::from_min_size(pos, egui::vec2(char_w, line_h)),
                        0.0,
                        bg,
                    );
                }

                if cell.has_contents() {
                    let fg = to_color(cell.fgcolor())
                        .unwrap_or(egui::Color32::from_rgb(0xe0, 0xe0, 0xe0));
                    painter.text(
                        pos,
                        egui::Align2::LEFT_TOP,
                        cell.contents(),
                        font_id.clone(),
                        fg,
                    );
                }
            }
        }

        if !screen.hide_cursor() {
            let (crow, ccol) = screen.cursor_position();
            let pos = egui::pos2(
                origin.x + ccol as f32 * char_w,
                origin.y + crow as f32 * line_h,
            );
            painter.rect_filled(
                egui::Rect::from_min_size(pos, egui::vec2(char_w, line_h)),
                0.0,
                egui::Color32::from_rgba_unmultiplied(0xe0, 0xe0, 0xe0, 110),
            );
        }
    }
}

/// Translate the keyboard/clipboard events for this frame into the byte
/// sequence a shell expects.
fn collect_input(ctx: &egui::Context) -> Vec<u8> {
    let mut out = Vec::new();
    ctx.input(|i| {
        for event in &i.events {
            match event {
                egui::Event::Text(text) => out.extend_from_slice(text.as_bytes()),
                egui::Event::Paste(text) => out.extend_from_slice(text.as_bytes()),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if modifiers.ctrl || modifiers.command {
                        if let Some(byte) = ctrl_byte(*key) {
                            out.push(byte);
                            continue;
                        }
                    }
                    if let Some(seq) = key_sequence(*key) {
                        out.extend_from_slice(seq);
                    }
                }
                _ => {}
            }
        }
    });
    out
}

/// Control characters for Ctrl+<letter> (and a few common combos).
fn ctrl_byte(key: egui::Key) -> Option<u8> {
    use egui::Key::*;
    let letter = match key {
        A => b'a',
        B => b'b',
        C => b'c',
        D => b'd',
        E => b'e',
        F => b'f',
        G => b'g',
        H => b'h',
        I => b'i',
        J => b'j',
        K => b'k',
        L => b'l',
        M => b'm',
        N => b'n',
        O => b'o',
        P => b'p',
        Q => b'q',
        R => b'r',
        S => b's',
        T => b't',
        U => b'u',
        V => b'v',
        W => b'w',
        X => b'x',
        Y => b'y',
        Z => b'z',
        _ => return None,
    };
    Some(letter & 0x1f)
}

/// Escape sequences for non-text keys.
fn key_sequence(key: egui::Key) -> Option<&'static [u8]> {
    use egui::Key::*;
    Some(match key {
        Enter => b"\r",
        Backspace => b"\x7f",
        Tab => b"\t",
        Escape => b"\x1b",
        ArrowUp => b"\x1b[A",
        ArrowDown => b"\x1b[B",
        ArrowRight => b"\x1b[C",
        ArrowLeft => b"\x1b[D",
        Home => b"\x1b[H",
        End => b"\x1b[F",
        Delete => b"\x1b[3~",
        PageUp => b"\x1b[5~",
        PageDown => b"\x1b[6~",
        _ => return None,
    })
}

/// Map a vt100 color to an egui color. `None` means "use the terminal default"
/// (transparent background / default foreground).
fn to_color(color: vt100::Color) -> Option<egui::Color32> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Rgb(r, g, b) => Some(egui::Color32::from_rgb(r, g, b)),
        vt100::Color::Idx(idx) => Some(ansi_256(idx)),
    }
}

/// Standard xterm 256-color palette.
fn ansi_256(idx: u8) -> egui::Color32 {
    const BASE: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0xcd, 0x00, 0x00),
        (0x00, 0xcd, 0x00),
        (0xcd, 0xcd, 0x00),
        (0x00, 0x00, 0xee),
        (0xcd, 0x00, 0xcd),
        (0x00, 0xcd, 0xcd),
        (0xe5, 0xe5, 0xe5),
        (0x7f, 0x7f, 0x7f),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x5c, 0x5c, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];

    if idx < 16 {
        let (r, g, b) = BASE[idx as usize];
        return egui::Color32::from_rgb(r, g, b);
    }
    if idx < 232 {
        let i = idx - 16;
        let levels = [0u8, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
        let r = levels[(i / 36) as usize];
        let g = levels[((i / 6) % 6) as usize];
        let b = levels[(i % 6) as usize];
        return egui::Color32::from_rgb(r, g, b);
    }
    let v = 8 + 10 * (idx - 232);
    egui::Color32::from_rgb(v, v, v)
}
