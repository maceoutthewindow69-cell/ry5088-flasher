//! ratatui TUI: detect the connected RY5088 keyboard, show its matching stock image, confirm, and flash
//! with a live progress gauge. The flash runs inline on the UI thread, redrawing the gauge per chunk.

use crate::{device::Dev, devmap, fetch, firmware, proto};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::path::PathBuf;
use std::time::Duration;

struct ImageInfo {
    path: PathBuf,
    slice: Vec<u8>,
    chunks: usize,
    cks: u32,
}

enum Screen {
    Main,
    Confirm,
    Result(bool, String),
}

struct App {
    dev: Dev,
    stock_dir: PathBuf,
    screen: Screen,
    info: Option<(u16, String)>,
    image: Option<ImageInfo>,
    note: String,
}

pub fn run(stock_dir: PathBuf) -> Result<(), String> {
    let dev = Dev::new()?;
    let mut term = ratatui::init();
    let mut app = App { dev, stock_dir, screen: Screen::Main, info: None, image: None, note: String::new() };
    app.detect();
    let res = app.main_loop(&mut term);
    ratatui::restore();
    res
}

impl App {
    fn detect(&mut self) {
        self.dev.refresh();
        self.info = self.dev.read_infor();
        self.image = None;
        self.note.clear();
        match self.info {
            Some((id, _)) => {
                // Refuse known non-RY5088 (sibling-platform) boards before fetching — their firmware
                // is not an AT32F405 keyboard image this flasher can write.
                if let Some(r) = devmap::lookup(id) {
                    if !r.is_ry5088() {
                        self.note = format!(
                            "dev {id} = {} is a {} board, not RY5088 (AT32F405) — not flashable here.",
                            r.display_name, r.platform);
                        return;
                    }
                }
                // Resolve a local image, or download the matching one from the vendor cloud (self-contained).
                let path = match firmware::local_image(&self.stock_dir, id) {
                    Some(p) => Some(p),
                    None => match fetch::fetch_to(id, &self.stock_dir) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            self.note = format!("No local image for dev {id}, and download failed: {e}");
                            None
                        }
                    },
                };
                if let Some(p) = path {
                    match firmware::load_slice(&p) {
                        Ok(s) => {
                            let cks = proto::fw_checksum(&s);
                            let chunks = s.len().div_ceil(64);
                            self.image = Some(ImageInfo { path: p, slice: s, chunks, cks });
                        }
                        Err(e) => self.note = e,
                    }
                }
            }
            None => self.note = "No keyboard detected. Plug in wired (slider on Middle) and close the vendor app.".into(),
        }
    }

    fn main_loop(&mut self, term: &mut DefaultTerminal) -> Result<(), String> {
        loop {
            term.draw(|f| self.render(f)).map_err(|e| e.to_string())?;
            if event::poll(Duration::from_millis(250)).map_err(|e| e.to_string())? {
                if let Event::Key(k) = event::read().map_err(|e| e.to_string())? {
                    if k.kind == KeyEventKind::Release {
                        continue;
                    }
                    match self.screen {
                        Screen::Main => match k.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('d') => self.detect(),
                            KeyCode::Char('f') => {
                                if self.image.is_some() {
                                    self.screen = Screen::Confirm;
                                }
                            }
                            _ => {}
                        },
                        Screen::Confirm => match k.code {
                            KeyCode::Char('y') => {
                                let r = self.do_flash(term);
                                self.screen = match r {
                                    Ok(m) => Screen::Result(true, m),
                                    Err(m) => Screen::Result(false, m),
                                };
                                self.detect();
                            }
                            KeyCode::Char('n') | KeyCode::Esc => self.screen = Screen::Main,
                            _ => {}
                        },
                        Screen::Result(..) => match k.code {
                            KeyCode::Char('q') => return Ok(()),
                            _ => self.screen = Screen::Main,
                        },
                    }
                }
            }
        }
    }

    fn do_flash(&mut self, term: &mut DefaultTerminal) -> Result<String, String> {
        let (slice, expect) = {
            let img = self.image.as_ref().ok_or("no image loaded")?;
            (img.slice.clone(), self.info.as_ref().map(|(id, _)| *id).unwrap_or(0))
        };
        if !self.dev.boot_present() {
            let _ = term.draw(|f| flashing_screen(f, "Entering bootloader (wiping config)…", None));
            self.dev.enter_bootloader()?;
        }
        let res = self.dev.flash_slice(&slice, |done, tot| {
            let _ = term.draw(|f| flashing_screen(f, "Flashing stock firmware…", Some((done, tot))));
        });
        match res {
            Ok(Some(false)) => return Err("Checksum mismatch (ACK 0xAA) — board stays in 502A, safe to retry.".into()),
            Err(e) => return Err(e),
            _ => {}
        }
        let _ = term.draw(|f| flashing_screen(f, "Verifying re-enumeration…", None));
        if self.dev.wait_normal() {
            match self.dev.read_infor() {
                Some((id2, v2)) if id2 != expect => Err(format!(
                    "Post-flash MISMATCH: flashed dev {expect} but the board now reports {id2} {v2} \
                     — variant-safety failure; flash the correct image.")),
                Some((id2, v2)) => Ok(format!(
                    "Flash OK — keyboard back online. Now: dev {id2} {v2} (MATCH ✓). \
                     Config was wiped: set actuation + run calibration in the vendor app.")),
                None => Ok("Flash OK — keyboard back online (dev_id unread). \
                            Config was wiped: set actuation + run calibration in the vendor app.".into()),
            }
        } else {
            Err("Did not return to normal mode (likely still in 502A — just flash again).".into())
        }
    }

    fn render(&self, f: &mut Frame) {
        let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(6), Constraint::Length(3)]).split(f.area());
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("ry-flash", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("  ·  RY5088 stock-firmware flasher (RongYuan magnetic)"),
            ]))
            .block(Block::default().borders(Borders::ALL)),
            rows[0],
        );
        match &self.screen {
            Screen::Main => self.render_main(f, rows[1]),
            Screen::Confirm => render_confirm(f, rows[1], self.image.as_ref()),
            Screen::Result(ok, msg) => render_result(f, rows[1], *ok, msg),
        }
        let hints = match self.screen {
            Screen::Main => "[d] re-detect   [f] flash stock   [q] quit",
            Screen::Confirm => "[y] yes, flash   [n] cancel",
            Screen::Result(..) => "[any] back   [q] quit",
        };
        f.render_widget(Paragraph::new(hints).block(Block::default().borders(Borders::ALL)), rows[2]);
    }

    fn render_main(&self, f: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        match &self.info {
            Some((id, ver)) => {
                lines.push(Line::from(vec![
                    Span::raw("Detected:  "),
                    Span::styled(devmap::label(*id), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(format!("           dev_id {id}   firmware {ver}")));
            }
            None => lines.push(Line::styled("No keyboard detected.", Style::default().fg(Color::Yellow))),
        }
        lines.push(Line::raw(""));
        match &self.image {
            Some(img) => {
                lines.push(Line::from(vec![
                    Span::raw("Stock image: "),
                    Span::styled(
                        img.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                        Style::default().fg(Color::Green),
                    ),
                ]));
                lines.push(Line::raw(format!(
                    "           {} bytes · {} chunks · checksum {:#08x}",
                    img.slice.len(),
                    img.chunks,
                    img.cks
                )));
                lines.push(Line::raw(""));
                lines.push(Line::styled("Press [f] to flash stock firmware (config will be wiped).", Style::default().fg(Color::Cyan)));
            }
            None => {
                if !self.note.is_empty() {
                    lines.push(Line::styled(self.note.clone(), Style::default().fg(Color::Yellow)));
                }
            }
        }
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).title(" device ")),
            area,
        );
    }
}

