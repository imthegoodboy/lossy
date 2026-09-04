# Lossy — Product and Technical Architecture Plan

## 1. Product definition

**Lossy** is a Windows-first, fully local draft-recovery and smart-clipboard application. It quietly records changes made inside supported text editors and clipboard copies, organizes them by application and working context, and lets the user recover, edit, or copy previous content after an application crash, accidental navigation, or computer restart.

Lossy is not a raw global keylogger. It observes the value of the currently focused editable control through approved local integrations and Windows accessibility APIs. This captures useful text changes—including typing, paste, speech-to-text, and input-method editor output—without recording passwords or reconstructing every physical key pressed.

### Primary product promises

- Fully local operation after installation; no account, cloud, telemetry, or network dependency.
- Starts automatically and silently in the signed-in user's Windows session.
- Does not open the main window during automatic startup.
- Saves each observable text change durably without blocking the user's typing.
- Separates drafts by application, account/profile, conversation/document/workspace, and editor.
- Resumes the correct unfinished draft after the user switches away and returns.
- Captures copied text and images as a local clipboard history.
- Presents saved items as simple pink, friendly cards with headings, previews, and timestamps.
- Allows full viewing, editing, copying, pinning, renaming, and deleting.
- Makes capture visible and controllable through a small tray icon, global pause shortcut, and per-application controls.

### Non-goals for the first release

- Capturing passwords, secure controls, private-browser windows, payment fields, password managers, or Windows lock/UAC screens.
- Reading applications that intentionally block accessibility and do not provide an integration.
- Synchronizing content between computers.
- Cloud AI classification or title generation.
- Perfect semantic understanding of every third-party application's Send behavior.
- Guaranteeing recovery of a change that Windows never delivered to Lossy or that the storage device never physically committed.

## 2. Platform and technology decision

The first supported platform is **Windows 10/11 x64**.

| Layer | Choice | Reason |
|---|---|---|
| Background capture agent | Rust native executable | Low memory, strong concurrency safety, direct Windows API access, no visible window |
| Desktop interface | Tauri 2 + React + TypeScript | Small installer/runtime, native desktop window, fast UI development |
| Generic text capture | Windows UI Automation using `windows-rs` | Reads focused editor values without global raw-keystroke logging |
| Clipboard capture | `AddClipboardFormatListener` and Win32 clipboard APIs | Event-driven local text/image clipboard monitoring |
| Durable metadata | SQLite in WAL mode | Transactional, recoverable, proven local database |
| Content encryption | AES-256-GCM; master key protected with Windows DPAPI CurrentUser | Data is unreadable at rest without the user's Windows profile |
| IPC | Local named pipe restricted to the current user's SID | UI and integrations communicate with the agent without a network port |
| Web integration | Optional Manifest V3 browser extension + native messaging | Reliable tab, conversation, and editor identity for web applications |
| Cursor/VS Code integration | Optional local editor extension | Reliable workspace, file, chat, and composer identity |
| Packaging | Signed Tauri/NSIS installer | Normal Windows installation and clean uninstall |

The browser/editor companions are local integrations, not cloud services. Lossy must remain useful through generic Windows capture without them, but integrations provide much more reliable context identification.

## 3. High-level architecture

