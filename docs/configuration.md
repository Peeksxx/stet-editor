# Configuration

Restart Stet after editing settings or keybindings. To isolate a session, set `STET_CONFIG_DIR` to another writable folder.

Default `settings.json`:

```json
{
  "theme": "dark",
  "tab_width": 4,
  "spaces": true,
  "wrap": false,
  "line_numbers": true,
  "sidebar": true,
  "sidebar_width": 24,
  "restore_session": true,
  "default_line_ending": "CRLF"
}
```

Themes: `dark`, `light`, `terminal`. Tab width: 1 to 16. Sidebar width: 16 to 60 cells. New-file endings: `CRLF`, `LF`, `CR`. Unknown settings survive writes. Invalid settings use defaults for that run and are never overwritten. Repair the file to restore persistence.

F6 changes and saves the theme immediately. Other interface settings persist on normal exit. Sessions store saved paths and view metadata, including unsaved buffers' paths when those files already exist. Unsaved text is never stored.

Invalid session files stay intact, including when restoration is disabled or filenames are supplied. Repair or remove the invalid file to resume session persistence.

`keybindings.json` maps command identifiers to one key combination:

```json
{
  "split_vertical": "Alt+V",
  "split_horizontal": "Alt+H",
  "save_as": "F9",
  "palette": "F10",
  "quick": ""
}
```

An empty string disables a binding. Unlisted commands keep their defaults. Duplicate bindings or invalid commands reject the entire override file and show an error; defaults remain usable. F2 opens Commands when it is not assigned to another action.

If a host reports Ctrl+H as Backspace, use F2, then Find and replace, or remap `replace` to `Ctrl+R`.

Use `Ctrl`, `Alt`, `Shift`, a letter or punctuation character, `F1` through `F24`, or a named key: `Tab`, `Enter`, `Esc`, `Left`, `Right`, `Up`, `Down`, `Home`, `End`, `Delete`, `Backspace`, `PageUp`, `PageDown`. Letters are case-insensitive. Plain editing keys cannot be rebound without modifiers.

Command identifiers:

```text
open save save_as close quit new next previous
find replace goto undo redo select_all copy cut paste
quick palette sidebar split_vertical split_horizontal close_pane single_pane
pane_left pane_right pane_up pane_down help theme wrap focus_sidebar
next_match previous_match case language path reload
```

Prompts use Left/Right, Home/End, Delete/Backspace, Ctrl+U to clear, and Ctrl+V to paste. Find updates as you type. Enter advances; Esc returns to editing with the selected match. Replace asks for a literal search term, then replacement text; Enter replaces all listed matches. Empty replacement deletes matches.

F7 accepts an extension such as `rs`, a syntax name such as `Python`, `plain`, or `auto`. Syntax overrides apply to the current run. Unknown types render as plain text.
