# Manual acceptance

Use temporary files. Check Windows Terminal with PowerShell, Windows Terminal with Command Prompt, and classic Console Host separately.

- Open several paths, including spaces and Unicode. Reopen one and confirm it shares a buffer.
- Insert, delete, indent, select, copy, cut, paste, undo, redo, and save.
- Navigate combining accents, emoji sequences, wide characters, tabs, and CRLF.
- Select by Shift+Arrow and mouse drag. Wheel-scroll without moving the cursor.
- Click filenames, focus the sidebar with F8, and activate with Enter.
- Split both ways. Edit the same file in each pane; switch focus and close a pane.
- Find next/previous, change case sensitivity, replace all, then undo once.
- Open Commands with F2 and use actions whose shortcuts the host intercepts.
- Close and quit dirty buffers. Exercise Save, Discard, Cancel, and Save As errors.
- Modify a file externally before saving. Confirm Cancel preserves both versions.
- Restart and restore panes and positions. Remove a saved path and confirm a notice.
- Paste quoted PowerShell and Command Prompt paths into Open File.
- Resize down to a tiny terminal, then back. Check mouse mapping after resizing.
- Change themes, restart, and confirm persistence.
- Exit normally and after a recoverable error. Check terminal echo, cursor, and mouse.
- In a disposable Windows VM, install, upgrade, and uninstall. Verify PATH has one Stet entry, unrelated entries survive, settings survive, and Stet runs without elevation.

Automated validation covers the model and terminal buffer rendering. Console hosts, clipboard contention, SmartScreen, installer elevation, Windows ACL inheritance, and full install/uninstall require on-device checks. Local verification results are kept outside the publishable source tree.