```text
┌──────────────────────────────── Capture sources ────────────────────────────────┐
│ Windows UI Automation │ Browser companion │ Cursor/VS Code │ Clipboard listener │
└───────────────┬─────────────────────┬────────────────┬───────────────┬───────────┘
                └─────────────────────┴───────┬────────┴───────────────┘
                                               v
┌────────────────────────────── lossy-agent.exe ──────────────────────────────────┐
│ Focus tracker → Context resolver → Privacy filter → Event normalizer/deduper    │
│                                      ↓                                          │
│                         Draft session state machine                              │
│                                      ↓                                          │
│                Dedicated durable writer → Encrypted local store                  │
│                                      ↓                                          │
│                  Indexer / retention / recovery / compaction                     │
└──────────────────────────────────────┬───────────────────────────────────────────┘
                                       │ current-user named pipe
                                       v
┌──────────────────────────────── lossy.exe ──────────────────────────────────────┐
│ Card timeline │ Search/filters │ Detail modal │ Editor │ Settings │ Diagnostics  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Process separation

Lossy uses two processes:

1. `lossy-agent.exe` is the lightweight, per-user background process. It owns capture, context resolution, encryption, and all database writes. It starts at user logon without opening a window.
2. `lossy.exe` is the Tauri interface. It is launched only when the user opens Lossy from Start, a shortcut, or the tray menu. Closing its window does not stop capture.

Only the agent may write to the database. This eliminates UI/agent write conflicts and prevents a slow render or crashed interface from interrupting capture.

## 4. Modules inside the background agent

### 4.1 Agent supervisor

- Acquires a named, current-user mutex so only one agent instance can run.
- Initializes logging, configuration, DPAPI key access, database recovery, and capture adapters.
- Tracks Windows logon, lock, unlock, sleep, resume, shutdown, and display-session changes.
- Restarts a failed adapter independently where possible.
- Exposes agent health and last-successful-write time to the UI.

### 4.2 Foreground and focus tracker

- Watches foreground-window and focused-control changes with WinEvent/UI Automation hooks.
- Maintains a monotonically increasing `focus_epoch` every time the active target changes.
- Collects process identity, executable signature/path, window identity, UI Automation tree data, browser/editor integration data, and the focused control.
- Never uses a window handle alone as permanent identity because handles are reused after applications close.

### 4.3 Context resolver

The resolver answers: **“Where is this text being written?”** It returns:

```text
ResolvedContext {
  app_id
  account_or_profile_id?
  container_id?          // browser profile, workspace, project, or document
  entity_id?             // conversation, chat, file, note, terminal tab, etc.
  editor_role            // main composer, reply composer, title field, document body
  stable_context_key
  display_label?
  confidence             // high, medium, or low
  source                 // browser, editor integration, app adapter, or generic UIA
  focus_epoch
}
```

The canonical draft identity is:

```text
app + account/profile + container + entity + editor role + draft generation
```

Raw identifiers are normalized and then stored as keyed hashes where the UI does not require the readable value. Display names such as a conversation name are encrypted.

### 4.4 Privacy filter

This is evaluated before text enters any durable queue.

Lossy must reject capture when any of these apply:

- The control reports itself as a password or secure text field.
- The active application is Lossy itself, a password manager, or on the denylist.
- The browser integration reports Incognito/InPrivate mode.
- The desktop is locked, displaying UAC, or is not the user's interactive desktop.
- The user has paused capture globally or for the current app/site.
- The field matches a conservative payment/OTP/secret-field rule configured by the user.
- An enterprise policy disables the application or site.

No rejected content is logged, cached, included in diagnostics, or placed into an error report.

### 4.5 Adapter manager

Adapters provide context and text snapshots with a common interface:

```text
identify_context()
read_editor_snapshot()
subscribe_to_changes()
classify_transition(previous, current)
health()
```

Priority for a given editing surface:

1. Purpose-built application integration
2. Browser or Cursor/VS Code companion
3. Application-specific UI Automation adapter
4. Generic UI Automation adapter
5. Unsupported/unknown state—do not guess or merge

Only one adapter is authoritative for a surface at a time. Lower-priority events are retained only as health signals, preventing duplicate capture.

### 4.6 Event normalizer and deduplicator

- Converts all adapter messages into `TextSnapshot`, `ContextChanged`, `SubmitHint`, `ClipboardChanged`, and lifecycle events.
- Assigns an event sequence, monotonic timestamp, wall-clock timestamp, adapter ID, and `focus_epoch`.
- Discards stale callbacks from an earlier focus epoch.
- Deduplicates identical snapshots by context key, value hash, selection state, and a short time window.
- Preserves Unicode exactly, including emoji, combining characters, right-to-left scripts, and line endings.
- Understands IME composition so candidate updates may exist in revision history without creating separate drafts.

### 4.7 Draft session state machine

Each context has an independent state machine:

```text
NEW → ACTIVE ⇄ SUSPENDED → COMPLETED
           ↘ CLEARED
           ↘ RECOVERED/ARCHIVED
