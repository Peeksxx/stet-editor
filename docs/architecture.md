# Architecture

`document` owns a Ropey buffer, edit transactions, saved revision, encoding, and newline counts. Offsets are Unicode scalar indices; navigation follows grapheme boundaries; drawing and mouse mapping use terminal cell widths. CRLF remains a single navigation unit without changing its stored bytes.

`files` treats paths as data. Existing paths resolve to their canonical targets. Saves write a sibling temporary file, flush it, recheck the disk hash, and replace the destination. Windows uses `ReplaceFileW`. If both files have identical owner, group, and DACL descriptors, redundant ACL merging may be skipped; otherwise strict merging is required. There is no overwrite fallback. New files use a no-clobber operation. Failed saves keep the buffer dirty. UTF-16 and unsupported data are refused before editing.

A sibling temporary backup protects partial Windows replacement failures. Success removes it. Failure restores the original without overwriting a concurrent destination, or retains the original at a named recovery path.

`app` owns documents and independent pane views. A recursive `Pane` tree stores layout. Every view references a document ID. Edits transform the other views' positions. Closing a pane leaves its documents open; closing a document redirects every view of it.

`ui` renders visible text, sanitizes control characters, and records rows for mouse hit testing. Syntax state is cached by document and line. Changes invalidate the affected suffix; parsing proceeds in short batches. Large documents and long lines use plain text. A worker performs large-file searches on cheap rope snapshots and discards stale requests. Quick open has a separate bounded directory scan.

`config` validates JSON before use and writes through the same replacement path. Settings preserve unknown fields. Session records contain paths and pane metadata. No document contents, clipboard text, or recovery library are stored.

The event loop blocks while idle. A cleanup guard restores mouse capture, bracketed paste, cursor visibility, alternate screen, and raw mode. A panic hook also attempts cleanup. Redirected interactive input or output is rejected.

There is no shell integration, script execution, networking, plugin runtime, or telemetry.

Opening, pasting, and replacement use the same text validation. Unsupported binary control characters are refused before the buffer changes.

The installer's PATH helpers compare expanded directory names while retaining the original bytes of unrelated entries. Native helper tests run without installing Stet or editing the registry.

The MSVC build links the C runtime statically through `.cargo/config.toml`. The editor needs only Windows system libraries at runtime.
