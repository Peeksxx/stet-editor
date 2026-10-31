use crate::files::{self, Revision};
use anyhow::{Result, bail};
use ropey::Rope;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
        }
    }
}

#[derive(Clone)]
struct Edit {
    start: usize,
    removed: String,
    inserted: String,
    before: u64,
    after: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Delta {
    pub start: usize,
    pub removed: usize,
    pub inserted: usize,
}

impl Delta {
    pub fn transform(self, offset: usize) -> usize {
        if offset <= self.start {
            offset
        } else if offset < self.start + self.removed {
            self.start + self.inserted
        } else {
            offset - self.removed + self.inserted
        }
    }
}

pub struct Document {
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub encoding: Encoding,
    pub newline: String,
    pub language: Option<String>,
    pub generation: u64,
    pub changed_line: usize,
    pub highlight: bool,
    pub disk: Option<Revision>,
    saved_state: u64,
    state: u64,
    serial: u64,
    undo: Vec<Vec<Edit>>,
    redo: Vec<Vec<Edit>>,
    typing: Option<Instant>,
    ending_counts: [usize; 3],
    history_bytes: usize,
}

impl Document {
    pub fn blank(newline: &str) -> Self {
        Self {
            text: Rope::new(),
            path: None,
            encoding: Encoding::Utf8,
            newline: newline.into(),
            language: None,
            generation: 0,
            changed_line: 0,
            highlight: true,
            disk: None,
            saved_state: 0,
            state: 0,
            serial: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: None,
            ending_counts: [0; 3],
            history_bytes: 0,
        }
    }