```

- **NEW:** first non-empty observable text appears.
- **ACTIVE:** the associated editor currently has focus and is receiving changes.
- **SUSPENDED:** the user changed app, tab, conversation, file, or editor. The draft remains resumable.
- **COMPLETED:** strong evidence indicates Send/Submit occurred.
- **CLEARED:** content became empty without sufficient evidence of submission. Revisions remain recoverable.
- **RECOVERED/ARCHIVED:** user explicitly archives it or retention policy processes it.

Switching away never completes a draft. Returning to an identical stable context resumes its unfinished draft if the editor value is compatible.

### 4.8 Durable writer

- Runs on a dedicated high-priority storage thread; capture callbacks never perform database I/O.
- Accepts immutable events through a bounded in-memory channel.
- Uses short SQLite transactions in WAL mode with `synchronous=FULL`, foreign keys enabled, and a busy timeout.
- Commits every observable change in maximum-durability mode. An event is considered saved only after SQLite reports a successful commit.
- If multiple changes arrive faster than storage can commit, preserves ordering and may compact superseded snapshots only after the newest snapshot is durable.
- Uses prefix/suffix diffing for normal revisions and periodic encrypted full checkpoints, avoiding a full copy of a long document for every character.
- Applies backpressure by dropping nonessential diagnostic events first; draft content is never silently dropped.
- Reports disk-full, permission, encryption-key, and corruption failures immediately through tray state and Windows notification.

Typing is never delayed by disk activity because Lossy observes the application—it does not sit in the application's keyboard input path.

### 4.9 Recovery, compaction, and retention

- On startup, runs SQLite integrity checks appropriate to the previous shutdown state and replays valid WAL content.
- A partially committed transaction is ignored atomically; the prior committed draft remains valid.
- Periodically compacts old revision deltas into checkpoints without blocking capture.
- Keeps a small rotating encrypted database backup after successful integrity checks.
- Applies configurable retention separately to drafts, completed entries, clipboard text, and clipboard images.
- Pinned items are excluded from automatic deletion.
- Secure deletion is described honestly: Lossy removes keys/references and vacuums when practical, but SSD wear-leveling prevents an absolute physical overwrite guarantee.

## 5. Correctly separating conversations, documents, and editors

### 5.1 Stable context examples

| Surface | Preferred stable identity |
|---|---|
| WhatsApp Web | Browser profile + account + internal conversation ID + main/reply composer |
| WhatsApp Desktop | Account + selected conversation's stable accessibility/app identifier + composer role |
| Claude/Codex web | Browser profile + origin + conversation/task ID from page/URL + editor role |
| Cursor/VS Code | Window + normalized workspace URI + chat/session/file identifier + editor role |
| Notepad | Process + normalized document path; unsaved window receives a temporary document identity |
| Generic app | Signed executable identity + durable window/document accessibility path + editor AutomationId |

Human-readable titles are not sufficient by themselves. Two people can share a display name, browser tab titles can change, and window handles are temporary.

### 5.2 User A → User B → User A example

```text
WhatsApp / account-1 / conversation-A / composer → Draft A becomes ACTIVE
WhatsApp / account-1 / conversation-B / composer → Draft A SUSPENDED; Draft B ACTIVE
WhatsApp / account-1 / conversation-A / composer → Draft B SUSPENDED; Draft A resumes
```

The text observed immediately after returning is compared to Draft A's latest checkpoint:

- Exact match or compatible continuation: resume Draft A.
- Editor is empty and Draft A was already submitted: start a new draft generation.
- Text differs because the application restored an older value: preserve both revisions and mark a restoration event.
- Identity or compatibility is uncertain: create an isolated **Unidentified context** item instead of merging it into another draft.

### 5.3 Rapid context changes and race prevention

Accessibility focus and value events can arrive out of order. Lossy prevents cross-chat contamination with these rules:

1. Every focus change increments `focus_epoch`.
2. Value events include the epoch and source-control identity captured when subscribed.
3. After a rapid navigation, context resolution gets a short 20–50 ms stabilization window.
4. Events received during that window are buffered in memory.
5. The agent resolves context twice; if the results disagree, it retries rather than assigning text.
6. Buffered events are attached only when control identity and epoch still match.
7. Late events from the previous chat are discarded as stale, never appended to the new chat.

The stabilization window does not intentionally lose text; it delays classification by a few milliseconds while the source application continues operating normally.

### 5.4 Determining Send versus newline versus clear

Enter alone is not treated as Send. Completion confidence combines:

- An explicit submit-button or form-submit event from an integration.
- A non-empty editor becoming empty immediately after a submit action.
- A new outgoing message appearing whose content hash matches the draft, when an adapter can observe it safely.
- The application reporting a successful send transition.

A plain newline, loss of focus, application switch, or timeout only suspends the draft. When confidence is insufficient, Lossy keeps the item as an unfinished/cleared draft. False preservation is preferable to falsely declaring content finished or merging unrelated text.

## 6. Capture behavior

### 6.1 Text capture

The generic Windows adapter subscribes to focused editable-control value/text-change events. On each notification it requests the current editor value, not the physical keys that produced it. This means:

- Paste, autocorrect, voice dictation, emoji, and IME text can be recovered.
- Backspaces and replacements become revisions.
- Keyboard shortcuts and passwords are not reconstructed as a keystroke stream.
- An application that exposes neither its text value nor a companion API is reported as unsupported rather than secretly falling back to raw key capture.

Large editors use incremental ranges when the API provides them. Otherwise, Lossy hashes the snapshot first and skips unchanged content.

### 6.2 Clipboard text and image capture

The clipboard listener uses the Windows clipboard sequence number and reads data only after ownership settles.

Supported initial formats:

- Unicode plain text
- PNG
- Device-independent bitmap, converted to PNG
- File-drop images, stored as references only if the user enables file capture; otherwise ignored

Clipboard pipeline:

1. Receive clipboard-change notification.
2. Retry briefly if another process still owns the clipboard.
3. Apply privacy/app exclusions.
4. Determine foreground source context.
5. Normalize the supported format.
6. Compute a BLAKE3 content hash and deduplicate.
7. Encrypt and commit metadata/content.
8. Generate an image thumbnail asynchronously.

Lossy's own Copy action is tagged and ignored by the watcher to prevent loops. Multiple Windows representations of one copied image become one item. Image originals are stored losslessly; thumbnails are separate disposable cache files.

### 6.3 Headings and summaries

Heading generation is deterministic and local:

- Preferred prefix: readable context such as `WhatsApp · User A` or `Cursor · lossy`.
- Draft heading: first meaningful line, trimmed to approximately 60 characters.
- Clipboard image heading: source app plus `Copied image` and time.
- Empty/mostly-symbol content: app/context plus timestamp.
- The heading may update while a draft is very short, then becomes stable after meaningful content exists.
- User-edited headings are permanently locked and never regenerated.

Card snippets use a locally produced whitespace-normalized preview and never call an external model.

## 7. Persistence and data model

### 7.1 Storage locations

Use Windows Known Folders rather than hard-coded paths:

```text
%LOCALAPPDATA%\Lossy\
  data\lossy.db
  data\blobs\              # encrypted image/content blobs
  cache\thumbnails\        # rebuildable
  logs\                     # metadata-only rotating logs
  backups\                  # small rotating encrypted backups
