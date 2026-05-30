//! Tests for `flush`, which force-writes every page resident in the file to
//! disk (not just the dirty ones) and fsyncs.
//!
//! To prove `flush` alone made the data durable, each test leaks the handle
//! with `std::mem::forget` after flushing, so the `Drop`/`sync` path never runs.
//! If the data is still there on reopen, `flush` is what persisted it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use btree::{Db, Error};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TmpDir(PathBuf);
impl TmpDir {
    fn new() -> TmpDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("btree_flush_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        TmpDir(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fill(db: &mut Db, range: std::ops::Range<u32>, val: &str) {
    for i in range {
        db.insert(format!("key{i:06}").as_bytes(), val.as_bytes())
            .unwrap();
    }
}

#[test]
fn flush_persists_every_page_without_drop() {
    let tmp = TmpDir::new();
    let path = tmp.path("main.db");

    let mut db = Db::open(&path).unwrap();
    fill(&mut db, 0..2000, "v0");
    db.flush().unwrap();
    std::mem::forget(db); // simulate a crash right after flush: Drop never runs

    let db = Db::open(&path).unwrap();
    assert_eq!(db.scan(None, None).unwrap().len(), 2000);
    for i in (0..2000).step_by(41) {
        assert_eq!(
            db.get(format!("key{i:06}").as_bytes()).unwrap().as_deref(),
            Some(&b"v0"[..]),
            "key {i} not durable after flush"
        );
    }
}

#[test]
fn flush_rewrites_clean_pages_after_a_sync() {
    // After a sync there are no dirty pages, so flush must re-read every
    // resident page from disk and write it back. Verify it still persists.
    let tmp = TmpDir::new();
    let path = tmp.path("main.db");

    let mut db = Db::open(&path).unwrap();
    fill(&mut db, 0..1500, "v0");
    db.sync().unwrap(); // dirty set now empty; pages already on disk
    db.flush().unwrap(); // force-rewrite all resident pages anyway
    std::mem::forget(db);

    let db = Db::open(&path).unwrap();
    assert_eq!(db.scan(None, None).unwrap().len(), 1500);
    assert_eq!(db.get(b"key000999").unwrap().as_deref(), Some(&b"v0"[..]));
}

#[test]
fn flush_on_overlay_persists_resident_pages_and_spares_the_base() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");
    let work = tmp.path("work.db");

    {
        let mut db = Db::open(&main).unwrap();
        fill(&mut db, 0..2000, "base");
        db.checkpoint(&snap).unwrap();
    }
    let snap_before = std::fs::read(&snap).unwrap();

    // Overlay: copy up some pages (edits) and add native pages (fresh), flush.
    let mut ov = Db::open_overlay(&work, &snap).unwrap();
    fill(&mut ov, 0..500, "edited"); // copy-up of base pages
    fill(&mut ov, 2000..2300, "fresh"); // native pages
    ov.flush().unwrap();
    std::mem::forget(ov); // only flush should have persisted things

    // The base snapshot is byte-for-byte untouched: flush wrote only this file.
    assert_eq!(std::fs::read(&snap).unwrap(), snap_before);

    // Reopen the overlay; resident pages (copied + native) survived the flush,
    // and untouched base pages still read through.
    let ov = Db::open(&work).unwrap();
    assert_eq!(
        ov.get(b"key000123").unwrap().as_deref(),
        Some(&b"edited"[..])
    ); // copied up
    assert_eq!(ov.get(b"key001234").unwrap().as_deref(), Some(&b"base"[..])); // read-through
    assert_eq!(
        ov.get(b"key002100").unwrap().as_deref(),
        Some(&b"fresh"[..])
    ); // native
    assert_eq!(ov.scan(None, None).unwrap().len(), 2300);
}

#[test]
fn flush_is_refused_on_a_frozen_database() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");

    let mut db = Db::open(&main).unwrap();
    fill(&mut db, 0..10, "v0");
    db.checkpoint(&snap).unwrap();

    // Frozen: flushing would write the shared snapshot inode, so it's refused.
    assert!(matches!(db.flush(), Err(Error::Frozen)));
}
