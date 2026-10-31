use crate::{
    config::{self, Session, Settings},
    document::{Delta, Document},
    files,
    input::{self, Bindings},
    panes::{Pane, View},
};
use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    None,
    Close,
    Quit,
    Reload,
}

pub enum PromptKind {
    Open,
    SaveAs,
    Find,
    ReplaceFind,
    ReplaceWith(String),
    Goto,
    Palette,
    Quick,
    Language,
    Dirty(After),
    Overwrite {
        path: PathBuf,
        revision: Option<files::Revision>,
    },
    Help,
}

pub struct Prompt {
    pub kind: PromptKind,
    pub text: String,
    pub caret: usize,
    pub selected: usize,
}

impl Prompt {
    pub fn title(&self) -> &str {
        match &self.kind {
            PromptKind::Open => "Open file | paste a quoted path",
            PromptKind::SaveAs => "Save As | enter a path",
            PromptKind::Find => "Find | Enter next | Alt+C case | Esc keep match",
            PromptKind::ReplaceFind => "Replace | find text",
            PromptKind::ReplaceWith(_) => "Replace all matches | replacement text",
            PromptKind::Goto => "Go to line",
            PromptKind::Palette => "Commands | type to filter",
            PromptKind::Quick => "Quick open | current directory, depth 4",
            PromptKind::Language => "Syntax | extension or plain",
            PromptKind::Dirty(_) => "Unsaved changes | S Save / D Discard / Esc Cancel",
            PromptKind::Overwrite { .. } => "File exists or changed | Y Overwrite / Esc Cancel",
            PromptKind::Help => "Shortcuts | Esc close | remap in keybindings.json",
        }
    }
}

pub struct HitRow {
    pub y: u16,
    pub start: usize,
    pub end: usize,
    pub column: usize,
}
pub struct PaneHit {
    pub id: usize,
    pub area: Rect,
    pub text_x: u16,
    pub rows: Vec<HitRow>,
}

struct SearchRequest {
    ticket: u64,
    text: ropey::Rope,
    query: String,
    sensitive: bool,
}
struct SearchResult {
    ticket: u64,
    matches: Vec<Range<usize>>,
}

pub struct App {
    pub documents: BTreeMap<usize, Document>,
    pub views: BTreeMap<usize, View>,
    pub layout: Pane,
    pub active: usize,
    pub settings: Settings,
    pub bindings: Bindings,
    pub prompt: Option<Prompt>,
    pub notice: String,
    pub sidebar_focus: bool,
    pub sidebar_selected: usize,
    pub sidebar_top: usize,
    pub sidebar_rect: Rect,
    pub hits: Vec<PaneHit>,
    pub query: String,
    pub sensitive: bool,
    pub matches: Vec<Range<usize>>,
    pub running: bool,
    pub directory: PathBuf,
    pub config_dir: PathBuf,
    pub quick_files: Vec<PathBuf>,
    pub scan: Option<Receiver<Vec<PathBuf>>>,
    pub pending_after: After,
    pub search_pending: bool,
    search_sender: Sender<SearchRequest>,
    search_results: Receiver<SearchResult>,
    search_signal: Arc<AtomicU64>,
    search_ticket: u64,
    discarded: BTreeSet<usize>,
    next_document: usize,
    next_view: usize,
    settings_writable: bool,
}