```

The installer and executables live under `%LOCALAPPDATA%\Programs\Lossy` for a per-user install unless an administrator chooses a machine-wide deployment.

### 7.2 Core schema

```text
apps
  id, normalized_identity, encrypted_display_name, icon_cache_key

contexts
  id, app_id, stable_key_hash, encrypted_labels, context_type,
  adapter_id, confidence, first_seen_at, last_seen_at

drafts
  id, context_id, generation, status, encrypted_heading,
  encrypted_current_content, current_content_hash,
  created_at, updated_at, completed_at, pinned, user_heading_locked

revisions
  id, draft_id, sequence, encrypted_delta_or_checkpoint,
  content_hash, event_kind, observed_at, committed_at

clipboard_items
  id, source_context_id, kind, encrypted_heading,
  encrypted_text_or_blob_ref, content_hash, copied_at, pinned

blobs
  id, content_hash, encrypted_path, media_type, byte_length,
  width, height, created_at, reference_count

capture_rules
  id, scope_type, scope_hash, action, encrypted_label, created_at

agent_state
  schema_version, clean_shutdown, last_sequence, last_integrity_check
```

Every encrypted payload receives a unique random nonce and authenticated associated data containing its record type and ID. Encryption failures fail closed: unencrypted draft content must never be written as a fallback.

### 7.3 Search

Version 1 provides instant filtering by app, type, status, pin, and date using unencrypted non-sensitive metadata. Content search decrypts candidate headings/current snapshots in agent memory and streams matches to the UI. It does not build a plaintext SQLite full-text index.

If future scale requires encrypted token indexing, it must be designed separately and threat-modeled; it should not weaken the first release for convenience.

## 8. Silent automatic startup

### Required behavior

- During onboarding, the user explicitly enables **Start Lossy when I sign in**.
- The installer registers a per-user Windows logon task for `lossy-agent.exe --background`.
- The task runs only in the interactive user's session, without elevation and with no console/window.
- It is configured to restart after unexpected failure with bounded backoff.
- Automatic startup launches only the agent; it never launches the main Tauri window.
- A small tray icon indicates Running, Paused, or Error and offers Open Lossy, Pause, exclusions, and Quit.
- Normal logon produces no splash screen, onboarding window, or promotional notification.
- The first-ever launch and material privacy-policy changes require explicit onboarding before capture begins.

A Windows service must not be used for capture because services run in Session 0 and cannot safely observe a user's interactive desktop. Lossy is a per-user background agent.

### Shutdown and restart

- On normal shutdown/logoff, stop accepting new callbacks, finish the current transaction, checkpoint when safe, mark clean shutdown, and exit quickly.
- On sudden power loss, SQLite WAL recovery restores the last transaction confirmed by the storage device.
- On sleep/lock, capture subscriptions are stopped and sensitive buffers are cleared; they are recreated after unlock/resume.

## 9. Interface and user experience

### 9.1 Visual direction

Lossy should feel warm, cute, and calm—not like surveillance software.

- Warm off-white background: `#FFF8FB`
- Primary soft pink: `#F48FB1`
- Deep readable berry text: `#51233A`
- Pale blush surfaces: `#FDE7F0`
- Success mint accent: `#A8DDB5`
- Warning peach: `#F4B183`
- Rounded corners around 16–20 px, subtle borders, restrained shadows
- Friendly but highly readable sans-serif typography
- Small Lossy mascot/icon may appear in empty states; never reduce content density or clarity
- Motion limited to short opacity/transform transitions and disabled by Reduced Motion

