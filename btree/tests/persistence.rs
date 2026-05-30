//! End-to-end persistence tests: write data, shut the tree down, reopen it from
//! disk, and verify the data is still there.
//!
//! A `Db` is shut down by dropping it (its `Drop` impl calls `sync`). Each test
//! opens a fresh `Db` over the same file to prove the data round-tripped through
//! the filesystem rather than living only in memory.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use btree::Db;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A self-deleting temp path shared by every reopen in a single test.
struct TmpPath(PathBuf);
impl TmpPath {
    fn new() -> TmpPath {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("btree_persist_{}_{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&path);
        TmpPath(path)
    }
    fn open(&self) -> Db {
        Db::open(&self.0).unwrap()
    }
}
impl Drop for TmpPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn data_survives_graceful_shutdown() {
    let tmp = TmpPath::new();

    // Session 1: write, then shut down by dropping the handle.
    {
        let mut db = tmp.open();
        db.insert(b"name", b"helios").unwrap();
        db.insert(b"kind", b"btree").unwrap();
        db.insert(b"wal", b"none").unwrap();
    } // <- shutdown: Drop flushes to disk

    // Session 2: a brand new handle reads it back from the file.
    let db = tmp.open();
    assert_eq!(db.get(b"name").unwrap().as_deref(), Some(&b"helios"[..]));
    assert_eq!(db.get(b"kind").unwrap().as_deref(), Some(&b"btree"[..]));
    assert_eq!(db.get(b"wal").unwrap().as_deref(), Some(&b"none"[..]));
    assert_eq!(db.scan(None, None).unwrap().len(), 3);
}

#[test]
fn explicit_sync_is_visible_to_a_new_handle() {
    let tmp = TmpPath::new();

    let mut writer = tmp.open();
    for i in 0..200u32 {
        writer.insert(format!("k{i:04}").as_bytes(), b"v").unwrap();
    }
    writer.sync().unwrap(); // durable now, without dropping `writer`

    // Open a second handle over the same file while the first is still alive.
    let reader = tmp.open();
    assert_eq!(reader.scan(None, None).unwrap().len(), 200);
    assert_eq!(reader.get(b"k0123").unwrap().as_deref(), Some(&b"v"[..]));
}

#[test]
fn data_accumulates_across_many_restarts() {
    let tmp = TmpPath::new();

    // Five separate sessions, each adding its own batch and shutting down.
    for session in 0..5u32 {
        let mut db = tmp.open();
        for i in 0..1000u32 {
            let key = format!("s{session}-{i:04}");
            db.insert(key.as_bytes(), format!("session{session}").as_bytes())
                .unwrap();
        }
    }

    // Final session: every batch from every prior session must be present.
    let db = tmp.open();
    assert_eq!(db.scan(None, None).unwrap().len(), 5 * 1000);
    for session in 0..5u32 {
        for i in (0..1000u32).step_by(97) {
            let key = format!("s{session}-{i:04}");
            assert_eq!(
                db.get(key.as_bytes()).unwrap().as_deref(),
                Some(format!("session{session}").as_bytes()),
                "missing {key} after restarts"
            );
        }
    }
}

#[test]
fn deletes_persist_across_restart() {
    let tmp = TmpPath::new();

    // Session 1: insert 2000 keys.
    {
        let mut db = tmp.open();
        for i in 0..2000u32 {
            db.insert(format!("{i:05}").as_bytes(), b"x").unwrap();
        }
    }
    // Session 2: delete every even key, then shut down.
    {
        let mut db = tmp.open();
        for i in (0..2000u32).step_by(2) {
            assert!(db.delete(format!("{i:05}").as_bytes()).unwrap());
        }
    }
    // Session 3: deletions stuck; odds remain, evens are gone.
    let db = tmp.open();
    assert_eq!(db.scan(None, None).unwrap().len(), 1000);
    for i in 0..2000u32 {
        let present = db.get(format!("{i:05}").as_bytes()).unwrap().is_some();
        assert_eq!(present, i % 2 == 1, "key {i} wrong presence after restart");
    }
}

#[test]
fn updates_persist_across_restart() {
    let tmp = TmpPath::new();

    {
        let mut db = tmp.open();
        for i in 0..500u32 {
            db.insert(format!("k{i:04}").as_bytes(), b"old").unwrap();
        }
    }
    {
        let mut db = tmp.open();
        for i in (0..500u32).step_by(2) {
            db.insert(format!("k{i:04}").as_bytes(), b"new").unwrap();
        }
    }
    let db = tmp.open();
    for i in 0..500u32 {
        let want: &[u8] = if i % 2 == 0 { b"new" } else { b"old" };
        assert_eq!(
            db.get(format!("k{i:04}").as_bytes()).unwrap().as_deref(),
            Some(want),
            "key {i} has wrong value after restart"
        );
    }
}

#[test]
fn multi_level_tree_survives_restart() {
    let tmp = TmpPath::new();
    let n = 20_000u32;

    // Big enough to force a tree several levels deep (internal pages + leaf
    // chain), so the restart exercises the whole on-disk structure.
    {
        let mut db = tmp.open();
        for i in 0..n {
            let key = format!("key-{i:08}");
            let val = format!("value-for-{i}");
            db.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
    }

    let db = tmp.open();

    // Full ordered scan is complete and sorted.
    let all = db.scan(None, None).unwrap();
    assert_eq!(all.len(), n as usize);
    assert!(all.windows(2).all(|w| w[0].0 < w[1].0), "scan not sorted");

    // Random-ish point lookups all resolve to the right value.
    for i in (0..n).step_by(331) {
        let key = format!("key-{i:08}");
        assert_eq!(
            db.get(key.as_bytes()).unwrap().as_deref(),
            Some(format!("value-for-{i}").as_bytes()),
            "lookup {i} failed after restart"
        );
    }

    // A bounded range comes back correct after restart too.
    let lo = b"key-00010000";
    let hi = b"key-00010010";
    let range = db.scan(Some(lo), Some(hi)).unwrap();
    let got: Vec<u32> = range
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(&k[4..]).parse().unwrap())
        .collect();
    assert_eq!(got, (10000..10010).collect::<Vec<u32>>());
}

#[test]
fn unsynced_writes_are_lost_but_synced_ones_persist() {
    // Documents the durability boundary: with no WAL, only data that reached
    // disk via `sync` (or a graceful drop) survives a crash. We simulate a crash
    // by leaking the handle so its `Drop`/flush never runs.
    let tmp = TmpPath::new();

    let mut db = tmp.open();
    db.insert(b"durable", b"1").unwrap();
    db.sync().unwrap(); // this reaches disk

    db.insert(b"volatile", b"2").unwrap(); // buffered only, never synced

    // "Crash": skip Drop (and therefore the flush) entirely.
    std::mem::forget(db);

    let db = tmp.open();
    assert_eq!(
        db.get(b"durable").unwrap().as_deref(),
        Some(&b"1"[..]),
        "synced write must survive the crash"
    );
    assert_eq!(
        db.get(b"volatile").unwrap(),
        None,
        "unsynced write must not survive the crash"
    );
}
