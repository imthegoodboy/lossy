# Runtime architecture

The original plan is a target design. `apps/desktop/src-tauri` implements the runtime; the
tested context-engine crate remains a foundation for richer future adapters.

```text
Native UIA snapshots ─┐
Allowed clipboard ───┼─ bounded queue ─ single writer ─ AES-GCM ─ SQLite FULL WAL
Browser companion ───┘                      ↑
                                current-user named pipe
                                           ↑
                                on-demand Tauri archive UI
```

One executable has separate UI and agent processes. Closing the archive cannot stop the writer.
The tray agent creates no webview. UIA, clipboard, IPC and writer have separate threads. Capture
requires onboarding and source allowlists; there are no keyboard hooks, elevated service or telemetry.

The UI is a single scrollable four-column card matrix, with responsive smaller-window layouts.
Cards open a full-content dialog with copy, encrypted persistent colour preferences, pinning,
recovery-copy editing, retained revisions and deletion. Native copy requests carry the viewed
revision so later background capture cannot change what is copied. A compact search box and
kind/pin filters query decrypted pages in memory; the page keeps no navigation or button
toolbar. Muted paper tones, compact source strips and bottom timestamps keep saved content
visually primary. Reduced motion disables card movement.
Drag/drop and Alt + arrow keys save an encrypted ID order, applied in SQLite before pagination.
Unloaded arranged cards retain their relative order; new items follow the arranged section.
During a drag/save the UI holds list refreshes; failures restore the previous visible order.
Concurrent windows use last-accepted layout order without altering content or revisions.
The native drag handler is disabled for [HTML drag events on Windows](https://v2.tauri.app/reference/javascript/api/namespacewebview/).
The first-run **Start saving** action enables supported capture. Quiet sign-in startup remains a separate opt-in checkbox.

Context hashes use a per-install keyed BLAKE3 secret. Schema v2 adds encrypted preferences,
kind/pin metadata and a unique active draft per context. Creation and ownership commit together;
an empty snapshot finishes ownership while retaining history. Restart resumes known contexts.
Native Edit and Document controls are accepted, so Notepad and Word are covered. Shared
prefix/suffix edits stay in one card; unrelated replacements and WhatsApp switches still favor
extra cards over incorrect merging. The native context also remembers the title chosen for each
editor control and the last real window title seen: Windows 11 Notepad briefly reports an empty
title and then replaces it with the first line, so a related edit keeps the original context
instead of creating a second card. A window-title change is treated as the same document when
the texts continue each other or share most of their words, so template-like documents with
different titles do not overwrite each other. An unrelated snapshot under a transient empty
title waits for the new title rather than contaminating the previous card.
Clipboard copies use the clipboard-owner process and do not recheck the foreground window, so a
quick window switch does not lose a copy. Copies whose owner process has already exited are
retried briefly and then skipped rather than attributed to the foreground app. Known password
managers, Lossy's own copies, Windows clipboard privacy markers and best-effort sensitive
focused fields are excluded before the clipboard is read.

Independent full checkpoints avoid delta-chain dependencies. Images are capped encrypted PNG
payloads with UI thumbnails. Search decrypts small pages in memory; there is no plaintext index.
Startup checks structure, reads authenticate payloads, Verify/backups authenticate all history.
Named-pipe IPC has current-user permissions, length limits and timeouts. The UI has a local CSP.

Known further hardening: signed distribution, physical-power-loss VM/device tests, additional
app-specific adapters, accessibility-provider watchdogs, disk quotas, portable key-aware backups
and separate large-archive search/maintenance scheduling. Large backups/searches can delay the
single writer. These limits are not represented as solved or as universal zero-loss capture.