### 9.2 Main window

The default view is a responsive card grid/list containing all saved items.

Each card shows:

- Heading
- Two-to-four-line content preview or image thumbnail
- Source app icon/name
- Context label when available
- Draft, Sent, Cleared, Clipboard, Image, or Unidentified badge
- Last-updated time
- Pin and quick-copy actions

Top controls:

- Local search
- Filters for All, Drafts, Clipboard, Images, Pinned, and Unidentified
- Application filter
- Capture status with Pause/Resume
- Settings button

Cards are grouped by Today, Yesterday, and older dates. Virtualization is used for large histories.

### 9.3 Detail modal

Clicking a card opens a large accessible dialog without navigating away:

- Editable heading
- Full editable text or full-resolution image preview
- Copy, Save changes, Pin, Export, Delete, and Close
- Source/context and timestamps
- Revision-history drawer with restore-as-new-item
- Previous/next item keyboard navigation

Editing inside Lossy must not be recaptured by Lossy. Copying from Lossy may update the Windows clipboard but must not create a duplicate clipboard-history item.

### 9.4 First-run onboarding

Onboarding is short but explicit:

1. Explain what Lossy captures and that data stays local.
2. Enable capture and auto-start separately.
3. Show secure-field/private-mode exclusions.
4. Offer browser and Cursor/VS Code companion installation.
5. Let the user choose retention and a global Pause/Resume shortcut.
6. Run a local test editor and confirm recovery works.

