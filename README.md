# Lossy

Lossy is a Windows-first, fully local draft-recovery and smart-clipboard app. It runs quietly in the signed-in user's session, keeps supported text drafts separated by application and context, and helps recover text or copied images after crashes, navigation mistakes, and restarts.

## Status

The Windows app has a single plain page of saved text and images: no logo, navigation, search,
action buttons or editor popups. It includes a windowless tray agent, opt-in native capture, encrypted persistence,
retention, rotating backups and a Chrome/Edge companion. See the [user guide](docs/user-guide.md)
for installation, supported sources, limits and recovery. This first release is unsigned.

Read the complete [product and technical architecture](./plan.md).

## Core principles

- Local-only storage with no account, cloud sync, or telemetry
- No raw global keystroke logging
- Secure fields and private browsing excluded
- Separate drafts for different conversations, workspaces, documents, and editors
- Silent background startup with an on-demand desktop interface
- Encrypted crash-safe storage for text and clipboard images

## Target platform

The first release targets Windows 10/11 x64.

## Development

Windows 10/11 x64, Rust 1.88+, Node.js 22+, WebView2 and the Windows MSVC build tools are required.

```powershell
cd apps/desktop
npm ci
npm run build
cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cd apps/desktop
npm run tauri -- build
```

Installer output is under `target/release/bundle/nsis`. For development, run `npm run tauri -- dev`
from `apps/desktop`. Use `-j 1` for Cargo builds on low-memory machines.

Download packaged builds from [Releases](https://github.com/imthegoodboy/lossy/releases).
Accept the one-time inline capture permission; then enable the
[browser companion](docs/browser-companion.md) on selected websites.

Native editors are sampled approximately every 35 ms and clipboard changes every 100 ms.
Browser input events forward immediately, with a 200 ms fallback for programmatic clearing.
These are sampling intervals, not latency guarantees. Only **committed, observed** snapshots
are recoverable: no app can guarantee recovering a key not yet exposed by the source app or
flushed to disk before power loss. Unsupported/private/protected fields are skipped.

Implementation changes are developed on focused branches and merged through pull requests.

The [storage design and verification notes](docs/storage.md) describe the current persistence
boundary, failure behavior, and tests. Runtime drafts and databases must never be committed.

## Security

Lossy handles sensitive text. Do not include captured content, window titles, URLs, file paths, or clipboard data in logs, test fixtures, screenshots, or bug reports.
