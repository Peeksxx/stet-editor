use crate::{
    files,
    panes::{Pane, View},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub tab_width: usize,
    pub spaces: bool,
    pub wrap: bool,
    pub line_numbers: bool,
    pub sidebar: bool,
    pub sidebar_width: u16,
    pub restore_session: bool,
    pub default_line_ending: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            tab_width: 4,
            spaces: true,
            wrap: false,
            line_numbers: true,
            sidebar: true,
            sidebar_width: 24,
            restore_session: true,
            default_line_ending: "CRLF".into(),
            extra: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.theme.as_str(), "dark" | "light" | "terminal") {
            bail!("theme must be dark, light, or terminal");
        }
        if !(1..=16).contains(&self.tab_width) {
            bail!("tab_width must be 1 through 16");
        }
        if !(16..=60).contains(&self.sidebar_width) {
            bail!("sidebar_width must be 16 through 60");
        }
        if !matches!(self.default_line_ending.as_str(), "LF" | "CRLF" | "CR") {
            bail!("default_line_ending must be LF, CRLF, or CR");
        }
        Ok(())
    }
    pub fn newline(&self) -> &'static str {
        match self.default_line_ending.as_str() {
            "LF" => "\n",
            "CR" => "\r",
            _ => "\r\n",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub paths: BTreeMap<usize, PathBuf>,
    pub views: BTreeMap<usize, View>,
    pub layout: Pane,
    pub active: usize,
    pub sidebar: bool,
    pub sidebar_width: u16,
    pub directory: PathBuf,
}

impl Session {
    pub fn validate(&self) -> Result<()> {
        let leaves = self.layout.leaves();
        if self.version != 1
            || self.views.keys().any(|id| *id > 1_000_000)
            || self.paths.len() > 256
            || self.views.is_empty()
            || self.views.len() > 8
            || leaves.len() != self.views.len()
            || !self.views.contains_key(&self.active)
        {
            bail!("Invalid session structure");
        }
        let unique: std::collections::BTreeSet<_> = leaves.iter().collect();
        if unique.len() != leaves.len() || leaves.iter().any(|id| !self.views.contains_key(id)) {
            bail!("Invalid pane references");
        }
        if !(16..=60).contains(&self.sidebar_width) {
            bail!("Invalid sidebar width");
        }
        Ok(())
    }
}

pub fn directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("STET_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("Stet"))
        .context("LOCALAPPDATA is unavailable; set STET_CONFIG_DIR to a writable directory")
}

pub fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > 1024 * 1024 {
        bail!("Configuration exceeds 1 MiB");
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        bail!("Configuration exceeds 1 MiB");
    }
    Ok(Some(
        serde_json::from_slice(&bytes).context("Invalid JSON configuration")?,
    ))
}

pub fn store<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("No configuration directory")?;
    std::fs::create_dir_all(parent)?;
    let expected = files::revision(path)?;
    files::atomic_write(path, &serde_json::to_vec_pretty(value)?, expected.as_ref())?;
    Ok(())
}