No capture begins before the user has completed the consent step.

### 9.5 Accessibility

- Full keyboard navigation and visible focus rings
- Correct dialog focus trapping and Escape behavior
- WCAG AA contrast despite the pale pink palette
- Screen-reader labels and semantic structure
- Reduced Motion support
- Text sizing up to 200% without clipping
- Never communicate item status through color alone

## 10. IPC and integration security

- Named pipe path contains a random installation ID and is ACL-restricted to the current user's SID.
- Every client performs a versioned handshake and challenge-response using an installation secret protected by DPAPI.
- UI receives paginated view models, not direct database access.
- Browser/editor integrations send the minimum necessary context and current editor snapshot.
- The agent validates message sizes, schema versions, UTF-8, sequence ordering, and adapter identity.
- No TCP listener or localhost web server is opened.
- Diagnostics redact content, conversation names, window titles, URLs, file paths, and clipboard data by default.
- Installer, agent, UI, and extensions should be signed/reproducibly built before public distribution.

## 11. Conflict and failure handling

| Situation | Required behavior |
|---|---|
| Two Lossy agents launch | Current-user mutex keeps one; the second asks the first to open UI or exits |
| UI and agent run together | Agent is sole database writer; UI uses IPC |
| Browser integration and UIA both report text | Integration wins; snapshot hash/epoch dedupe removes duplicates |
| User switches chats extremely quickly | Epoch validation and buffered context resolution prevent cross-chat merging |
| Chat identities are ambiguous | Create separate Unidentified item; never merge based only on a display name |
| Same chat starts a new message | Completed/cleared editor transition increments draft generation |
| App crashes with text present | Latest committed draft stays Active/Suspended and is highlighted as recoverable |
| Lossy UI crashes | Agent and capture continue unaffected |
| Agent crashes | Logon-task restart policy relaunches it; database transaction recovery runs |
| Sudden power loss | Recover the last storage-confirmed SQLite transaction from WAL |
| Disk becomes full | Stop acknowledging writes, retain a small bounded emergency memory queue, show Error tray state, never claim unsaved data is saved |
| Database corruption | Preserve original, open latest verified backup, replay valid WAL where possible, and show recovery report |
| Encryption key unavailable | Fail closed and pause capture; never create plaintext storage |
| App runs elevated | Do not auto-elevate Lossy; show unsupported/elevation mismatch and allow a deliberate user decision later |
| Clipboard temporarily locked | Retry with short bounded exponential backoff; skip safely if it remains unavailable |
| Same clipboard item appears repeatedly | Sequence number + content hash dedupes while preserving a new copy timestamp if configured |
| Huge clipboard image | Process off the capture thread, enforce configurable storage quota, and report rejection rather than freezing |
| Database/blob cleanup races with UI | Reference counts and agent transactions prevent deletion while an item is being read |
| Windows clock changes | Order with monotonic sequence; display corrected wall-clock time separately |
| App update changes its accessibility tree | Adapter health falls back to generic/Unidentified; never reuse an uncertain old context key |
| Lossy is paused during typing | Do not buffer secretly; resume starts from a fresh observed snapshot and records a visible capture gap |
| User deletes a draft while its editor is active | Suppress that generation until the editor changes or refocuses; do not immediately recreate the deleted card |
| Retention removes the latest revision | Retention operates on whole unpinned items/checkpoints, never leaves an unreconstructable delta chain |

