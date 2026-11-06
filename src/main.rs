use anyhow::{Context, Result, bail};
use crossterm::{
    cursor,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyEventKind,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;
use stet::{app::App, config, ui::Ui};

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()
            .context("Cannot enter raw mode. Run Stet in an interactive Windows console")?;
        let guard = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            cursor::Hide
        )?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    let _ = execute!(
        io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        cursor::Show,
        LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

fn run() -> Result<()> {
    let mut paths = Vec::new();
    let mut restore_session = true;
    let mut positional = false;
    for argument in std::env::args_os().skip(1) {
        if !positional {
            match argument.to_str() {
                Some("--version") => {
                    println!("stet {}", env!("CARGO_PKG_VERSION"));
                    return Ok(());
                }
                Some("--help" | "-h") => {
                    println!(
                        "Stet {}\n\nUsage: stet [--no-restore] [--] [FILE ...]\n\n--no-restore  Start without the previous session\n--version     Show version\n--help        Show this help\n\nCtrl+O Open  Ctrl+S Save  Ctrl+Q Quit  F2 Commands  F1 Help",
                        env!("CARGO_PKG_VERSION")
                    );
                    return Ok(());
                }
                Some("--no-restore") => {
                    restore_session = false;
                    continue;
                }
                Some("--") => {
                    positional = true;
                    continue;
                }
                Some(value) if value.starts_with('-') => {
                    bail!("Unknown option: {value}. Use -- before filenames beginning with '-'")
                }
                _ => {}
            }
        }
        paths.push(PathBuf::from(argument));
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Stet needs an interactive terminal on stdin and stdout. Run stet --help for usage");
    }
    let mut app = App::new(config::directory()?, paths, restore_session)?;
    let mut ui = Ui::new();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
    let guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    while app.running {
        if app.poll_background() {
            ui.follow_cursor = true;
        }
        terminal.draw(|frame| ui.draw(frame, &mut app))?;
        if (ui.pending || app.scan.is_some() || app.search_pending)
            && !event::poll(Duration::from_millis(40))?
        {
            continue;
        }
        let result = match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                ui.follow_cursor = true;
                app.notice.clear();
                app.key(key)
            }
            Event::Paste(value) => {
                ui.follow_cursor = true;
                app.paste(&value)
            }
            Event::Mouse(mouse) => {
                ui.follow_cursor = matches!(
                    mouse.kind,
                    event::MouseEventKind::Down(_) | event::MouseEventKind::Drag(_)
                );
                app.mouse(mouse)
            }
            Event::Resize(_, _) => {
                ui.follow_cursor = true;
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            app.notice = format!("{error:#}");
        }
    }
    let result = app.persist();
    drop(terminal);
    drop(guard);
    result.context("Could not save session metadata; document saves are unaffected")
}

fn main() {
    if let Err(error) = run() {
        eprintln!("stet: {error:#}");
        std::process::exit(1);
    }
}
