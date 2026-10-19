# Use Lossy

> **Release hold:** Defender detected the installed v0.1.0 app as
> `Trojan:Win32/Bearfoos.A!ml`. The cause is under investigation; a false positive is
> not yet confirmed. Do not bypass protection or reinstall the detected version.
> The installation instructions below apply after the hold is cleared.

Install the x64 installer from [Releases](https://github.com/imthegoodboy/lossy/releases). The
initial build is unsigned; Windows may warn about an unknown publisher. Only install trusted builds.
Press **Start saving** and test a synthetic Notepad draft. This enables tray-only startup
at sign-in. Closing the archive leaves saving running. **Quit Lossy** in the tray stops saving
until the next app start.

## Supported capture

| Source | Behavior |
| --- | --- |
| Archive page | Four-column clickable cards, full-content popup, automatically loaded older items |
| Native UI Automation Edit/Document fields | Notepad, Word, Office and other standard editors; selected apps or optional broader desktop capture; full executable-path checks also support separate renderer processes |
| Standard web text boxes | Opt-in [Chrome/Edge companion](browser-companion.md) |
| WhatsApp Web | Best-effort isolation; uncertain chat switches make separate cards |
| Native clipboard | Any desktop app except password managers; text and PNG bitmap images, including browser copies |
| Private windows, password fields, known password managers | Excluded from passive drafts; clipboard copies are skipped when a password field is focused |
| WhatsApp Desktop, terminals, elevated/inaccessible editors | Not reliably supported for drafts; explicit copies can still be kept |

Cursor/VS Code capture depends on standard editable accessibility fields. Incompatible native
text starts another card. No adapter can infer IDs that the source does not expose. Native
polling is about 35 ms; clipboard polling 100 ms. Neither guarantees every physical key before a power cut.

## Capture setup and installation checks

Finish the installer (the destination-folder screen is not the completed installation).
Keep **Create desktop shortcut** selected on the finish page. Lossy should appear on your
desktop, in the Start menu, and under Windows **Settings → Apps → Installed apps**.
Rerunning the installer repairs installation files; it does not reset your encrypted archive.

Expand **Capture setup** on the archive page to enable saving, pause it, control quiet sign-in
startup, enable clipboard images/text, or choose desktop apps. New installs start with common
writing and work apps; **Add common apps** extends an existing list without replacing it. Add an executable
such as `orca.exe` manually if it is absent. The optional **all supported desktop apps** setting
includes future apps, not just known ones. It still excludes known browsers, terminal hosts,
WhatsApp Desktop and password managers. Unknown applications cannot always be classified; use
the selected-app list for tighter privacy.

The panel shows the latest desktop field check (including its time), not a guarantee of ongoing
capture in every window. A supported-field result means text can be read; the last-save time
changes only when content is committed. While viewing Lossy, the previous external field's
result stays visible. Read the card itself to confirm an important draft was recovered.
Browser companion setup and per-site permission are separate from desktop-app permissions.
Clipboard copies are explicit user actions and are kept from any desktop app except password
managers, including browsers and terminals. Passive browser drafts still need the companion,
and terminal buffers are never scraped as a fallback.

If Lossy is missing, finish or rerun setup and inspect Windows Security Protection history if
the executable disappears again. Do not disable security software or delete the data folder.
For maintainers: `scripts/verify-install.ps1` checks the executable/version, companion resources,
uninstaller, Installed Apps registration and both shortcuts. CI installs and checks the built installer.

## Archive

Everything appears in one scrolling page, four cards across on desktop and fewer in smaller
windows. Use the search box to find saved words and the filter chips to show drafts, clipboard,
images, notes or pinned items. Click anywhere on a card (or focus it and press Enter/Space) to
read the full item.
Drag the card's top strip onto another card to rearrange the board. Focus a card and use
Alt + arrow keys for keyboard movement. The arrangement is stored locally and restored on
reopening; arranged cards stay in place, with newly captured items following them. This moves
cards within the grid, not files to other apps. Dropping outside the board cancels the move.
Use Copy for text or images, choose one of six saved box colours, or keep an item pinned.
More options contains editing, revision history and deletion. Editing a captured item creates
a recovery copy without changing the original; notes can be edited in place. Save changes
explicitly before closing. Escape or clicking outside closes the popup; unsaved edits prompt
for confirmation. There is no navigation sidebar, settings page or button toolbar.
The background still retains the first and roughly 32 recent revisions and creates backups.
Images preserve PNG pixels, not filenames or animations.

Limits: native drafts 1 MiB; browser text 200,000 UTF-16 characters; images 16 million pixels
and 6 MiB after base64 PNG encoding. Clipboard copies from Paint, Snipping Tool, browsers and
other desktop apps are kept, except known password managers and Lossy itself. Copies made while a password field is focused are skipped
when Windows exposes that state. File-drop lists are not captured. Lossy's own copies are skipped.
Copies whose owner has already exited fall back to the foreground app; if neither can be identified,
the copy is retried briefly and then skipped. Capture setup reports the last clipboard check separately.
Image reading currently accepts PNG and DIBV5 clipboard formats; CF_BITMAP-only copies are not supported.

## Privacy, retention and recovery

Data is at `%LOCALAPPDATA%\Lossy\lossy.db`. AES-GCM encrypts content, headings, source labels
and preferences; DPAPI protects the key for this Windows account. Timestamps, sizes, kind and
pin status are visible metadata. Same-user malware can access the archive. No cloud or analytics.

Retention and three rotating verified backups run every 30 minutes. Pinned items do not expire.
Deleting removes an active item, not immediately existing backups or forensic disk remnants.
Automatic snapshots are stored under the data folder's `backups` directory.
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