## 12. Performance targets

These are engineering acceptance targets, not marketing guarantees:

- Agent idle memory: target below 50 MB without integrations.
- Idle CPU: target below 0.2% averaged over five minutes.
- Capture callback work: target below 1 ms before handing off to queues.
- Context classification: p95 below 50 ms for known adapters.
- Durable commit after an observed normal text change: p95 below 75 ms on a healthy SSD in maximum-durability mode.
- Main window first useful render: below 500 ms on the reference machine.
- Card scrolling: 60 FPS for 10,000 items through virtualization.
- Clipboard image handling must never block text capture or the UI thread.
- Background queues and adapter health metrics must be observable in Diagnostics without exposing content.

### Honest durability boundary

Lossy can save only after the source application exposes the changed value. With `synchronous=FULL`, it can request that Windows flush the transaction before marking it saved. An instantaneous power cut can still defeat hardware/controller caches, and some editors emit accessibility updates late. Therefore, “every physical keystroke is guaranteed” is impossible. The correct guarantee is: **Lossy preserves every text change that it observed and that the operating system/storage confirmed as committed, and it minimizes the time between observation and commit.**

## 13. Project structure

```text
lossy/
  apps/
    desktop-ui/             # Tauri + React interface
    agent/                  # Rust background executable
  crates/
    capture-core/           # normalized events and adapter contracts
    context-engine/         # stable identity and confidence rules
    windows-uia/            # Windows UI Automation implementation
    clipboard-win/          # Windows clipboard listener
    storage/                # encryption, SQLite, blobs, migrations, recovery
    ipc/                    # named-pipe protocol and authorization
    privacy/                # exclusion policy and secure-control rules
  integrations/
    browser-extension/      # Chromium/Edge first; Firefox later if needed
    vscode-extension/       # VS Code and Cursor companion
    adapters/               # application-specific resolver rules
  packages/
    ui-kit/                 # Lossy pink design tokens and components
    protocol-schema/        # generated versioned IPC schemas
  tests/
    fixtures/
    integration/
    power-failure/
    accessibility/
  installer/
  docs/
  plan.md
```

## 14. Delivery phases

### Phase 0 — Feasibility spikes

Before building the full UI, prove the risky parts on real Windows applications:

- Capture changing text through UI Automation in Notepad, Chrome/Edge, WhatsApp Desktop, Cursor, and one Electron chat app.
- Determine which surfaces require browser/editor companions.
- Validate secure-field detection and private-mode exclusion.
- Measure SQLite `synchronous=FULL` commit latency per observed change.
- Simulate rapid User A/User B switching and stale callbacks.
- Test clipboard text, PNG, DIB, and large-image behavior.

Exit condition: a written compatibility matrix and measured durability numbers. Unsupported surfaces are explicitly identified.

### Phase 1 — Reliable local core

- Agent supervisor, single-instance enforcement, lifecycle handling
- Generic UIA focus/text capture
- Clipboard text/image capture
- Context engine and draft state machine
- Privacy filter and application exclusions
- Encrypted SQLite/blob storage and migrations
- Power-loss/corruption recovery tests
- Named-pipe IPC and diagnostics

Exit condition: headless automated tests prove drafts never cross known contexts and committed data recovers after forced termination.

### Phase 2 — Simple polished interface

- Pink Lossy design system
- Virtualized card timeline and filters
- Detail modal, editing, copying, headings, pinning, deletion
- Revision history and restore-as-new
- Settings, retention, exclusions, storage usage, and diagnostic health
- Accessible onboarding and local test editor

Exit condition: the user can install Lossy, enable capture, close the UI, produce recoverable drafts, restart Windows, and recover/copy/edit them.

### Phase 3 — Reliable high-value integrations

- Chromium/Edge companion for Claude, Codex, WhatsApp Web, and generic web editors
- Cursor/VS Code companion
- App-specific WhatsApp Desktop resolver if stable APIs are available
- Integration versioning, health fallback, and compatibility tests

