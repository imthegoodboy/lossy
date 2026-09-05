#![cfg(windows)]

use lossy_storage::{DraftContent, Store, StoreError};
use rusqlite::Connection;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

fn sample(text: &str) -> DraftContent {
    DraftContent {
        heading: "Synthetic pink note".into(),
        text: text.into(),
        source: "Synthetic chat A".into(),
    }
}

#[test]
fn committed_revisions_survive_reopen_without_plaintext_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lossy.db");
    let mut store = Store::open(&path).unwrap();
    let first = store
        .create([7; 32], &sample("Synthetic secret initial draft"))
        .unwrap();
    let second = store
        .update(
            first.id,
            1,
            &sample("नमस्ते 🌸\nSynthetic secret continued draft"),
        )
        .unwrap();
    // Inspect the live WAL too: content must be encrypted before SQLite ever sees it.
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        for secret in [
            "Synthetic secret",
            "Synthetic pink note",
            "Synthetic chat A",
        ] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
        }
    }
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.latest(first.id).unwrap(), second);
    assert_eq!(reopened.revision(first.id, 1).unwrap(), first);
    assert_eq!(reopened.list(20, 0).unwrap(), vec![second]);
    let conn = Connection::open(path).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
}

#[test]
fn stale_edits_and_deletes_do_not_overwrite_newer_capture() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lossy.db");
    let mut capture = Store::open(&path).unwrap();
    let first = capture
        .create([1; 32], &sample("Initial synthetic message"))
        .unwrap();
    let mut popup = Store::open(&path).unwrap();
    let second = capture
        .update(first.id, 1, &sample("Newer synthetic captured text"))
        .unwrap();
    assert_eq!(
        popup.update(first.id, 1, &sample("Stale popup")),
        Err(StoreError::Conflict)
    );
    assert_eq!(popup.delete(first.id, 1), Err(StoreError::Conflict));
    assert_eq!(capture.latest(first.id).unwrap(), second);
    popup.delete(first.id, 2).unwrap();
    assert_eq!(capture.latest(first.id), Err(StoreError::NotFound));
    assert_eq!(capture.revision(first.id, 1), Err(StoreError::NotFound));
}

#[test]
fn failed_revision_insert_rolls_back_current_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lossy.db");
    let mut store = Store::open(&path).unwrap();
    let first = store
        .create([1; 32], &sample("Durable synthetic text"))
        .unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER injected_failure BEFORE INSERT ON revisions BEGIN SELECT RAISE(ABORT, 'synthetic fault'); END;").unwrap();
    assert_eq!(
        store.update(first.id, 1, &sample("Should not commit")),
        Err(StoreError::Database)
    );
    assert_eq!(store.latest(first.id).unwrap(), first);
    store.verify_integrity().unwrap();
}

#[test]
fn damaged_or_missing_wrapped_key_fails_without_replacement() {
    for missing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lossy.db");
        let mut store = Store::open(&path).unwrap();
        store
            .create([1; 32], &sample("Preserve even when locked out"))
            .unwrap();
        drop(store);
        let conn = Connection::open(&path).unwrap();
        if missing {
            conn.execute("DELETE FROM metadata", []).unwrap();
        } else {
            conn.execute("UPDATE metadata SET wrapped_key=x'00010203'", [])
                .unwrap();
        }
        let original: Vec<u8> = conn
            .query_row("SELECT payload FROM revisions", [], |r| r.get(0))
            .unwrap();
        assert!(matches!(
            Store::open(&path),
            Err(StoreError::KeyUnavailable)
        ));
        let unchanged: Vec<u8> = conn
            .query_row("SELECT payload FROM revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(original, unchanged);
    }
}

#[test]
fn ciphertext_swaps_and_metadata_tampering_are_rejected() {
    for tamper in [
        "UPDATE revisions SET payload=zeroblob(40)",
        "UPDATE drafts SET context=zeroblob(32)",
        "UPDATE revisions SET updated_ms=updated_ms+1",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lossy.db");
        let mut store = Store::open(&path).unwrap();
        store
            .create([1; 32], &sample("Authenticated synthetic message"))
            .unwrap();
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute(tamper, [])
            .unwrap();
        assert!(matches!(
            Store::open(&path),
            Err(StoreError::Authentication)
        ));
    }
}

#[test]
fn verified_backup_can_be_opened_and_never_overwrites_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("lossy.db")).unwrap();
    let item = store
        .create([1; 32], &sample("Synthetic backup checkpoint"))
        .unwrap();
    let backup = dir.path().join("verified.db");
    store.backup(&backup).unwrap();
    assert_eq!(Store::open(&backup).unwrap().latest(item.id).unwrap(), item);
    assert_eq!(store.backup(&backup), Err(StoreError::Database));
}

#[test]
fn unknown_existing_database_is_not_initialized_or_erased() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE unrelated(value TEXT); INSERT INTO unrelated VALUES('synthetic');",
    )
    .unwrap();
    assert!(matches!(Store::open(&path), Err(StoreError::InvalidSchema)));
    assert_eq!(
        conn.query_row("SELECT value FROM unrelated", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "synthetic"
    );
}

#[test]
fn payload_limits_and_debug_redaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("lossy.db")).unwrap();
    assert_eq!(
        store.create([1; 32], &sample(&"x".repeat(8 * 1024 * 1024))),
        Err(StoreError::TooLarge)
    );
    let saved = store
        .create([1; 32], &sample("Synthetic private sentinel"))
        .unwrap();
    let debug = format!("{saved:?}");
    assert!(!debug.contains("Synthetic private sentinel"));
    assert!(!debug.contains("Synthetic pink note"));
}

#[test]
fn crash_writer() {
    let Some(path) = std::env::var_os("LOSSY_SYNTHETIC_CRASH_DB") else {
        return;
    };
    let mut store = Store::open(path).unwrap();
    let item = store
        .create([1; 32], &sample("Synthetic first checkpoint"))
        .unwrap();
    store
        .update(
            item.id,
            1,
            &sample("Synthetic last acknowledged checkpoint"),
        )
        .unwrap();
    println!("LOSSY_COMMITTED");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn forced_process_termination_recovers_acknowledged_wal_commit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crash.db");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_writer", "--nocapture"])
        .env("LOSSY_SYNTHETIC_CRASH_DB", &path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(output)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if line.contains("LOSSY_COMMITTED") {
                let _ = tx.send(());
                break;
            }
        }
    });
    let ready = rx.recv_timeout(Duration::from_secs(20));
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    ready.expect("child must acknowledge the durable commit before termination");
    let store = Store::open(path).unwrap();
    let items = store.list(20, 0).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].revision, 2);
    assert_eq!(
        items[0].content.text,
        "Synthetic last acknowledged checkpoint"
    );
    assert_eq!(
        store.revision(items[0].id, 1).unwrap().content.text,
        "Synthetic first checkpoint"
    );
}
