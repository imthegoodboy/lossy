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
revision so later background capture cannot change what is copied. The page has no logo,
navigation, search or button toolbar. Muted paper tones, compact source strips and bottom
timestamps keep saved content visually primary. Reduced motion disables card movement.
Drag/drop and Alt + arrow keys save an encrypted ID order, applied in SQLite before pagination.
Unloaded arranged cards retain their relative order; new items follow the arranged section.
During a drag/save the UI holds list refreshes; failures restore the previous visible order.
Concurrent windows use last-accepted layout order without altering content or revisions.
The native drag handler is disabled for [HTML drag events on Windows](https://v2.tauri.app/reference/javascript/api/namespacewebview/).
The first-run inline consent checkbox enables supported capture and silent sign-in startup.

Context hashes use a per-install keyed BLAKE3 secret. Schema v2 adds encrypted preferences,
kind/pin metadata and a unique active draft per context. Creation and ownership commit together;
an empty snapshot finishes ownership while retaining history. Restart resumes known contexts.
Uncertain native replacements and WhatsApp switches favor extra cards over incorrect merging.

Independent full checkpoints avoid delta-chain dependencies. Images are capped encrypted PNG
payloads with UI thumbnails. Search decrypts small pages in memory; there is no plaintext index.
Startup checks structure, reads authenticate payloads, Verify/backups authenticate all history.
Named-pipe IPC has current-user permissions, length limits and timeouts. The UI has a local CSP.

Known further hardening: signed distribution, physical-power-loss VM/device tests, additional
app-specific adapters, accessibility-provider watchdogs, disk quotas, portable key-aware backups
and separate large-archive search/maintenance scheduling. Large backups/searches can delay the
single writer. These limits are not represented as solved or as universal zero-loss capture.
