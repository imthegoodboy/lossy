//! Encrypted draft checkpoints. A write is acknowledged only after a FULL WAL commit.
//! The caller owns the single writer; stale edits require an explicit conflict resolution.

mod crypto;
#[cfg(windows)]
mod dpapi;

use crypto::Cipher;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{
    fmt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

pub type Result<T> = std::result::Result<T, StoreError>;
const SCHEMA_VERSION: i64 = 1;
const MAX_PAYLOAD: usize = 8 * 1024 * 1024;
const KEY_AAD: &[u8] = b"lossy/key-check/v1";

/// Error messages intentionally omit paths, SQL values, keys, and captured content.
#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    Database,
    KeyUnavailable,
    Authentication,
    Randomness,
    UnsupportedPlatform,
    InvalidSchema,
    Corrupt,
    Conflict,
    NotFound,
    TooLarge,
    InvalidPayload,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lossy storage: {self:?}")
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Database
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemId(pub [u8; 16]);

/// Plaintext exists only in caller/agent memory. Debug formatting is redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct DraftContent {
    pub heading: String,
    pub text: String,
    pub source: String,
}
impl fmt::Debug for DraftContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DraftContent([redacted])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedDraft {
    pub id: ItemId,
    pub context: [u8; 32],
    pub revision: i64,
    pub updated_ms: i64,
    pub content: DraftContent,
}

/// A per-user store. Opening an existing database never regenerates a missing key.
pub struct Store {
    connection: Connection,
    cipher: Cipher,
}

impl Store {
    #[cfg(windows)]
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let existed = path.try_exists().map_err(|_| StoreError::Database)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if existed && version != SCHEMA_VERSION {
            return Err(StoreError::InvalidSchema);
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "trusted_schema", false)?;
        connection.pragma_update(None, "temp_store", "MEMORY")?;

        let cipher = if !existed {
            let mut key = Zeroizing::new([0; 32]);
            getrandom::fill(key.as_mut()).map_err(|_| StoreError::Randomness)?;
            let wrapped = dpapi::protect(key.as_ref())?;
            let cipher = Cipher::new(key.as_ref())?;
            let check = cipher.seal(b"lossy-v1", KEY_AAD)?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(include_str!("schema.sql"))?;
            tx.execute(
                "INSERT INTO metadata(id, wrapped_key, key_check) VALUES(1, ?1, ?2)",
                params![wrapped, check],
            )?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
            cipher
        } else {
            let (wrapped, check): (Vec<u8>, Vec<u8>) = connection
                .query_row(
                    "SELECT wrapped_key, key_check FROM metadata WHERE id=1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|_| StoreError::KeyUnavailable)?;
            let key = Zeroizing::new(dpapi::unprotect(&wrapped)?);
            let cipher = Cipher::new(&key)?;
            if cipher.open(&check, KEY_AAD)?.as_slice() != b"lossy-v1" {
                return Err(StoreError::Authentication);
            }
            cipher
        };
        let store = Self { connection, cipher };
        store.verify_integrity()?;
        Ok(store)
    }