fn render_confirm(f: &mut Frame, area: Rect, image: Option<&ImageInfo>) {
    let model = image.map(|i| i.path.display().to_string()).unwrap_or_default();
    let lines = vec![
        Line::styled("Flash stock firmware?", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::raw("This will:"),
        Line::raw("  • WIPE the app config (keymap, RGB, actuation — re-settable in the app)"),
        Line::raw("  • erase + reflash the app region, then reboot"),
        Line::raw(""),
        Line::raw(format!("Image: {model}")),
        Line::raw(""),
        Line::styled("Recoverable: any error leaves it in the bootloader — just flash again.", Style::default().fg(Color::Green)),
        Line::raw(""),
        Line::styled("Press [y] to flash, [n] to cancel.", Style::default().add_modifier(Modifier::BOLD)),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).title(" confirm ")),
        area,
    );
}

fn render_result(f: &mut Frame, area: Rect, ok: bool, msg: &str) {
    let (color, title) = if ok { (Color::Green, " success ") } else { (Color::Red, " failed ") };
    f.render_widget(
        Paragraph::new(msg.to_string())
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

/// Full-screen flashing view with an optional progress gauge.
fn flashing_screen(f: &mut Frame, msg: &str, progress: Option<(usize, usize)>) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)]).split(f.area());
    f.render_widget(
        Paragraph::new(Line::styled(msg, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
            .block(Block::default().borders(Borders::ALL).title(" flashing ")),
        rows[0],
    );
    if let Some((done, tot)) = progress {
        let ratio = if tot > 0 { done as f64 / tot as f64 } else { 0.0 };
        f.render_widget(
            Gauge::default()
                .block(Block::default().borders(Borders::ALL))
                .gauge_style(Style::default().fg(Color::Green))
                .ratio(ratio.clamp(0.0, 1.0))
                .label(format!("{done}/{tot} chunks")),
            rows[1],
        );
    }
    f.render_widget(
        Paragraph::new("Do not unplug the keyboard.").style(Style::default().fg(Color::Yellow)),
        rows[2],
    );
}