    pub fn open(path: &Path, newline: &str) -> Result<Self> {
        let path = files::identity(path)?;
        let mut doc = Self::blank(newline);
        if path.exists() {
            let bytes = files::read(&path)?;
            if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
                bail!("UTF-16 is detected. Convert to UTF-8 in another editor before opening");
            }
            let content = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                doc.encoding = Encoding::Utf8Bom;
                &bytes[3..]
            } else {
                &bytes
            };
            let text = std::str::from_utf8(content).map_err(|_| {
                anyhow::anyhow!("Unsupported encoding or binary file; UTF-8 is required")
            })?;
            validate_text(text)?;
            if let Some(position) = text.find(['\r', '\n']) {
                doc.newline = if text[position..].starts_with("\r\n") {
                    "\r\n"
                } else if text[position..].starts_with('\r') {
                    "\r"
                } else {
                    "\n"
                }
                .into();
            }
            doc.text = Rope::from_str(text);
            doc.ending_counts = count_endings(text.chars());
            doc.highlight =
                text.len() <= 1024 * 1024 && !text.split(['\r', '\n']).any(|l| l.len() > 4096);
            doc.disk = Some(Revision(blake3::hash(&bytes)));
        }
        doc.path = Some(path);
        Ok(doc)
    }

    pub fn dirty(&self) -> bool {
        self.state != self.saved_state
    }
    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into())
    }
    pub fn end(&self) -> usize {
        self.text.len_chars()
    }
    pub fn line_of(&self, offset: usize) -> usize {
        self.text.char_to_line(offset.min(self.end()))
    }
    pub fn line_start(&self, line: usize) -> usize {
        self.text.line_to_char(line.min(self.text.len_lines() - 1))
    }
    pub fn line_text(&self, line: usize) -> String {
        self.text
            .line(line.min(self.text.len_lines() - 1))
            .to_string()
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    }
    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line_text(line).chars().count()
    }
    pub fn selected(&self, range: Range<usize>) -> String {
        self.text.slice(range).to_string()
    }
    pub fn stop_typing(&mut self) {
        self.typing = None;
    }

    pub fn edit(&mut self, range: Range<usize>, insert: &str, typing: bool) -> Result<Delta> {
        if range.start > range.end || range.end > self.end() {
            bail!("Invalid edit range");
        }
        let removed_bytes = self.text.char_to_byte(range.end) - self.text.char_to_byte(range.start);
        if self
            .text
            .len_bytes()
            .saturating_sub(removed_bytes)
            .saturating_add(insert.len())
            > files::MAX_BYTES as usize
        {
            bail!("Editing limit is 64 MiB");
        }
        validate_text(insert)?;
        let first = self.line_of(range.start);
        let last = self.line_of(range.end);
        let prefix = self
            .text
            .slice(self.line_start(first)..range.start)
            .to_string();
        let suffix = self
            .text
            .slice(range.end..self.line_end(last).max(range.end))
            .to_string();
        let joined = format!("{prefix}{insert}{suffix}");
        validate_text(&joined)?;
        let removed = self.text.slice(range.clone()).to_string();
        let delta = Delta {
            start: range.start,
            removed: range.len(),
            inserted: insert.chars().count(),
        };
        if removed == insert {
            return Ok(Delta {
                start: range.start,
                removed: 0,
                inserted: 0,
            });
        }
        let group = typing
            && self.undo.last().is_none_or(|group| group.len() < 2048)
            && range.is_empty()
            && !insert.contains(['\n', '\r'])
            && self
                .typing
                .is_some_and(|t| t.elapsed() < Duration::from_millis(800))
            && self.undo.last().and_then(|g| g.last()).is_some_and(|e| {
                e.start + e.inserted.chars().count() == range.start && e.removed.is_empty()
            });
        self.serial += 1;
        let edit = Edit {
            start: range.start,
            removed,
            inserted: insert.into(),
            before: self.state,
            after: self.serial,
        };
        self.history_bytes += edit_size(&edit);
        self.history_bytes = self
            .history_bytes
            .saturating_sub(self.redo.iter().flatten().map(edit_size).sum::<usize>());
        self.replace_text(range.clone(), insert);
        self.highlight &= !joined.split(['\r', '\n']).any(|l| l.len() > 4096);
        self.state = self.serial;
        self.generation += 1;
        self.changed_line = first;
        if group {
            if let Some(last) = self.undo.last_mut() {
                last.push(edit);
            }
        } else {
            self.undo.push(vec![edit]);
        }
        self.redo.clear();
        self.trim_history();
        self.typing = if typing { Some(Instant::now()) } else { None };
        Ok(delta)
    }

    pub fn undo(&mut self, redo: bool) -> Vec<Delta> {
        self.stop_typing();
        let group = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        let Some(group) = group else {
            return Vec::new();
        };
        let mut deltas = Vec::new();
        let indices: Vec<usize> = if redo {
            (0..group.len()).collect()
        } else {
            (0..group.len()).rev().collect()
        };
        for index in indices {
            let edit = &group[index];
            let (remove, insert) = if redo {
                (&edit.removed, &edit.inserted)
            } else {
                (&edit.inserted, &edit.removed)
            };
            let removed = remove.chars().count();
            self.replace_text(edit.start..edit.start + removed, insert);
            self.state = if redo { edit.after } else { edit.before };
            self.generation += 1;
            self.changed_line = self.line_of(edit.start);
            deltas.push(Delta {
                start: edit.start,
                removed,
                inserted: insert.chars().count(),
            });
        }
        if redo {
            self.undo.push(group);
        } else {
            self.redo.push(group);
        }
        deltas
    }

    pub fn save(&mut self, path: &Path, expected: Option<&Revision>) -> Result<()> {
        let path = files::identity(path)?;
        let mut bytes = Vec::with_capacity(self.text.len_bytes() + 3);
        if self.encoding == Encoding::Utf8Bom {
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        for chunk in self.text.chunks() {
            bytes.extend_from_slice(chunk.as_bytes());
        }
        let revision = files::atomic_write(&path, &bytes, expected)?;
        self.disk = Some(revision);
        self.path = Some(path);
        self.saved_state = self.state;
        self.stop_typing();
        Ok(())
    }

    fn replace_text(&mut self, range: Range<usize>, insert: &str) {
        let start = range.start.saturating_sub(1);
        let end = (range.end + 1).min(self.end());
        let old = count_endings(self.text.slice(start..end).chars());
        self.text.remove(range.clone());
        self.text.insert(range.start, insert);
        let new_end = end - range.len() + insert.chars().count();
        let new = count_endings(self.text.slice(start..new_end).chars());
        for i in 0..3 {
            self.ending_counts[i] = (self.ending_counts[i] as isize + new[i] as isize
                - old[i] as isize)
                .max(0) as usize;
        }
    }

    pub fn replace_all(
        &mut self,
        needle: &str,
        replacement: &str,
        sensitive: bool,
    ) -> Result<Vec<Delta>> {
        let ranges: Vec<_> = self
            .matches(needle, sensitive)
            .into_iter()
            .filter(|range| self.text.slice(range.clone()) != replacement)
            .collect();
        if ranges.is_empty() {
            return Ok(Vec::new());
        }
        let size = self
            .text
            .len_bytes()
            .saturating_sub(ranges.len() * needle.len())
            .saturating_add(ranges.len().saturating_mul(replacement.len()));
        if size > files::MAX_BYTES as usize {
            bail!("Editing limit is 64 MiB");
        }
        validate_text(replacement)?;
        let mut staged = self.text.clone();
        for range in ranges.iter().rev() {
            staged.remove(range.clone());
            staged.insert(range.start, replacement);
        }
        for line in staged.lines() {
            if line.len_bytes() > files::MAX_LINE_BYTES
                && line.to_string().trim_end_matches(['\r', '\n']).len() > files::MAX_LINE_BYTES
            {
                bail!("A line exceeds the 128 KiB safety limit");
            }
        }
        self.changed_line = self.line_of(ranges[0].start);
        let mut edits = Vec::new();
        let mut deltas = Vec::new();
        for range in ranges.into_iter().rev() {
            self.serial += 1;
            let edit = Edit {
                start: range.start,
                removed: self.text.slice(range.clone()).to_string(),
                inserted: replacement.into(),
                before: self.state,
                after: self.serial,
            };
            self.state = self.serial;
            self.history_bytes += edit_size(&edit);
            edits.push(edit);
            deltas.push(Delta {
                start: range.start,
                removed: range.len(),
                inserted: replacement.chars().count(),
            });
        }
        self.history_bytes = self
            .history_bytes
            .saturating_sub(self.redo.iter().flatten().map(edit_size).sum::<usize>());
        self.redo.clear();
        self.undo.push(edits);
        self.text = staged;
        self.ending_counts = count_endings(self.text.chars());
        self.highlight &= !self.text.lines().any(|line| line.len_bytes() > 4096);
        self.generation += 1;
        self.stop_typing();
        self.trim_history();
        Ok(deltas)
    }

    fn trim_history(&mut self) {
        while self.undo.len() > 1
            && (self.history_bytes > 128 * 1024 * 1024 || self.undo.len() > 2000)
        {
            let removed = self.undo.remove(0);
            self.history_bytes = self
                .history_bytes
                .saturating_sub(removed.iter().map(edit_size).sum::<usize>());
        }
    }

    pub fn boundary(&self, offset: usize, forward: bool) -> usize {
        let offset = offset.min(self.end());
        let line = self.line_of(offset);
        let start = self.line_start(line);
        if !forward && offset == start {
            return if line == 0 {
                0
            } else {
                self.line_end(line - 1)
            };
        }
        let text = self.text.line(line).to_string();
        let local = offset - start;
        let mut count = 0;
        let mut previous = start;
        for g in text.graphemes(true) {
            count += g.chars().count();
            let next = start + count;
            if forward && next > offset {
                return next;
            }
            if !forward && next >= start + local {
                return previous;
            }
            previous = next;
        }
        if forward {
            self.end()
        } else if start > 0 {
            self.boundary(start - 1, false).max(self.line_end(line - 1))
        } else {
            0
        }
    }

    pub fn word(&self, offset: usize, forward: bool) -> usize {
        let mut position = offset;
        let mut met_word = false;
        while if forward {
            position < self.end()
        } else {
            position > 0
        } {
            let next = self.boundary(position, forward);
            if next == position {
                break;
            }
            let c = self.text.char(if forward { position } else { next });
            let is_word = c.is_alphanumeric() || c == '_';
            if met_word && !is_word {
                break;
            }
            met_word |= is_word;
            position = next;
        }
        position
    }

    pub fn column(&self, offset: usize, tab: usize) -> usize {
        let start = self.line_start(self.line_of(offset));
        cells(
            &self.text.slice(start..offset.min(self.end())).to_string(),
            tab,
        )
    }

    pub fn at_column(&self, line: usize, column: usize, tab: usize) -> usize {
        let mut width = 0;
        let mut position = self.line_start(line);
        for g in self.line_text(line).graphemes(true) {
            let size = grapheme_width(g, width, tab);
            if width + size > column {
                break;
            }
            width += size;
            position += g.chars().count();
        }
        position
    }

    pub fn matches(&self, needle: &str, sensitive: bool) -> Vec<Range<usize>> {
        self.matches_cancellable(needle, sensitive, || false)
    }

    pub fn matches_cancellable(
        &self,
        needle: &str,
        sensitive: bool,
        cancelled: impl Fn() -> bool,
    ) -> Vec<Range<usize>> {
        if needle.is_empty() || needle.contains(['\r', '\n']) || needle.len() > 4096 {
            return Vec::new();
        }
        let mut matches = Vec::new();
        for (line_index, line) in self.text.lines().enumerate() {
            if cancelled() {
                return Vec::new();
            }
            let text = line.to_string();
            let start = self.line_start(line_index);
            let haystack = if sensitive {
                text.clone()
            } else {
                text.to_ascii_lowercase()
            };
            let needle = if sensitive {
                needle.to_owned()
            } else {
                needle.to_ascii_lowercase()
            };
            let mut previous_byte = 0;
            let mut previous_char = 0;
            for (byte, found) in haystack.match_indices(&needle) {
                previous_char += text[previous_byte..byte].chars().count();
                matches.push(start + previous_char..start + previous_char + found.chars().count());
                previous_byte = byte;
                if matches.len() == 10000 {
                    return matches;
                }
            }
        }
        matches
    }

    pub fn endings(&self) -> &'static str {
        let kinds = self.ending_counts.map(|n| n > 0);
        if !kinds.iter().any(|v| *v) {
            return match self.newline.as_str() {
                "\r\n" => "CRLF",
                "\r" => "CR",
                _ => "LF",
            };
        }
        if kinds.iter().filter(|v| **v).count() > 1 {
            "Mixed"
        } else if kinds[0] || self.newline == "\r\n" && !kinds[1] && !kinds[2] {
            "CRLF"
        } else if kinds[2] {
            "CR"
        } else {
            "LF"
        }
    }
}

