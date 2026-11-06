use std::collections::BTreeMap;
use stet::{config::Settings, document::Document, input::Bindings};

#[test]
fn editing_contract() -> anyhow::Result<()> {
    let mut doc = Document::blank("\r\n");
    doc.edit(0..0, "Stet\r\n世界", false)?;
    assert!(doc.dirty());
    assert_eq!(doc.line_text(1), "世界");
    assert_eq!(doc.boundary(6, false), 4);
    doc.undo(false);
    assert_eq!(doc.end(), 0);
    assert!(!doc.dirty());
    doc.undo(true);
    assert_eq!(doc.end(), 8);
    Ok(())
}

#[test]
fn defaults_are_valid() -> anyhow::Result<()> {
    Settings::default().validate()?;
    Bindings::new(BTreeMap::new())?;
    Ok(())
}

#[test]
fn logo_preserves_editing_and_handles_terminal_sizes() -> anyhow::Result<()> {
    use ratatui::{Terminal, backend::TestBackend};
    use stet::{app::App, ui::Ui};

    let directory = tempfile::tempdir()?;
    let mut app = App::new(directory.path().into(), Vec::new(), false)?;
    let mut ui = Ui::new();
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| ui.draw(frame, &mut app))?;
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.symbol() == "▀")
    );
    assert_eq!(app.doc().end(), 0);
    assert!(!app.doc().dirty());
    let cursor = terminal.get_cursor_position()?;
    assert_eq!(cursor.x, app.hits[0].text_x);
    assert_eq!(cursor.y, app.hits[0].area.y + 1);
    app.insert("hello", false)?;
    terminal.draw(|frame| ui.draw(frame, &mut app))?;
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.symbol() != "▀")
    );
    assert_eq!(app.doc().text.to_string(), "hello");
    app.command("undo")?;
    for theme in ["dark", "light", "terminal"] {
        app.settings.theme = theme.into();
        for (width, height) in [(0, 0), (1, 1), (12, 5), (30, 8), (40, 16), (100, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| ui.draw(frame, &mut app))?;
            assert_eq!(app.doc().end(), 0);
            assert!(!app.doc().dirty());
            if theme == "terminal" {
                for cell in &terminal.backend().buffer().content {
                    if matches!(cell.symbol(), "▀" | "▄" | "█") {
                        assert_eq!(cell.fg, ratatui::style::Color::Rgb(0, 113, 188));
                        assert_eq!(cell.bg, ratatui::style::Color::Reset);
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn existing_empty_files_have_no_logo() -> anyhow::Result<()> {
    use ratatui::{Terminal, backend::TestBackend};
    use stet::{app::App, ui::Ui};

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("empty.txt");
    std::fs::write(&path, "")?;
    let mut app = App::new(directory.path().join("config"), vec![path], false)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    let mut ui = Ui::new();
    terminal.draw(|frame| ui.draw(frame, &mut app))?;
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.symbol() != "▀")
    );
    Ok(())
}
