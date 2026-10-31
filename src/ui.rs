use crate::{
    app::{App, HitRow, PaneHit, PromptKind},
    document::{self, Document, wrapped_columns},
    input,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::collections::BTreeMap;
use std::ops::Range;
use syntect::{
    easy::HighlightLines,
    highlighting::{HighlightState, ThemeSet},
    parsing::{ParseState, SyntaxSet},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
struct Palette {
    background: Color,
    foreground: Color,
    muted: Color,
    panel: Color,
    active: Color,
    accent: Color,
    selection: Color,
}

impl Palette {
    fn new(theme: &str) -> Self {
        match theme {
            "light" => Self {
                background: Color::Rgb(249, 249, 246),
                foreground: Color::Rgb(35, 45, 52),
                muted: Color::Rgb(112, 118, 123),
                panel: Color::Rgb(235, 237, 233),
                active: Color::Rgb(229, 234, 231),
                accent: Color::Rgb(19, 112, 108),
                selection: Color::Rgb(183, 217, 218),
            },
            "terminal" => Self {
                background: Color::Reset,
                foreground: Color::Reset,
                muted: Color::DarkGray,
                panel: Color::Reset,
                active: Color::Reset,
                accent: Color::Cyan,
                selection: Color::Blue,
            },
            _ => Self {
                background: Color::Rgb(22, 27, 33),
                foreground: Color::Rgb(210, 219, 225),
                muted: Color::Rgb(106, 122, 137),
                panel: Color::Rgb(28, 35, 43),
                active: Color::Rgb(32, 42, 51),
                accent: Color::Rgb(114, 203, 182),
                selection: Color::Rgb(49, 78, 97),
            },
        }
    }
}

type Tokens = Vec<(Range<usize>, Color)>;
struct SyntaxCache {
    generation: u64,
    theme: String,
    language: String,
    lines: Vec<Tokens>,
    states: Vec<(HighlightState, ParseState)>,
}

pub struct Ui {
    syntaxes: SyntaxSet,
    themes: ThemeSet,
    cache: BTreeMap<usize, SyntaxCache>,
    pub pending: bool,
    pub follow_cursor: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    pub fn new() -> Self {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        for definition in [
            include_str!("../assets/typescript.sublime-syntax"),
            include_str!("../assets/toml.sublime-syntax"),
        ] {
            if let Ok(syntax) =
                syntect::parsing::SyntaxDefinition::load_from_str(definition, true, None)
            {
                builder.add(syntax);
            }
        }
        Self {
            syntaxes: builder.build(),
            themes: ThemeSet::load_defaults(),
            cache: BTreeMap::new(),
            pending: false,
            follow_cursor: true,
        }
    }

    pub fn language(&self, doc: &Document) -> String {
        let extension = doc
            .language
            .as_deref()
            .or_else(|| {
                doc.path
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|s| s.to_str())
            })
            .unwrap_or("plain");
        if extension == "plain" {
            return "Plain Text".into();
        }
        self.syntaxes
            .find_syntax_by_extension(extension)
            .or_else(|| self.syntaxes.find_syntax_by_token(extension))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Plain Text".into())
    }

    fn prepare_syntax(&mut self, id: usize, doc: &Document, end: usize, theme_name: &str) {
        if theme_name == "terminal"
            || doc.text.len_bytes() > 1024 * 1024
            || doc.text.len_lines() > 20000
            || !doc.highlight
        {
            self.cache.remove(&id);
            return;
        }
        let language = self.language(doc);
        let syntax = self
            .syntaxes
            .find_syntax_by_name(&language)
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let theme_key = if theme_name == "light" {
            "InspiredGitHub"
        } else {
            "base16-ocean.dark"
        };
        let Some(theme) = self.themes.themes.get(theme_key) else {
            return;
        };
        let cache = self.cache.entry(id).or_insert_with(|| SyntaxCache {
            generation: doc.generation,
            theme: theme_name.into(),
            language: language.clone(),
            lines: Vec::new(),
            states: Vec::new(),
        });
        if cache.generation != doc.generation
            || cache.theme != theme_name
            || cache.language != language
        {
            let keep = if cache.theme == theme_name
                && cache.language == language
                && cache.generation + 1 == doc.generation
            {
                doc.changed_line.min(cache.lines.len())
            } else {
                0
            };
            cache.lines.truncate(keep);
            cache.states.truncate(keep);
            cache.generation = doc.generation;
            cache.theme = theme_name.into();
            cache.language = language;
        }
        let mut highlighter = if let Some((highlight, parse)) = cache.states.last() {
            HighlightLines::from_state(theme, highlight.clone(), parse.clone())
        } else {
            HighlightLines::new(syntax, theme)
        };
        let end = end.min(doc.text.len_lines());
        let start = std::time::Instant::now();
        while cache.lines.len() < end {
            let line = doc.text.line(cache.lines.len()).to_string();
            let Ok(tokens) = highlighter.highlight_line(&line, &self.syntaxes) else {
                break;
            };
            let mut offset = 0;
            let tokens = tokens
                .into_iter()
                .map(|(style, text)| {
                    let first = offset;
                    offset += text.chars().count();
                    (
                        first..offset,
                        Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b),
                    )
                })
                .collect();
            cache.lines.push(tokens);
            let state = highlighter.state();
            highlighter = HighlightLines::from_state(theme, state.0.clone(), state.1.clone());
            cache.states.push(state);
            if start.elapsed().as_millis() >= 5 {
                break;
            }
        }
        self.pending |= cache.lines.len() < end;
    }

    pub fn draw(&mut self, frame: &mut Frame, app: &mut App) {
        self.cache
            .retain(|id, _| app.views.values().any(|view| view.document == *id));
        self.pending = false;
        let area = frame.area();
        let colors = Palette::new(&app.settings.theme);
        frame.render_widget(
            Block::default().style(Style::default().bg(colors.background).fg(colors.foreground)),
            area,
        );
        app.hits.clear();
        app.sidebar_rect = Rect::default();
        if area.width < 12 || area.height < 5 {
            write(
                frame,
                area,
                "Resize terminal",
                Style::default().fg(colors.accent),
            );
            return;
        }
        write(
            frame,
            Rect::new(1, 0, area.width.saturating_sub(2), 1),
            "stet  1.0.0",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        );
        let title = format!(
            "{}{}",
            app.doc().name(),
            if app.doc().dirty() { " *" } else { "" }
        );
        write(
            frame,
            Rect::new(18.min(area.width), 0, area.width.saturating_sub(19), 1),
            &title,
            Style::default().fg(colors.muted),
        );
        let mut editor = Rect::new(0, 2, area.width, area.height.saturating_sub(5));
        if app.settings.sidebar && area.width >= 70 {
            let width = app.settings.sidebar_width.min(area.width / 3);
            let sidebar = Rect::new(0, 2, width, editor.height);
            app.sidebar_rect = sidebar;
            frame.render_widget(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(colors.muted))
                    .style(Style::default().bg(colors.panel)),
                sidebar,
            );
            write(
                frame,
                Rect::new(1, 2, width.saturating_sub(2), 1),
                "OPEN FILES",
                Style::default().fg(if app.sidebar_focus {
                    colors.accent
                } else {
                    colors.muted
                }),
            );
            let capacity = sidebar.height.saturating_sub(2) as usize;
            if app.sidebar_focus {
                if app.sidebar_selected < app.sidebar_top {
                    app.sidebar_top = app.sidebar_selected;
                }
                if app.sidebar_selected >= app.sidebar_top + capacity {
                    app.sidebar_top = app
                        .sidebar_selected
                        .saturating_sub(capacity.saturating_sub(1));
                }
            }
            for (index, (id, doc)) in app
                .documents
                .iter()
                .enumerate()
                .skip(app.sidebar_top)
                .take(capacity)
            {
                let y = 3 + (index - app.sidebar_top) as u16;
                let text = format!(
                    "{} {}{}",
                    if *id == app.view().document {
                        "›"
                    } else {
                        " "
                    },
                    doc.name(),
                    if doc.dirty() { " *" } else { "" }
                );
                let style = Style::default()
                    .fg(if *id == app.view().document {
                        colors.accent
                    } else {
                        colors.foreground
                    })
                    .bg(if app.sidebar_focus && index == app.sidebar_selected {
                        colors.selection
                    } else {
                        colors.panel
                    });
                write(
                    frame,
                    Rect::new(1, y, width.saturating_sub(2), 1),
                    &text,
                    style,
                );
            }
            let open_y = sidebar.bottom().saturating_sub(1);
            write(
                frame,
                Rect::new(1, open_y, width.saturating_sub(2), 1),
                "+ Open file",
                Style::default().fg(colors.accent),
            );
            editor.x += width;
            editor.width -= width;
        }
        let mut rectangles = Vec::new();
        app.layout.rectangles(editor, &mut rectangles);
        if rectangles.iter().any(|(_, r)| r.width < 16 || r.height < 4) {
            rectangles = vec![(app.active, editor)];
        }
        for (id, rectangle) in rectangles {
            self.pane(frame, app, id, rectangle, colors);
        }
        let doc = app.doc();
        let status = format!(
            " {}  | Ln {}, Col {} | {} | {} | {} | {}:{}{}",
            if app.sidebar_focus { "FILES" } else { "EDIT" },
            doc.line_of(app.view().cursor) + 1,
            doc.column(app.view().cursor, app.settings.tab_width) + 1,
            self.language(doc),
            doc.encoding.name(),
            doc.endings(),
            if app.settings.spaces {
                "Spaces"
            } else {
                "Tabs"
            },
            app.settings.tab_width,
            if doc.dirty() { " | Modified" } else { "" }
        );
        write(
            frame,
            Rect::new(0, area.height - 3, area.width, 1),
            &status,
            Style::default().fg(colors.foreground).bg(colors.panel),
        );
        let notice = if app.notice.is_empty() {
            "^S Save   ^O Open   ^F Find   F2 Commands   F1 Help"
        } else {
            &app.notice
        };
        write(
            frame,
            Rect::new(1, area.height - 2, area.width.saturating_sub(2), 1),
            notice,
            Style::default().fg(colors.accent),
        );
        if app.prompt.is_some() {
            self.modal(frame, app, colors);
        }
        self.follow_cursor = false;
    }

    fn pane(&mut self, frame: &mut Frame, app: &mut App, id: usize, area: Rect, colors: Palette) {
        if area.width < 2 || area.height < 2 {
            return;
        }
        let view = &app.views[&id];
        let doc_id = view.document;
        let doc = &app.documents[&doc_id];
        let focused = app.active == id && !app.sidebar_focus;
        let title = format!(" {}{} ", doc.name(), if doc.dirty() { " *" } else { "" });
        write(
            frame,
            Rect::new(area.x, area.y, area.width, 1),
            &title,
            Style::default()
                .fg(if focused { colors.accent } else { colors.muted })
                .bg(colors.panel),
        );
        let gutter = if app.settings.line_numbers {
            (doc.text.len_lines().to_string().len() as u16 + 2).min(area.width.saturating_sub(2))
        } else {
            1
        };
        let text_x = area.x + gutter;
        let width = area.width.saturating_sub(gutter + 1).max(1) as usize;
        let height = area.height.saturating_sub(1) as usize;
        let line = doc.line_of(view.cursor);
        let column = doc.column(view.cursor, app.settings.tab_width);
        let wrap = app.settings.wrap;
        let mut top = view.top.min(doc.text.len_lines() - 1);
        let mut wrap_top = view.wrap_top;
        let mut left = view.left;
        if self.follow_cursor && focused {
            if line < top || line >= top + height {
                top = if line < top {
                    line
                } else {
                    line.saturating_sub(height / 2)
                };
                wrap_top = 0;
            }
            if wrap {
                left = 0;
                let columns = wrapped_columns(&doc.line_text(line), width, app.settings.tab_width);
                let cursor_row = columns.partition_point(|c| *c <= column).saturating_sub(1);
                let preceding: usize = (top..line)
                    .map(|index| {
                        wrapped_columns(&doc.line_text(index), width, app.settings.tab_width).len()
                    })
                    .take(height)
                    .sum();
                if preceding.saturating_sub(wrap_top) + cursor_row >= height {
                    top = line;
                    wrap_top = cursor_row.saturating_sub(height / 2);
                }
                if line == top {
                    let row = cursor_row;
                    if row < wrap_top {
                        wrap_top = row;
                    }
                    if row >= wrap_top + height {
                        wrap_top = row.saturating_sub(height - 1);
                    }
                } else {
                    wrap_top = 0;
                }
            } else {
                wrap_top = 0;
                if column < left {
                    left = column;
                }
                if column >= left + width {
                    left = column.saturating_sub(width - 1);
                }
            }
        }
        self.prepare_syntax(doc_id, doc, top + height + 1, &app.settings.theme);
        let selection = view.selection();
        let brackets = bracket_pair(doc, view.cursor);
        let mut rows = Vec::new();
        let mut y = area.y + 1;
        let mut cursor_position = None;
        for line_index in top..doc.text.len_lines() {
            if y >= area.bottom() {
                break;
            }
            let text = doc.line_text(line_index);
            let start = doc.line_start(line_index);
            let columns = if wrap {
                wrapped_columns(&text, width, app.settings.tab_width)
            } else {
                vec![left]
            };
            let segments = columns.len();
            for segment in if line_index == top {
                wrap_top.min(segments - 1)
            } else {
                0
            }..segments
            {
                if y >= area.bottom() {
                    break;
                }
                let first_column = columns[segment];
                let row_start = doc.at_column(line_index, first_column, app.settings.tab_width);
                let row_end =
                    doc.at_column(line_index, first_column + width, app.settings.tab_width);
                rows.push(HitRow {
                    y,
                    start: row_start,
                    end: row_end,
                    column: first_column,
                });
                let background = if line_index == line && focused {
                    colors.active
                } else {
                    colors.background
                };
                frame.render_widget(
                    Block::default().style(Style::default().bg(background)),
                    Rect::new(area.x, y, area.width, 1),
                );
                if app.settings.line_numbers && segment == 0 {
                    write(
                        frame,
                        Rect::new(area.x, y, gutter.saturating_sub(1), 1),
                        &format!(
                            "{:>width$}",
                            line_index + 1,
                            width = gutter.saturating_sub(1) as usize
                        ),
                        Style::default().fg(if line_index == line {
                            colors.accent
                        } else {
                            colors.muted
                        }),
                    );
                }
                let mut position = start;
                let mut cell = 0;
                let tokens = self
                    .cache
                    .get(&doc_id)
                    .and_then(|cache| cache.lines.get(line_index));
                for g in text.graphemes(true) {
                    let size = document::grapheme_width(g, cell, app.settings.tab_width);
                    if cell >= first_column + width {
                        break;
                    }
                    if cell + size > first_column
                        && cell >= first_column
                        && cell + size <= first_column + width
                    {
                        let x = text_x + (cell - first_column) as u16;
                        let local = position - start;
                        let foreground = tokens
                            .and_then(|tokens| {
                                tokens
                                    .iter()
                                    .find(|(range, _)| range.contains(&local))
                                    .map(|(_, c)| *c)
                            })
                            .unwrap_or(colors.foreground);
                        let selected = position >= selection.start && position < selection.end;
                        let match_index = app.matches.partition_point(|m| m.end <= position);
                        let search_match = doc_id == app.view().document
                            && app
                                .matches
                                .get(match_index)
                                .is_some_and(|m| m.contains(&position));
                        let bracket = brackets.is_some_and(|(a, b)| a == position || b == position);
                        let mut style = Style::default()
                            .fg(if bracket { colors.accent } else { foreground })
                            .bg(if selected {
                                colors.selection
                            } else {
                                background
                            });
                        if search_match {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        if bracket {
                            style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                        }
                        let symbol = if g == "\t" {
                            " ".repeat(size)
                        } else if g.chars().any(char::is_control) {
                            "�".into()
                        } else if unicode_width::UnicodeWidthStr::width(g) == 0 {
                            format!("◌{g}")
                        } else {
                            g.to_owned()
                        };
                        write(frame, Rect::new(x, y, size as u16, 1), &symbol, style);
                    }
                    position += g.chars().count();
                    cell += size;
                }
                if focused
                    && line_index == line
                    && column >= first_column
                    && column < first_column + width
                    && columns.get(segment + 1).is_none_or(|next| column < *next)
                {
                    cursor_position = Some((text_x + (column - first_column) as u16, y));
                }
                y += 1;
            }
        }
        if let Some(view) = app.views.get_mut(&id) {
            view.top = top;
            view.wrap_top = wrap_top;
            view.left = left;
        }
        if focused
            && app.prompt.is_none()
            && let Some(position) = cursor_position
        {
            frame.set_cursor_position(position);
        }
        app.hits.push(PaneHit {
            id,
            area,
            text_x,
            rows,
        });
    }

    fn modal(&self, frame: &mut Frame, app: &mut App, colors: Palette) {
        let Some(prompt) = app.prompt.as_mut() else {
            return;
        };
        let full = frame.area();
        let width = full.width.saturating_sub(6).min(90);
        let mut choices: Vec<String> = match &prompt.kind {
            PromptKind::Palette => input::COMMANDS
                .iter()
                .filter(|(_, label, _)| input::fuzzy(&prompt.text, label))
                .map(|(_, label, key)| format!("{label:<42} {key}"))
                .collect(),
            PromptKind::Help => input::COMMANDS
                .iter()
                .map(|(_, label, key)| format!("{key:<20} {label}"))
                .collect(),
            PromptKind::Quick => app
                .quick_files
                .iter()
                .filter(|path| input::fuzzy(&prompt.text, &path.to_string_lossy()))
                .map(|path| {
                    path.strip_prefix(&app.directory)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned()
                })
                .collect(),
            PromptKind::Dirty(_) => vec![app.documents[&app.views[&app.active].document].name()],
            PromptKind::Overwrite { path, .. } => vec![path.display().to_string()],
            _ => Vec::new(),
        };
        if matches!(prompt.kind, PromptKind::Quick) && choices.is_empty() {
            choices.push(
                if app.scan.is_some() {
                    "Scanning..."
                } else {
                    "No matching files"
                }
                .into(),
            );
        }
        let capacity = full.height.saturating_sub(8).clamp(1, 14) as usize;
        prompt.selected = prompt.selected.min(choices.len().saturating_sub(1));
        let offset = prompt.selected.saturating_sub(capacity - 1);
        let rows = choices.len().min(capacity);
        let height = (rows as u16 + 4).min(full.height);
        let area = Rect::new(
            (full.width - width) / 2,
            (full.height - height) / 3,
            width,
            height,
        );
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.accent))
                .style(Style::default().bg(colors.panel).fg(colors.foreground)),
            area,
        );
        write(
            frame,
            Rect::new(area.x + 1, area.y, width.saturating_sub(2), 1),
            prompt.title(),
            Style::default().fg(colors.accent),
        );
        let before = document::cells(&prompt.text[..prompt.caret], app.settings.tab_width);
        let input_width = width.saturating_sub(4) as usize;
        let skip = before.saturating_sub(input_width.saturating_sub(1));
        let mut cells = 0;
        let visible: String = prompt
            .text
            .graphemes(true)
            .filter(|g| {
                let start = cells;
                cells += document::grapheme_width(g, cells, app.settings.tab_width);
                start >= skip
            })
            .collect();
        write(
            frame,
            Rect::new(area.x + 2, area.y + 1, width.saturating_sub(4), 1),
            &visible,
            Style::default().fg(colors.foreground),
        );
        for (index, choice) in choices.iter().enumerate().skip(offset).take(capacity) {
            write(
                frame,
                Rect::new(
                    area.x + 2,
                    area.y + 2 + (index - offset) as u16,
                    width.saturating_sub(4),
                    1,
                ),
                choice,
                Style::default()
                    .fg(colors.foreground)
                    .bg(if index == prompt.selected {
                        colors.selection
                    } else {
                        colors.panel
                    }),
            );
        }
        if !matches!(
            prompt.kind,
            PromptKind::Dirty(_) | PromptKind::Overwrite { .. } | PromptKind::Help
        ) {
            frame.set_cursor_position((
                area.x
                    + 2
                    + before
                        .saturating_sub(skip)
                        .min(input_width.saturating_sub(1)) as u16,
                area.y + 1,
            ));
        }
    }
}

fn write(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    let safe: String = text
        .chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect();
    frame.render_widget(Paragraph::new(safe).style(style), area);
}

fn bracket_pair(doc: &Document, cursor: usize) -> Option<(usize, usize)> {
    let start = if cursor < doc.end() && "()[]{}".contains(doc.text.char(cursor)) {
        cursor
    } else if cursor > 0 && "()[]{}".contains(doc.text.char(cursor - 1)) {
        cursor - 1
    } else {
        return None;
    };
    let c = doc.text.char(start);
    let (open, close, forward) = match c {
        '(' | ')' => ('(', ')', c == '('),
        '[' | ']' => ('[', ']', c == '['),
        _ => ('{', '}', c == '{'),
    };
    let mut depth = 0;
    let mut position = start;
    for _ in 0..10000 {
        let current = doc.text.char(position);
        if current == if forward { open } else { close } {
            depth += 1;
        }
        if current == if forward { close } else { open } {
            depth -= 1;
            if depth == 0 {
                return Some((start, position));
            }
        }
        if forward {
            position += 1;
            if position >= doc.end() {
                break;
            }
        } else {
            if position == 0 {
                break;
            }
            position -= 1;
        }
    }
    None
}