Exit condition: switching among two chats/tasks/workspaces creates separate drafts and reliably resumes each one.

### Phase 4 — Production hardening

- Signed installer and update mechanism with signature verification
- Logon task, restart policy, clean uninstall, and keep/delete-data choice
- Long-running memory/CPU/storage tests
- Fuzz IPC, database migrations, corrupt events, and Unicode input
- Automated sleep/wake, lock/unlock, crash, disk-full, and power-cut simulations
- Security review and external privacy audit before public release

## 15. Test matrix

### Context correctness

- Alternate 100 times between two WhatsApp conversations while typing.
- Alternate between two browser profiles with the same site open.
- Use identical contact display names and confirm internal identity separates them.
- Switch among Claude/Codex chats whose titles are identical.
- Switch Cursor workspaces, chat tabs, files, and main/reply composers.
- Close/reopen tabs and applications and verify stable contexts resume.
- Confirm uncertain contexts isolate rather than merge.

### Durability

- Terminate UI only; capture must continue.
- Terminate agent at every point around a transaction.
- Kill power in a Windows VM during continuous typing and recover the last committed revision.
- Fill the storage volume during text and image capture.
- Corrupt the latest WAL/database copy and exercise verified-backup recovery.
- Upgrade through every schema migration with existing encrypted content.

### Privacy and security

- Password inputs, Windows credential dialogs, payment forms, OTP fields, password managers, private browsing, lock screen, and UAC are never persisted.
- Paused/excluded applications generate no content, previews, diagnostics, or clipboard items.
- Another local user cannot open the database/key or IPC pipe.
- Malformed browser/editor IPC cannot crash the agent or allocate unbounded memory.
- Logs remain content-free even during errors.

### Input correctness

- Emoji, Hindi and other Indic scripts, Arabic/RTL, CJK IME, combining characters, multiline text, speech-to-text, paste, undo/redo, selection replacement, and autocorrect.
- Send with Enter, newline with Shift+Enter, clicking Send, clearing manually, navigation while non-empty, and app crash while non-empty.
- Clipboard PNG, DIB, transparency, high-DPI image, repeated image, unavailable clipboard, and huge image.

### UX and accessibility

- Keyboard-only operation, screen reader, 200% text, high contrast, Reduced Motion.
- 10,000 cards and large drafts without interaction lag.
- Main window never appears during normal automatic logon.
- Capture state is always understandable from the tray and settings.

## 16. Release acceptance criteria

Lossy v1 is ready only when all of the following are true:

1. It starts at logon in the current user's session without opening the main window.
2. Closing/crashing the UI does not stop the agent.
3. Supported application switches never mix two known contexts in stress tests.
4. An uncertain identity creates a separate Unidentified item rather than guessing.
5. Password/private/excluded surfaces persist zero content.
6. Every acknowledged event survives forced agent termination and database restart tests.
7. A sudden VM power-off recovers through SQLite WAL without database corruption.
8. Clipboard text and images are encrypted, deduplicated, previewable, copyable, and removable.
9. Cards have useful local headings, previews, source, context, state, and time.
10. The detail modal can edit, copy, rename, pin, delete, and restore revisions.
11. Idle CPU/memory and commit-latency targets are met on the reference system.
12. Uninstall clearly offers to preserve or remove the user's local Lossy data.

## 17. Decisions to keep the product simple

- Build Windows first; do not dilute reliability with simultaneous macOS/Linux work.
- Use integrations for context fidelity, UI Automation for broad fallback, and never raw global keystroke capture.
- Keep one authoritative background writer and one optional interface process.
- Prefer an isolated extra card over an incorrectly merged draft.
- Use deterministic local headings before considering any AI feature.
- Ship a small, excellent timeline/detail experience instead of folders, collaboration, sync, or complex note-taking.
- Make privacy controls and capture status obvious while keeping automatic startup visually quiet.

This architecture makes Lossy a focused recovery tool: invisible during normal work, extremely fast, conservative when context is uncertain, resilient to crashes, and pleasant when the user opens it to recover something.