impl App {
    pub fn new(config_dir: PathBuf, paths: Vec<PathBuf>, restore: bool) -> Result<Self> {
        let mut notice = String::new();
        let (settings, settings_writable) =
            match config::load::<Settings>(&config_dir.join("settings.json")) {
                Ok(Some(settings)) => match settings.validate() {
                    Ok(()) => (settings, true),
                    Err(e) => {
                        notice =
                            format!("Invalid settings: {e}. Defaults active; original preserved");
                        (Settings::default(), false)
                    }
                },
                Ok(None) => (Settings::default(), true),
                Err(e) => {
                    notice =
                        format!("Cannot load settings: {e}. Defaults active; original preserved");
                    (Settings::default(), false)
                }
            };
        let custom = config::load::<BTreeMap<String, String>>(&config_dir.join("keybindings.json"));
        let bindings = match custom.and_then(|v| Bindings::new(v.unwrap_or_default())) {
            Ok(bindings) => bindings,
            Err(e) => {
                notice = format!("Invalid keybindings: {e}. Defaults active");
                Bindings::new(BTreeMap::new())?
            }
        };
        let (search_sender, requests) = mpsc::channel::<SearchRequest>();
        let (results, search_results) = mpsc::channel();
        let search_signal = Arc::new(AtomicU64::new(0));
        let signal = search_signal.clone();
        std::thread::spawn(move || {
            while let Ok(mut request) = requests.recv() {
                while let Ok(next) = requests.try_recv() {
                    request = next;
                }
                let mut doc = Document::blank("\n");
                doc.text = request.text;
                let matches = doc.matches_cancellable(&request.query, request.sensitive, || {
                    signal.load(Ordering::Relaxed) != request.ticket
                });
                if results
                    .send(SearchResult {
                        ticket: request.ticket,
                        matches,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut app = Self {
            documents: BTreeMap::new(),
            views: BTreeMap::from([(0, View::default())]),
            layout: Pane::default(),
            active: 0,
            settings,
            bindings,
            prompt: None,
            notice,
            sidebar_focus: false,
            sidebar_selected: 0,
            sidebar_top: 0,
            sidebar_rect: Rect::default(),
            hits: Vec::new(),
            query: String::new(),
            sensitive: true,
            matches: Vec::new(),
            running: true,
            directory: std::env::current_dir()?,
            config_dir,
            quick_files: Vec::new(),
            scan: None,
            search_pending: false,
            search_sender,
            search_results,
            search_signal,
            search_ticket: 0,
            pending_after: After::None,
            discarded: BTreeSet::new(),
            next_document: 0,
            next_view: 1,
            settings_writable,
        };
        if paths.is_empty() && restore && app.settings.restore_session {
            match config::load::<Session>(&app.config_dir.join("session.json")) {
                Ok(Some(session)) => {
                    if let Err(e) = app.restore(session) {
                        app.notice = format!("Session not restored: {e}");
                    }
                }
                Err(e) => app.notice = format!("Session not restored: {e}"),
                _ => {}
            }
        }
        for path in paths {
            if let Err(e) = app.open(&path) {
                app.notice = format!("Cannot open {}: {e}", path.display());
            }
        }
        if app.documents.is_empty() {
            app.new_document();
        }
        Ok(app)
    }

    pub fn view(&self) -> &View {
        &self.views[&self.active]
    }
    pub fn doc(&self) -> &Document {
        &self.documents[&self.view().document]
    }
    pub fn doc_mut(&mut self) -> &mut Document {
        let id = self.view().document;
        self.documents
            .get_mut(&id)
            .unwrap_or_else(|| unreachable!())
    }

    pub fn new_document(&mut self) {
        let id = self.next_document;
        self.next_document += 1;
        self.documents
            .insert(id, Document::blank(self.settings.newline()));
        self.views.insert(
            self.active,
            View {
                document: id,
                ..View::default()
            },
        );
        self.matches.clear();
    }

    pub fn open(&mut self, path: &Path) -> Result<usize> {
        let identity = files::identity(path)?;
        if let Some((&id, _)) = self.documents.iter().find(|(_, doc)| {
            doc.path
                .as_ref()
                .is_some_and(|p| files::path_key(p) == files::path_key(&identity))
        }) {
            self.activate_document(id);
            return Ok(id);
        }
        if self.documents.len() >= 256 {
            bail!("At most 256 files can be open");
        }
        let document = Document::open(&identity, self.settings.newline())?;
        let id = self.next_document;
        self.next_document += 1;
        self.documents.insert(id, document);
        self.activate_document(id);
        Ok(id)
    }

    pub fn activate_document(&mut self, id: usize) {
        if self.view().document != id {
            self.views.insert(
                self.active,
                View {
                    document: id,
                    ..View::default()
                },
            );
        }
        self.sidebar_focus = false;
        self.refresh_search();
    }

    pub fn restore(&mut self, session: Session) -> Result<()> {
        session.validate()?;
        let mut remap = BTreeMap::new();
        for (old, path) in session.paths {
            if !path.is_file() {
                self.notice = format!("Session file missing: {}", path.display());
                continue;
            }
            match self.open(&path) {
                Ok(id) => {
                    remap.insert(old, id);
                }
                Err(e) => self.notice = format!("Skipped {}: {e}", path.display()),
            }
        }
        if self.documents.is_empty() {
            self.new_document();
        }
        let fallback = self
            .documents
            .keys()
            .next()
            .copied()
            .context("No document")?;
        let mut views = session.views;
        for view in views.values_mut() {
            view.document = remap.get(&view.document).copied().unwrap_or(fallback);
            let doc = &self.documents[&view.document];
            view.cursor = view.cursor.min(doc.end());
            view.anchor = None;
            view.top = view.top.min(doc.text.len_lines() - 1);
            view.left = view.left.min(files::MAX_LINE_BYTES);
            view.wrap_top = view.wrap_top.min(files::MAX_LINE_BYTES);
        }
        self.active = session.active;
        self.next_view = views.keys().max().copied().unwrap_or(0).saturating_add(1);
        self.views = views;
        self.layout = session.layout;
        self.settings.sidebar = session.sidebar;
        self.settings.sidebar_width = session.sidebar_width;
        if session.directory.is_dir() {
            self.directory = session.directory;
        }
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        if self.settings_writable {
            config::store(&self.config_dir.join("settings.json"), &self.settings)?;
        }
        let session = Session {
            version: 1,
            paths: self
                .documents
                .iter()
                .filter_map(|(id, doc)| {
                    doc.path
                        .as_ref()
                        .filter(|p| p.is_file())
                        .map(|p| (*id, p.clone()))
                })
                .collect(),
            views: self.views.clone(),
            layout: self.layout.clone(),
            active: self.active,
            sidebar: self.settings.sidebar,
            sidebar_width: self.settings.sidebar_width,
            directory: self.directory.clone(),
        };
        config::store(&self.config_dir.join("session.json"), &session)
    }

    pub fn set_prompt(&mut self, kind: PromptKind, text: String) {
        self.doc_mut().stop_typing();
        let caret = text.len();
        self.prompt = Some(Prompt {
            kind,
            text,
            caret,
            selected: 0,
        });
    }

    pub fn insert(&mut self, value: &str, typing: bool) -> Result<()> {
        let range = self.view().selection();
        let cursor = range.start + value.chars().count();
        let delta = self.doc_mut().edit(range, value, typing)?;
        self.apply_delta(delta);
        if let Some(view) = self.views.get_mut(&self.active) {
            view.cursor = cursor;
            view.anchor = None;
            view.preferred_column = None;
        }
        self.refresh_search();
        Ok(())
    }

    fn apply_delta(&mut self, delta: Delta) {
        let document = self.view().document;
        for view in self
            .views
            .values_mut()
            .filter(|view| view.document == document)
        {
            view.apply(delta);
        }
    }

    pub fn refresh_search(&mut self) {
        self.search_ticket += 1;
        self.search_signal
            .store(self.search_ticket, Ordering::Relaxed);
        self.search_pending = false;
        self.matches.clear();
        if self.query.is_empty() || !self.documents.contains_key(&self.view().document) {
            return;
        }
        if self.doc().text.len_bytes() <= 1024 * 1024 {
            self.matches = self.doc().matches(&self.query, self.sensitive);
        } else {
            let request = SearchRequest {
                ticket: self.search_ticket,
                text: self.doc().text.clone(),
                query: self.query.clone(),
                sensitive: self.sensitive,
            };
            self.search_pending = self.search_sender.send(request).is_ok();
        }
    }

    fn find_next(&mut self, previous: bool, include_current: bool) {
        let cursor = self.view().cursor;
        let found = if previous {
            self.matches
                .iter()
                .rev()
                .find(|r| r.start < self.view().selection().start)
                .or_else(|| self.matches.last())
        } else {
            self.matches
                .iter()
                .find(|r| {
                    if include_current {
                        r.start >= cursor
                    } else {
                        r.start > cursor
                    }
                })
                .or_else(|| self.matches.first())
        }
        .cloned();
        if let Some(found) = found
            && let Some(view) = self.views.get_mut(&self.active)
        {
            view.anchor = Some(found.start);
            view.cursor = found.end;
            view.preferred_column = None;
        }
    }

    pub fn paste(&mut self, value: &str) -> Result<()> {
        if let Some(prompt) = self.prompt.as_mut() {
            if matches!(
                prompt.kind,
                PromptKind::Dirty(_) | PromptKind::Overwrite { .. } | PromptKind::Help
            ) {
                return Ok(());
            }
            let value: String = value
                .chars()
                .filter(|c| !c.is_control())
                .take(32768)
                .collect();
            if prompt.text.len() + value.len() > 65536 {
                bail!("Prompt input exceeds 64 KiB");
            }
            prompt.text.insert_str(prompt.caret, &value);
            prompt.caret += value.len();
            prompt.selected = 0;
            self.incremental();
            Ok(())
        } else {
            self.insert(value, false)
        }
    }

    pub fn command(&mut self, command: &str) -> Result<()> {
        match command {
            "new" => self.new_document(),
            "open" => self.set_prompt(PromptKind::Open, String::new()),
            "save" => self.save()?,
            "save_as" => self.set_prompt(
                PromptKind::SaveAs,
                self.doc()
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            "close" => self.request_after(After::Close)?,
            "quit" => {
                self.discarded.clear();
                self.request_after(After::Quit)?;
            }
            "reload" => self.request_after(After::Reload)?,
            "next" | "previous" => {
                let ids: Vec<_> = self.documents.keys().copied().collect();
                let index = ids
                    .iter()
                    .position(|id| *id == self.view().document)
                    .unwrap_or(0);
                let next = if command == "next" {
                    (index + 1) % ids.len()
                } else {
                    (index + ids.len() - 1) % ids.len()
                };
                self.activate_document(ids[next]);
            }
            "undo" | "redo" => {
                let deltas = self.doc_mut().undo(command == "redo");
                for delta in &deltas {
                    self.apply_delta(*delta);
                }
                if let Some(last) = deltas.last()
                    && let Some(view) = self.views.get_mut(&self.active)
                {
                    view.cursor = last.start + last.inserted;
                    view.anchor = None;
                }
                self.refresh_search();
            }
            "select_all" => {
                let end = self.doc().end();
                if let Some(view) = self.views.get_mut(&self.active) {
                    view.anchor = Some(0);
                    view.cursor = end;
                }
            }
            "copy" | "cut" => {
                let range = self.view().selection();
                if !range.is_empty() {
                    clipboard_set(&self.doc().selected(range))?;
                    if command == "cut" {
                        self.insert("", false)?;
                    }
                }
            }
            "paste" => self.insert(&clipboard_get()?, false)?,
            "find" => self.set_prompt(PromptKind::Find, self.query.clone()),
            "replace" => self.set_prompt(PromptKind::ReplaceFind, self.query.clone()),
            "goto" => self.set_prompt(PromptKind::Goto, String::new()),
            "palette" => self.set_prompt(PromptKind::Palette, String::new()),
            "help" => self.set_prompt(PromptKind::Help, String::new()),
            "language" => self.set_prompt(PromptKind::Language, String::new()),
            "quick" => {
                let directory = self.directory.clone();
                if directory.parent().is_none() {
                    bail!("Quick open is disabled at drive roots. Use Open file");
                }
                let (tx, rx) = mpsc::channel();
                self.scan = Some(rx);
                self.quick_files.clear();
                std::thread::spawn(move || {
                    let _ = tx.send(scan_files(&directory));
                });
                self.set_prompt(PromptKind::Quick, String::new());
            }
            "sidebar" => self.settings.sidebar = !self.settings.sidebar,
            "focus_sidebar" => {
                self.settings.sidebar = true;
                self.sidebar_focus = !self.sidebar_focus;
            }
            "split_vertical" | "split_horizontal" => {
                let vertical = command == "split_vertical";
                let area = self
                    .hits
                    .iter()
                    .find(|hit| hit.id == self.active)
                    .map(|hit| hit.area)
                    .unwrap_or(Rect::new(0, 0, 120, 40));
                if self.views.len() >= 8
                    || if vertical {
                        area.width < 40
                    } else {
                        area.height < 10
                    }
                {
                    bail!("Not enough room for another pane");
                }
                let id = self.next_view;
                self.next_view += 1;
                self.views.insert(id, self.view().clone());
                self.layout.split(self.active, id, vertical);
                self.active = id;
            }
            "close_pane" => {
                if self.views.len() > 1 {
                    self.layout.remove(self.active);
                    self.views.remove(&self.active);
                    self.active = self.layout.leaves()[0];
                }
            }
            "single_pane" => {
                let view = self.view().clone();
                self.views.clear();
                self.views.insert(self.active, view);
                self.layout = Pane::Leaf(self.active);
            }
            "pane_left" | "pane_right" | "pane_up" | "pane_down" => self.focus_direction(command),
            "theme" => {
                self.settings.theme = match self.settings.theme.as_str() {
                    "dark" => "light",
                    "light" => "terminal",
                    _ => "dark",
                }
                .into();
                if !self.settings_writable {
                    bail!("Theme changed for this run. Repair settings.json to enable persistence");
                }
                config::store(&self.config_dir.join("settings.json"), &self.settings)?;
            }
            "wrap" => {
                self.settings.wrap = !self.settings.wrap;
                for view in self.views.values_mut() {
                    view.wrap_top = 0;
                    view.left = 0;
                }
            }
            "case" => {
                self.sensitive = !self.sensitive;
                self.refresh_search();
                self.notice = format!("Search case sensitivity: {}", self.sensitive);
            }
            "next_match" => self.find_next(false, false),
            "previous_match" => self.find_next(true, false),
            "path" => {
                self.notice = self
                    .doc()
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Unsaved document has no path".into())
            }
            _ => bail!("Unknown command"),
        }
        Ok(())
    }

    fn focus_direction(&mut self, command: &str) {
        let Some(current) = self.hits.iter().find(|hit| hit.id == self.active) else {
            return;
        };
        let cx = current.area.x as i32 + current.area.width as i32 / 2;
        let cy = current.area.y as i32 + current.area.height as i32 / 2;
        let next = self
            .hits
            .iter()
            .filter(|hit| hit.id != self.active)
            .filter_map(|hit| {
                let dx = hit.area.x as i32 + hit.area.width as i32 / 2 - cx;
                let dy = hit.area.y as i32 + hit.area.height as i32 / 2 - cy;
                let valid = match command {
                    "pane_left" => dx < 0,
                    "pane_right" => dx > 0,
                    "pane_up" => dy < 0,
                    _ => dy > 0,
                };
                valid.then_some((dx.abs() + dy.abs(), hit.id))
            })
            .min();
        if let Some((_, id)) = next {
            self.active = id;
            self.sidebar_focus = false;
            self.refresh_search();
        }
    }

    fn request_after(&mut self, after: After) -> Result<()> {
        if after == After::Quit {
            if let Some(id) = self
                .documents
                .iter()
                .find(|(id, doc)| doc.dirty() && !self.discarded.contains(id))
                .map(|(id, _)| *id)
            {
                self.activate_document(id);
                self.set_prompt(PromptKind::Dirty(after), String::new());
                return Ok(());
            }
        } else if self.doc().dirty() {
            self.set_prompt(PromptKind::Dirty(after), String::new());
            return Ok(());
        }
        self.finish_after(after)
    }

    fn finish_after(&mut self, after: After) -> Result<()> {
        match after {
            After::Close => {
                let id = self.view().document;
                self.documents.remove(&id);
                if self.documents.is_empty() {
                    self.new_document();
                }
                let fallback = self
                    .documents
                    .keys()
                    .next()
                    .copied()
                    .context("No document")?;
                for view in self.views.values_mut().filter(|v| v.document == id) {
                    *view = View {
                        document: fallback,
                        ..View::default()
                    };
                }
                self.refresh_search();
            }
            After::Quit => self.running = false,
            After::Reload => {
                let path = self.doc().path.clone().context("Save the document first")?;
                let id = self.view().document;
                let document = Document::open(&path, self.settings.newline())?;
                self.documents.insert(id, document);
                let end = self.doc().end();
                for view in self.views.values_mut().filter(|v| v.document == id) {
                    view.cursor = view.cursor.min(end);
                    view.anchor = None;
                    view.top = 0;
                    view.wrap_top = 0;
                }
                self.refresh_search();
            }
            After::None => {}
        }
        Ok(())
    }

    fn saved(&mut self) -> Result<()> {
        self.notice = "Saved".into();
        let after = std::mem::replace(&mut self.pending_after, After::None);
        if after == After::Quit {
            self.request_after(after)
        } else {
            self.finish_after(after)
        }
    }

    fn save(&mut self) -> Result<()> {
        let Some(path) = self.doc().path.clone() else {
            self.set_prompt(PromptKind::SaveAs, String::new());
            return Ok(());
        };
        let expected = self.doc().disk.clone();
        let current = files::revision(&path)?;
        if current != expected {
            self.set_prompt(
                PromptKind::Overwrite {
                    path,
                    revision: current,
                },
                String::new(),
            );
            return Ok(());
        }
        self.doc_mut().save(&path, expected.as_ref())?;
        self.saved()
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<()> {
        if self.prompt.is_some() {
            return self.prompt_key(key);
        }
        self.pending_after = After::None;
        if let Some(command) = self.bindings.command(key).map(str::to_owned) {
            return self.command(&command);
        }
        if key.code == KeyCode::F(2) {
            return self.command("palette");
        }
        if self.sidebar_focus {
            match key.code {
                KeyCode::Up => self.sidebar_selected = self.sidebar_selected.saturating_sub(1),
                KeyCode::Down => {
                    self.sidebar_selected = (self.sidebar_selected + 1).min(self.documents.len())
                }
                KeyCode::Enter => {
                    if let Some(id) = self.documents.keys().nth(self.sidebar_selected).copied() {
                        self.activate_document(id);
                    } else {
                        self.command("open")?;
                    }
                }
                KeyCode::Esc | KeyCode::Tab => self.sidebar_focus = false,
                _ => {}
            }
            return Ok(());
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown => self.move_cursor(key.code, ctrl, shift),
            KeyCode::Backspace | KeyCode::Delete => {
                if self.view().selection().is_empty() {
                    let offset = if ctrl {
                        self.doc()
                            .word(self.view().cursor, key.code == KeyCode::Delete)
                    } else {
                        self.doc()
                            .boundary(self.view().cursor, key.code == KeyCode::Delete)
                    };
                    if let Some(view) = self.views.get_mut(&self.active) {
                        view.anchor = Some(offset);
                    }
                }
                self.insert("", false)?;
            }
            KeyCode::Enter => {
                let line = self.doc().line_text(self.doc().line_of(self.view().cursor));
                let indentation: String = line
                    .chars()
                    .take_while(|c| matches!(c, ' ' | '\t'))
                    .collect();
                let newline = format!("{}{indentation}", self.doc().newline);
                self.insert(&newline, false)?;
            }
            KeyCode::Tab => {
                let indentation = if self.settings.spaces {
                    " ".repeat(
                        self.settings.tab_width
                            - self
                                .doc()
                                .column(self.view().cursor, self.settings.tab_width)
                                % self.settings.tab_width,
                    )
                } else {
                    "\t".into()
                };
                self.insert(&indentation, false)?;
            }
            KeyCode::Esc => {
                if let Some(view) = self.views.get_mut(&self.active) {
                    view.anchor = None;
                }
            }
            KeyCode::Char(c)
                if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) && !c.is_control() =>
            {
                self.insert(&c.to_string(), true)?
            }
            _ => {}
        }
        Ok(())
    }

    fn move_cursor(&mut self, key: KeyCode, ctrl: bool, shift: bool) {
        self.doc_mut().stop_typing();
        let view = self.view();
        let doc = self.doc();
        let line = doc.line_of(view.cursor);
        let mut column = view
            .preferred_column
            .unwrap_or_else(|| doc.column(view.cursor, self.settings.tab_width));
        let page = self
            .hits
            .iter()
            .find(|h| h.id == self.active)
            .map(|h| h.area.height.saturating_sub(3) as usize)
            .unwrap_or(20)
            .max(1);
        let vertical = matches!(
            key,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
        );
        let cursor = if self.settings.wrap && vertical {
            let (cursor, preferred) = self.wrapped_vertical(key, page);
            column = preferred;
            cursor
        } else {
            match key {
                KeyCode::Left if !shift && !view.selection().is_empty() => view.selection().start,
                KeyCode::Right if !shift && !view.selection().is_empty() => view.selection().end,
                KeyCode::Left => {
                    if ctrl {
                        doc.word(view.cursor, false)
                    } else {
                        doc.boundary(view.cursor, false)
                    }
                }
                KeyCode::Right => {
                    if ctrl {
                        doc.word(view.cursor, true)
                    } else {
                        doc.boundary(view.cursor, true)
                    }
                }
                KeyCode::Home => {
                    if ctrl {
                        0
                    } else {
                        doc.line_start(line)
                    }
                }
                KeyCode::End => {
                    if ctrl {
                        doc.end()
                    } else {
                        doc.line_end(line)
                    }
                }
                KeyCode::Up | KeyCode::PageUp => doc.at_column(
                    line.saturating_sub(if key == KeyCode::Up { 1 } else { page }),
                    column,
                    self.settings.tab_width,
                ),
                KeyCode::Down | KeyCode::PageDown => doc.at_column(
                    (line + if key == KeyCode::Down { 1 } else { page })
                        .min(doc.text.len_lines() - 1),
                    column,
                    self.settings.tab_width,
                ),
                _ => view.cursor,
            }
        };
        if let Some(view) = self.views.get_mut(&self.active) {
            if shift {
                if view.anchor.is_none() {
                    view.anchor = Some(view.cursor);
                }
            } else {
                view.anchor = None;
            }
            view.cursor = cursor;
            view.preferred_column = if vertical { Some(column) } else { None };
        }
    }

    fn wrapped_vertical(&self, key: KeyCode, page: usize) -> (usize, usize) {
        let doc = self.doc();
        let view = self.view();
        let width = self
            .hits
            .iter()
            .find(|h| h.id == self.active)
            .map(|h| h.area.right().saturating_sub(h.text_x + 1) as usize)
            .unwrap_or(70)
            .max(1);
        let mut line = doc.line_of(view.cursor);
        let mut columns =
            crate::document::wrapped_columns(&doc.line_text(line), width, self.settings.tab_width);
        let column = doc.column(view.cursor, self.settings.tab_width);
        let mut row = columns.partition_point(|c| *c <= column).saturating_sub(1);
        let preferred = view.preferred_column.unwrap_or(column - columns[row]);
        let forward = matches!(key, KeyCode::Down | KeyCode::PageDown);
        for _ in 0..if matches!(key, KeyCode::PageUp | KeyCode::PageDown) {
            page
        } else {
            1
        } {
            if forward {
                if row + 1 < columns.len() {
                    row += 1;
                } else if line + 1 < doc.text.len_lines() {
                    line += 1;
                    columns = crate::document::wrapped_columns(
                        &doc.line_text(line),
                        width,
                        self.settings.tab_width,
                    );
                    row = 0;
                }
            } else if row > 0 {
                row -= 1;
            } else if line > 0 {
                line -= 1;
                columns = crate::document::wrapped_columns(
                    &doc.line_text(line),
                    width,
                    self.settings.tab_width,
                );
                row = columns.len() - 1;
            }
        }
        let end = columns
            .get(row + 1)
            .map(|c| c.saturating_sub(1))
            .unwrap_or(usize::MAX);
        (
            doc.at_column(
                line,
                (columns[row] + preferred).min(end),
                self.settings.tab_width,
            ),
            preferred,
        )
    }

    fn incremental(&mut self) {
        let origin = self.view().selection().start;
        if let Some(prompt) = &self.prompt
            && matches!(prompt.kind, PromptKind::Find)
        {
            self.query = prompt.text.clone();
            self.refresh_search();
            if let Some(view) = self.views.get_mut(&self.active) {
                view.cursor = origin;
                view.anchor = None;
            }
            self.find_next(false, true);
        }
    }

    fn prompt_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(mut prompt) = self.prompt.take() else {
            return Ok(());
        };
        if key.code == KeyCode::Esc {
            self.pending_after = After::None;
            return Ok(());
        }
        match &prompt.kind {
            PromptKind::Dirty(after) => {
                let after = *after;
                match key.code {
                    KeyCode::Char('s' | 'S') => {
                        self.pending_after = after;
                        self.save()?;
                    }
                    KeyCode::Char('d' | 'D') => {
                        if after == After::Quit {
                            self.discarded.insert(self.view().document);
                            self.request_after(after)?;
                        } else {
                            self.finish_after(after)?;
                        }
                    }
                    _ => self.prompt = Some(prompt),
                }
                return Ok(());
            }
            PromptKind::Overwrite { path, revision } => {
                if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                    self.doc_mut().save(path, revision.as_ref())?;
                    self.saved()?;
                } else {
                    self.prompt = Some(prompt);
                }
                return Ok(());
            }
            PromptKind::Help => {
                match key.code {
                    KeyCode::Up => prompt.selected = prompt.selected.saturating_sub(1),
                    KeyCode::Down => prompt.selected += 1,
                    KeyCode::PageUp => prompt.selected = prompt.selected.saturating_sub(10),
                    KeyCode::PageDown => prompt.selected += 10,
                    _ => {}
                }
                self.prompt = Some(prompt);
                return Ok(());
            }
            _ => {}
        }
        if key.code == KeyCode::Enter {
            let result = self.submit(&prompt);
            if result.is_err() && self.prompt.is_none() {
                self.prompt = Some(prompt);
            }
            return result;
        }
        if matches!(key.code, KeyCode::Up | KeyCode::Down) {
            prompt.selected = if key.code == KeyCode::Up {
                prompt.selected.saturating_sub(1)
            } else {
                prompt.selected + 1
            };
        } else if key.code == KeyCode::Left {
            prompt.caret = prompt.text[..prompt.caret]
                .grapheme_indices(true)
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        } else if key.code == KeyCode::Right {
            prompt.caret += prompt.text[prompt.caret..]
                .graphemes(true)
                .next()
                .map(str::len)
                .unwrap_or(0);
        } else if key.code == KeyCode::Home {
            prompt.caret = 0;
        } else if key.code == KeyCode::End {
            prompt.caret = prompt.text.len();
        } else if key.code == KeyCode::Backspace {
            let start = prompt.text[..prompt.caret]
                .grapheme_indices(true)
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            prompt.text.replace_range(start..prompt.caret, "");
            prompt.caret = start;
            prompt.selected = 0;
        } else if key.code == KeyCode::Delete {
            let end = prompt.caret
                + prompt.text[prompt.caret..]
                    .graphemes(true)
                    .next()
                    .map(str::len)
                    .unwrap_or(0);
            prompt.text.replace_range(prompt.caret..end, "");
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.prompt = Some(prompt);
            return self.paste(&clipboard_get()?);
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            prompt.text.clear();
            prompt.caret = 0;
        } else if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('c') {
            self.sensitive = !self.sensitive;
        } else if let KeyCode::Char(c) = key.code
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && !c.is_control()
            && prompt.text.len() < 65532
        {
            prompt.text.insert(prompt.caret, c);
            prompt.caret += c.len_utf8();
            prompt.selected = 0;
        }
        self.prompt = Some(prompt);
        self.incremental();
        Ok(())
    }

    fn submit(&mut self, prompt: &Prompt) -> Result<()> {
        match &prompt.kind {
            PromptKind::Open => {
                self.open(&files::parse_path(&prompt.text)?)?;
            }
            PromptKind::SaveAs => {
                let path = files::identity(&files::parse_path(&prompt.text)?)?;
                if self.documents.iter().any(|(id, doc)| {
                    *id != self.view().document
                        && doc
                            .path
                            .as_ref()
                            .is_some_and(|p| files::path_key(p) == files::path_key(&path))
                }) {
                    bail!("That file is open in another buffer");
                }
                let current = files::revision(&path)?;
                if current.is_some() {
                    self.set_prompt(
                        PromptKind::Overwrite {
                            path,
                            revision: current,
                        },
                        String::new(),
                    );
                } else {
                    self.doc_mut().save(&path, None)?;
                    self.saved()?;
                }
            }
            PromptKind::Find => {
                self.query = prompt.text.clone();
                self.refresh_search();
                self.find_next(false, false);
                self.prompt = Some(Prompt {
                    kind: PromptKind::Find,
                    text: prompt.text.clone(),
                    caret: prompt.caret,
                    selected: 0,
                });
            }
            PromptKind::ReplaceFind => {
                if prompt.text.is_empty() {
                    bail!("Enter text to find");
                }
                self.set_prompt(PromptKind::ReplaceWith(prompt.text.clone()), String::new());
            }
            PromptKind::ReplaceWith(needle) => {
                let sensitive = self.sensitive;
                let deltas = self
                    .doc_mut()
                    .replace_all(needle, &prompt.text, sensitive)?;
                let count = deltas.len();
                for delta in deltas {
                    self.apply_delta(delta);
                }
                self.notice = format!("Replaced {count} matches");
                self.refresh_search();
            }
            PromptKind::Goto => {
                let line: usize = prompt
                    .text
                    .parse()
                    .context("Enter a positive line number")?;
                if line == 0 || line > self.doc().text.len_lines() {
                    bail!("Line is outside the document");
                }
                let cursor = self.doc().line_start(line - 1);
                if let Some(view) = self.views.get_mut(&self.active) {
                    view.cursor = cursor;
                    view.anchor = None;
                    view.preferred_column = None;
                }
            }
            PromptKind::Palette => {
                let choices: Vec<_> = input::COMMANDS
                    .iter()
                    .filter(|(_, label, _)| input::fuzzy(&prompt.text, label))
                    .collect();
                if let Some((command, _, _)) =
                    choices.get(prompt.selected.min(choices.len().saturating_sub(1)))
                {
                    self.command(command)?;
                }
            }
            PromptKind::Quick => {
                let choices: Vec<_> = self
                    .quick_files
                    .iter()
                    .filter(|path| input::fuzzy(&prompt.text, &path.to_string_lossy()))
                    .collect();
                if let Some(path) =
                    choices.get(prompt.selected.min(choices.len().saturating_sub(1)))
                {
                    self.open(&(*path).clone())?;
                }
            }
            PromptKind::Language => {
                self.doc_mut().language = if prompt.text.trim() == "auto" {
                    None
                } else {
                    Some(prompt.text.trim().to_owned())
                };
                self.doc_mut().generation += 1;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn mouse(&mut self, event: MouseEvent) -> Result<()> {
        if self.prompt.is_some() {
            return Ok(());
        }
        if self.sidebar_rect.contains((event.column, event.row).into()) {
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if event.row == self.sidebar_rect.bottom().saturating_sub(1) {
                        self.command("open")?;
                        return Ok(());
                    }
                    if event.row == self.sidebar_rect.y {
                        return Ok(());
                    }
                    let index = self.sidebar_top
                        + event.row.saturating_sub(self.sidebar_rect.y + 1) as usize;
                    if let Some(id) = self.documents.keys().nth(index).copied() {
                        self.activate_document(id);
                    } else {
                        self.command("open")?;
                    }
                }
                MouseEventKind::ScrollDown => {
                    self.sidebar_top =
                        (self.sidebar_top + 3).min(self.documents.len().saturating_sub(1))
                }
                MouseEventKind::ScrollUp => self.sidebar_top = self.sidebar_top.saturating_sub(3),
                _ => {}
            }
            return Ok(());
        }
        let Some(hit) = self
            .hits
            .iter()
            .find(|hit| hit.area.contains((event.column, event.row).into()))
        else {
            return Ok(());
        };
        let id = hit.id;
        let document = self.views[&id].document;
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                let Some(row) = hit.rows.iter().find(|r| r.y == event.row) else {
                    return Ok(());
                };
                let doc = &self.documents[&document];
                let line = doc.line_of(row.start);
                let cursor = doc
                    .at_column(
                        line,
                        row.column + event.column.saturating_sub(hit.text_x) as usize,
                        self.settings.tab_width,
                    )
                    .clamp(row.start, row.end);
                self.active = id;
                self.sidebar_focus = false;
                if let Some(view) = self.views.get_mut(&id) {
                    if matches!(event.kind, MouseEventKind::Drag(_))
                        || event.modifiers.contains(KeyModifiers::SHIFT)
                    {
                        if view.anchor.is_none() {
                            view.anchor = Some(view.cursor);
                        }
                    } else {
                        view.anchor = Some(cursor);
                    }
                    view.cursor = cursor;
                    view.preferred_column = None;
                }
                self.refresh_search();
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                if let Some(view) = self.views.get_mut(&id) {
                    view.top = if event.kind == MouseEventKind::ScrollDown {
                        (view.top + 3).min(self.documents[&document].text.len_lines() - 1)
                    } else {
                        view.top.saturating_sub(3)
                    };
                    view.wrap_top = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn poll_background(&mut self) -> bool {
        let mut moved = false;
        while let Ok(result) = self.search_results.try_recv() {
            if result.ticket == self.search_ticket {
                self.matches = result.matches;
                self.search_pending = false;
                if self
                    .prompt
                    .as_ref()
                    .is_some_and(|p| matches!(p.kind, PromptKind::Find))
                {
                    self.find_next(false, true);
                    moved = true;
                }
            }
        }
        if let Some(scan) = &self.scan
            && let Ok(files) = scan.try_recv()
        {
            self.quick_files = files;
            self.scan = None;
        }
        moved
    }
}

fn scan_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![(directory.to_owned(), 0)];
    let mut visited = 0;
    while let Some((path, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > 10000 || files.len() >= 5000 {
                files.sort();
                return files;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_file() {
                files.push(entry.path());
            } else if kind.is_dir()
                && depth < 4
                && !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | "node_modules" | "target" | ".svn")
                )
            {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    files.sort();
    files
}

#[cfg(windows)]
fn clipboard_set(value: &str) -> Result<()> {
    clipboard_win::set_clipboard_string(value).map_err(|e| {
        anyhow::anyhow!("Clipboard unavailable: {e}. Use terminal copy with Shift+mouse")
    })
}
#[cfg(windows)]
fn clipboard_get() -> Result<String> {
    clipboard_win::get_clipboard_string()
        .map_err(|e| anyhow::anyhow!("Clipboard unavailable: {e}. Use terminal paste"))
}
#[cfg(not(windows))]
fn clipboard_set(_: &str) -> Result<()> {
    bail!("Native clipboard is available on Windows. Use terminal copy")
}
#[cfg(not(windows))]
fn clipboard_get() -> Result<String> {
    bail!("Native clipboard is available on Windows. Use terminal paste")
}