fn edit_size(edit: &Edit) -> usize {
    std::mem::size_of::<Edit>() + edit.removed.len() + edit.inserted.len()
}

fn count_endings(chars: impl Iterator<Item = char>) -> [usize; 3] {
    let mut counts = [0; 3];
    let mut previous = '\0';
    for c in chars {
        if previous == '\r' {
            if c == '\n' {
                counts[0] += 1;
            } else {
                counts[2] += 1;
            }
        }
        if c == '\n' && previous != '\r' {
            counts[1] += 1;
        }
        previous = c;
    }
    if previous == '\r' {
        counts[2] += 1;
    }
    counts
}

pub fn validate_text(text: &str) -> Result<()> {
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n' | '\u{1b}'))
    {
        bail!("Binary or unsupported control data detected");
    }
    if text
        .split(['\n', '\r'])
        .any(|line| line.len() > files::MAX_LINE_BYTES)
    {
        bail!("A line exceeds the 128 KiB safety limit");
    }
    Ok(())
}

pub fn grapheme_width(g: &str, column: usize, tab: usize) -> usize {
    if g == "\t" {
        tab.max(1) - column % tab.max(1)
    } else if g.chars().any(char::is_control) {
        1
    } else {
        UnicodeWidthStr::width(g).max(1)
    }
}

pub fn cells(text: &str, tab: usize) -> usize {
    text.graphemes(true)
        .fold(0, |column, g| column + grapheme_width(g, column, tab))
}

pub fn wrapped_columns(text: &str, width: usize, tab: usize) -> Vec<usize> {
    let width = width.max(1);
    let mut columns = vec![0];
    let mut column = 0;
    let mut start = 0;
    for g in text.graphemes(true) {
        let size = grapheme_width(g, column, tab);
        if column > start && column + size > start + width {
            columns.push(column);
            start = column;
        }
        column += size;
    }
    if column > start && column == start + width {
        columns.push(column);
    }
    columns
}
