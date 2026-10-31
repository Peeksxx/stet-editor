**Stet** comes from the Latin word meaning "let it stand." In proofreading, *stet* is an instruction to disregard a correction and leave the original text unchanged.

Stet is a lightweight, keyboard-first terminal text editor for Windows. Open a file, make your changes, and get back to work.

# Stet

**Windows terminal editor · Rust · 1.0.0**

Your files, a quiet interface, and familiar shortcuts. No accounts, network access, background services, or workspace setup.

```text
 stet  1.0.0      main.rs *

 OPEN FILES             │ main.rs *
 › main.rs *            │  1  fn main() {
   Cargo.toml           │  2      println!("Let it stand.");
 + Open file            │  3  }
                        │

 EDIT | Ln 2, Col 5 | Rust | UTF-8 | CRLF | Spaces:4 | Modified
 ^S Save   ^O Open   ^F Find   F2 Commands   F1 Help
```

Interface example, not a screenshot. Colors follow the selected theme.

## Editing

- Multiple files with a clickable sidebar.
- Vertical and horizontal splits, up to eight panes. Views of a file share its buffer.
- Grapheme navigation, Shift selection, mouse selection, native Windows clipboard.
- Grouped typing undo, redo, auto-indent, tabs or spaces, optional line wrapping.
- Literal find, match navigation, case sensitivity, and Replace All with one undo step.
- Case-insensitive replacements can change letter case. Unchanged replacements keep buffers and other pane positions intact.
- Syntax colors for common languages, including Rust, C/C++, Python, JS/TS, JSON, TOML, HTML/CSS, and Markdown.
- Dark, light, and terminal-color themes. File paths, pane layouts, and positions restore on restart.
- Conflict checks, read-only protection, and flushed temporary-file replacement on save.

## Install

Requires Windows 10/11 x64 and an interactive console. Windows Terminal is the primary target. No separate application runtime is needed.

Download `Stet-Setup-x64.exe` from the repository's Releases page after a maintainer publishes it. Run setup, then open a new terminal for PATH changes.

Setup installs into `C:\Program Files\Stet`, adds its directory to the machine PATH, and registers an uninstaller. Setup needs elevation; Stet does not. Settings survive upgrades and uninstall.

PATH updates recognize environment-variable aliases and preserve unrelated entries, including empty entries. Custom installation directories cannot contain semicolons.

The installer is unsigned. Windows SmartScreen may warn. Verify its SHA-256 against `SHA256SUMS.txt` from the same release.

## Open a file

```powershell
stet
stet README.md
stet "C:\Users\Example\Documents\Financial Report.txt"
stet main.cpp bookmarks.h
stet --no-restore
stet --help
stet --version
```

Explicit filenames replace session restoration. A new path opens an empty buffer and is created only when saved. Use `--` before filenames starting with `-`.

In Open File, paste one path, including quotes supplied by PowerShell or Command Prompt. Terminal drag-and-drop works when the host pastes the path. Stet does not receive native drop events or execute paths.

## Keys

| Key | Action |
|---|---|
| Ctrl+O / Ctrl+S / Ctrl+Shift+S | Open / Save / Save As |
| Ctrl+N / Ctrl+W / Ctrl+Q | New / Close file / Quit |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous file |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+A / C / X / V | Select all / copy / cut / paste |
| Ctrl+F / Ctrl+H / Ctrl+G | Find / Replace All / Go to line |
| F3 / Shift+F3 / Alt+C | Next match / previous match / case sensitivity |
| Ctrl+P | Quick open under the starting directory |
| F2 / Ctrl+Shift+P | Command palette |
| Ctrl+B / F8 | Toggle sidebar / focus file list |
| Ctrl+\ / Ctrl+Shift+\ | Vertical / horizontal split |
| Alt+Arrow / Alt+W / Alt+1 | Focus pane / close pane / single pane |
| Alt+Z / F6 / F7 | Wrap / theme / syntax override |
| Alt+I / Alt+R | Full path / reload from disk |
| F1 / Esc | Help / cancel prompt |

F2 exposes every command when a terminal intercepts a shortcut. Help and lists scroll with Up/Down. Hold Shift while using the mouse for the terminal's own selection, where supported.

Closing a modified file offers Save, Discard, or Cancel. Quitting visits each modified buffer. A conflict prompt requires explicit confirmation before replacing changed content, and checks the revision again before saving.

## Configure

Files live in `%LOCALAPPDATA%\Stet`:

```text
settings.json      Theme, indentation, wrapping, sidebar, restoration
keybindings.json   Command-to-key overrides
session.json       Saved paths and pane positions, never document contents
```

Settings are optional. Invalid files produce a notification and stay intact. [Configuration reference](docs/configuration.md) includes defaults and key remapping.

## Build

Install stable Rust with the MSVC toolchain and Visual Studio C++ Build Tools with a Windows SDK.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

For an installer, install Inno Setup 6 or 7:

```powershell
./scripts/package.ps1 -Compiler "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
```

Outputs go to `release`. Use `-OutputDirectory` to choose another folder. Packaging respects `CARGO_TARGET_DIR`. The build needs registry access; the installed editor works offline.

## Limits

- UTF-8 with or without BOM. UTF-16, invalid UTF-8, and binary files are refused. Original LF, CRLF, CR, and mixed endings are preserved; new lines use the file's first ending.
- Files are limited to 64 MiB and lines to 128 KiB. Syntax coloring stops above 1 MiB, 20,000 lines, or a 4 KiB line. Highlighting catches up incrementally after edits.
- Find is single-line literal text, with up to 10,000 non-overlapping results. Case-insensitive matching folds ASCII only. There are no regular expressions.
- Undo history keeps up to 2,000 transactions and about 128 MiB per document; older transactions expire. A single large transaction may exceed that budget.
- Quick open scans at most 10,000 directory entries, returns up to 5,000 files, skips symbolic links and common build folders, and does not scan drive roots.
- No recovery copies of unsaved text. Forced termination or power loss can lose changes. Concurrent writers can still race between the final conflict check and filesystem replacement.
- File loading and saving are synchronous. Large files or slow drives can pause the interface. See [acceptance checks](docs/acceptance.md) for host-specific verification.
- The executable and installer are unsigned. This project has no license selected yet.

[Manual publishing guide](docs/publishing.md) · [Architecture](docs/architecture.md)