    #[cfg(not(windows))]
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        Err(StoreError::UnsupportedPlatform)
    }

    pub fn create(&mut self, context: [u8; 32], content: &DraftContent) -> Result<SavedDraft> {
        let mut id = [0; 16];
        getrandom::fill(&mut id).map_err(|_| StoreError::Randomness)?;
        let id = ItemId(id);
        let updated_ms = now_ms();
        let sealed = self
            .cipher
            .seal(&encode(content)?, &aad(id, context, 1, updated_ms))?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO drafts(id, context, current_revision) VALUES(?1, ?2, 1)",
            params![id.0.as_slice(), context.as_slice()],
        )?;
        tx.execute(
            "INSERT INTO revisions(draft_id, revision, updated_ms, payload) VALUES(?1, 1, ?2, ?3)",
            params![id.0.as_slice(), updated_ms, sealed],
        )?;
        tx.commit()?;
        Ok(SavedDraft {
            id,
            context,
            revision: 1,
            updated_ms,
            content: content.clone(),
        })
    }

    /// Returns Conflict if another update committed after the caller loaded this revision.
    pub fn update(
        &mut self,
        id: ItemId,
        expected_revision: i64,
        content: &DraftContent,
    ) -> Result<SavedDraft> {
        let latest = self.latest(id)?;
        if expected_revision != latest.revision {
            return Err(StoreError::Conflict);
        }
        let revision = expected_revision
            .checked_add(1)
            .ok_or(StoreError::Conflict)?;
        let updated_ms = now_ms();
        let sealed = self.cipher.seal(
            &encode(content)?,
            &aad(id, latest.context, revision, updated_ms),
        )?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE drafts SET current_revision=?1 WHERE id=?2 AND current_revision=?3",
            params![revision, id.0.as_slice(), expected_revision],
        )?;
        if changed != 1 {
            return Err(StoreError::Conflict);
        }
        tx.execute(
            "INSERT INTO revisions(draft_id, revision, updated_ms, payload) VALUES(?1, ?2, ?3, ?4)",
            params![id.0.as_slice(), revision, updated_ms, sealed],
        )?;
        tx.commit()?;
        Ok(SavedDraft {
            id,
            context: latest.context,
            revision,
            updated_ms,
            content: content.clone(),
        })
    }

    pub fn latest(&self, id: ItemId) -> Result<SavedDraft> {
        let revision = self
            .connection
            .query_row(
                "SELECT current_revision FROM drafts WHERE id=?1",
                [id.0.as_slice()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound)?;
        self.revision(id, revision)
    }

    pub fn revision(&self, id: ItemId, revision: i64) -> Result<SavedDraft> {
        let (context, updated_ms, sealed): (Vec<u8>, i64, Vec<u8>) = self.connection.query_row(
            "SELECT d.context, r.updated_ms, r.payload FROM drafts d JOIN revisions r ON r.draft_id=d.id WHERE d.id=?1 AND r.revision=?2",
            params![id.0.as_slice(), revision], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?.ok_or(StoreError::NotFound)?;
        let context: [u8; 32] = context.try_into().map_err(|_| StoreError::Corrupt)?;
        let plain = self
            .cipher
            .open(&sealed, &aad(id, context, revision, updated_ms))?;
        Ok(SavedDraft {
            id,
            context,
            revision,
            updated_ms,
            content: decode(&plain)?,
        })
    }

    /// Bounded page of newest current snapshots; callers can search decrypted pages in memory.
    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<SavedDraft>> {
        let mut statement = self.connection.prepare("SELECT d.id FROM drafts d JOIN revisions r ON r.draft_id=d.id AND r.revision=d.current_revision ORDER BY r.updated_ms DESC, d.id LIMIT ?1 OFFSET ?2")?;
        let ids = statement
            .query_map(params![limit.min(200), offset], |r| r.get::<_, Vec<u8>>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| self.latest(ItemId(id.try_into().map_err(|_| StoreError::Corrupt)?)))
            .collect()
    }

    pub fn delete(&mut self, id: ItemId, expected_revision: i64) -> Result<()> {
        let changed = self.connection.execute(
            "DELETE FROM drafts WHERE id=?1 AND current_revision=?2",
            params![id.0.as_slice(), expected_revision],
        )?;
        if changed != 1 {
            return Err(StoreError::Conflict);
        }
        Ok(())
    }

    /// Explicit verified snapshot. Refuses to overwrite an existing backup.
    pub fn backup(&self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = destination.as_ref();
        let reservation = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|_| StoreError::Database)?;
        drop(reservation);
        let mut backup =
            Connection::open_with_flags(destination, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let job = rusqlite::backup::Backup::new(&self.connection, &mut backup)?;
        job.run_to_completion(128, Duration::from_millis(1), None)?;
        drop(job);
        drop(backup);
        Self::open(destination)?.verify_integrity()
    }

    /// SQLite integrity plus authenticated reads of all checkpoints. Run at open/backup,
    /// not inside capture callbacks. Corruption never triggers automatic deletion or reset.
    pub fn verify_integrity(&self) -> Result<()> {
        let check: String = self
            .connection
            .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(StoreError::Corrupt);
        }
        let violations: i64 = self.connection.query_row(
            "SELECT count(*) FROM pragma_foreign_key_check",
            [],
            |r| r.get(0),
        )?;
        if violations != 0 {
            return Err(StoreError::Corrupt);
        }
        let mut statement = self
            .connection
            .prepare("SELECT draft_id, revision FROM revisions")?;
        let rows =
            statement.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (id, revision) = row?;
            self.revision(
                ItemId(id.try_into().map_err(|_| StoreError::Corrupt)?),
                revision,
            )?;
        }
        let missing: i64 = self.connection.query_row("SELECT count(*) FROM drafts d WHERE NOT EXISTS (SELECT 1 FROM revisions r WHERE r.draft_id=d.id AND r.revision=d.current_revision)", [], |r| r.get(0))?;
        if missing != 0 {
            return Err(StoreError::Corrupt);
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis().min(i64::MAX as u128) as i64)
}

fn aad(id: ItemId, context: [u8; 32], revision: i64, timestamp: i64) -> Vec<u8> {
    let mut output = b"lossy/draft-checkpoint/v1".to_vec();
    output.extend(id.0);
    output.extend(context);
    output.extend(revision.to_le_bytes());
    output.extend(timestamp.to_le_bytes());
    output
}

fn encode(content: &DraftContent) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    for value in [&content.heading, &content.text, &content.source] {
        if value.len() > MAX_PAYLOAD || bytes.len() + value.len() + 4 > MAX_PAYLOAD {
            return Err(StoreError::TooLarge);
        }
        bytes.extend((value.len() as u32).to_le_bytes());
        bytes.extend(value.as_bytes());
    }
    Ok(bytes)
}

fn decode(mut bytes: &[u8]) -> Result<DraftContent> {
    if bytes.len() > MAX_PAYLOAD {
        return Err(StoreError::TooLarge);
    }
    let mut fields = Vec::new();
    for _ in 0..3 {
        let length = bytes.get(..4).ok_or(StoreError::InvalidPayload)?;
        let length =
            u32::from_le_bytes(length.try_into().map_err(|_| StoreError::InvalidPayload)?) as usize;
        bytes = &bytes[4..];
        let text = std::str::from_utf8(bytes.get(..length).ok_or(StoreError::InvalidPayload)?)
            .map_err(|_| StoreError::InvalidPayload)?;
        fields.push(text.to_owned());
        bytes = &bytes[length..];
    }
    if !bytes.is_empty() {
        return Err(StoreError::InvalidPayload);
    }
    let mut fields = fields.into_iter();
    Ok(DraftContent {
        heading: fields.next().unwrap(),
        text: fields.next().unwrap(),
        source: fields.next().unwrap(),
    })
}
