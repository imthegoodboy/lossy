# Encrypted revision storage

The `lossy-storage` crate provides a Windows-only local store. Each create/update returns
only after its SQLite transaction commits with WAL and `synchronous=FULL`. The desktop and
agent now use the store; tests use synthetic content in temporary folders.

## Data and keys

Every database receives a random 256-bit master key. Windows DPAPI protects it for the
current user with UI forbidden and without machine-wide protection. Schema creation,
wrapped-key insertion, and an authenticated key-check record commit together.
An existing database with missing or unreadable key metadata fails to open. It is never reset.
An interrupted first initialization can leave an uninitialized file; that file also fails closed
and requires explicit recovery instead of silently generating another key.

Headings, text, and source labels are serialized and encrypted together using AES-256-GCM.
Each checkpoint uses a fresh OS-random 96-bit nonce. Its authenticated data binds the payload
to its item ID, context key, revision, and timestamp. Temporary serialized plaintext and key
buffers are wiped on drop. Returned text remains in agent/UI memory for use by the application;
this does not protect against malware already running as the same Windows user.

SQLite schema, random IDs, opaque context keys, revision counts, sizes, and timestamps are
not encrypted. Raw text, headings, source labels, and plaintext search indexes never enter SQLite.
Context keys supplied by callers must already be keyed hashes; the agent uses a per-install secret.
Kind, pinned status and active-context ownership are also unencrypted metadata in schema v2.

## Revision rules

- Each item has a random ID; every saved update appends an encrypted full checkpoint.
- Updating requires the version the caller loaded. A stale edit returns `Conflict`.
- Updating the current revision pointer and inserting the checkpoint share one transaction.
- Deleting requires the current version and removes dependent revisions using a foreign key.
- Explicit backups are staged, reopened, verified, and published without overwriting a destination.
  Failed verification cleans up the staging file so the requested destination remains retryable.
- Reads and integrity checks reject a current-revision pointer rolled back below the latest checkpoint.
- Corrupt storage never triggers deletion or silent fallback to an older backup.

Full checkpoints keep recovery independent of delta chains. The runtime compacts old revisions,
expires unpinned items and rotates three verified backups. Images are capped encrypted base64 PNG
payloads in the same store. Active context ownership is committed atomically with new drafts.
The current payload cap is 8 MiB per checkpoint and result pages are capped at 200 items.

## Verification and boundaries

Tests cover Unicode recovery, raw database/WAL plaintext checks, stale edits/deletes,
rollback after an injected insertion failure, missing/damaged DPAPI metadata, altered ciphertext
and metadata, verified backups, unknown schema preservation, payload limits, and debug redaction.
A child-process test commits two revisions, signals the parent, and is forcibly terminated;
reopening must recover both acknowledged revisions without a clean SQLite close.

That test verifies process-crash recovery, not physical power-loss behavior. Hardware/storage
flush behavior still needs VM power-cut and device testing. No claim is made that physical keys
not yet observed by the capture adapter are recoverable.

Opening checks SQLite structure. Payloads authenticate on read, while explicit Verify and backups
scan all history. Large searches and verified backups can delay the single writer; separate
maintenance scheduling and disk-size quotas remain production scaling improvements.

Native DPAPI calls are confined to `dpapi.rs`, with documented pointer ownership and cleanup.
The rest of the storage crate denies unsafe code; the platform-independent crates forbid it.

References: [SQLite durability settings](https://www.sqlite.org/pragma.html#pragma_synchronous),
[Windows DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata),
[RustCrypto AES-GCM](https://docs.rs/aes-gcm/0.11.1/aes_gcm/).
