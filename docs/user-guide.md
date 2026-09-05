# Use Lossy

Install the x64 installer from [Releases](https://github.com/imthegoodboy/lossy/releases). The
initial build is unsigned; Windows may warn about an unknown publisher. Only install trusted builds.
Complete the initial preferences, choose allowed executables, and test a synthetic Notepad draft.
Enable **Start quietly with Windows** for tray-only startup at sign-in. Closing the archive
leaves saving running. **Quit Lossy** in the tray stops saving until the next app start.

## Supported capture

| Source | Behavior |
| --- | --- |
| Lossy notes | Auto-saving, revision-checked edits |
| Native UI Automation Edit fields | Explicit app allowlist, process/window/editor identity |
| Standard web text boxes | Opt-in [Chrome/Edge companion](browser-companion.md) |
| WhatsApp Web | Best-effort isolation; uncertain chat switches make separate cards |
| Native clipboard | Allowed apps only; text and PNG bitmap images |
| Private windows, password fields, known password managers | Excluded; ordinary free text may still contain secrets |
| WhatsApp Desktop, terminals, elevated/inaccessible editors | Not reliably supported; no raw keylogging fallback |

Cursor/VS Code capture depends on standard editable accessibility fields. Incompatible native
text starts another card. No adapter can infer IDs that the source does not expose. Native
polling is about 35 ms; clipboard polling 100 ms. Neither guarantees every physical key before a power cut.

## Archive

Search, open a card, copy, pin or edit. Captured originals are preserved while edits autosave
as separate My notes recovery copies. A version conflict never silently overwrites another
window's changes. Retain the unsaved text and reconcile with a freshly opened version.
The first and roughly 32 recent revisions remain; intermediate numbers may be compacted.
Images can be viewed/copied, not edited. They preserve PNG pixels, not filenames or animations.

Limits: native drafts 1 MiB; browser text 200,000 UTF-16 characters; images 16 million pixels
and 6 MiB after base64 PNG encoding. Native clipboard defaults include Paint and Snipping Tool.
Browser clipboard images and file-drop lists are not captured. Lossy's own copies are skipped.

## Privacy, retention and recovery

Data is at `%LOCALAPPDATA%\Lossy\lossy.db`. AES-GCM encrypts content, headings, source labels
and preferences; DPAPI protects the key for this Windows account. Timestamps, sizes, kind and
pin status are visible metadata. Same-user malware can access the archive. No cloud or analytics.

Retention and three rotating verified backups run every 30 minutes. Pinned items do not expire.
Deleting removes an active item, not immediately existing backups or forensic disk remnants.
Preferences → Back up now creates a verified snapshot under the data folder's `backups` directory.
These remain tied to your account's DPAPI keys, not portable to another account/computer by themselves.

For manual restore, quit Lossy and preserve the **entire** data folder including any WAL/SHM
files. Never copy an open database alone. With Lossy stopped, move the existing database/WAL/SHM
aside, copy a verified backup as `lossy.db`, and reopen on the same account. Keep originals until
recovery is confirmed. Missing keys/corruption never trigger automatic deletion or reset.

If saving fails, check pause, source permissions and free disk space. Reopen Lossy if the agent
is unresponsive; if necessary quit only Lossy from the tray/Task Manager first. Never delete the
database merely to clear an error. Unobserved or uncommitted input cannot be recovered.

Startup uses the current-user scheduled task **Lossy Background Recovery**, normal privileges,
`lossy.exe --agent`, no window. If task registration is denied, a current-user Run entry is used
as a fallback. It cannot capture before sign-in. Uninstall removes startup and host registration, but keeps
your encrypted history. Remove the unpacked browser extension separately.
