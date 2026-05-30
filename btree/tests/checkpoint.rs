//! Checkpoint + overlay tests.
//!
//! A checkpoint hard-links the current file to a snapshot path and freezes the
//! database. A new writable database can then overlay the snapshot: it reads
//! through to the snapshot for untouched pages and copies pages up into its own
//! file on write, leaving the snapshot byte-for-byte unchanged.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use btree::{Db, Error};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temp directory (each test juggles several files) cleaned up on drop.
struct TmpDir(PathBuf);
impl TmpDir {
    fn new() -> TmpDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("btree_ckpt_{}_{}", std::process::id(), n));
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
fn checkpoint_freezes_the_database() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");

    let mut db = Db::open(&main).unwrap();
    fill(&mut db, 0..100, "v0");
    db.checkpoint(&snap).unwrap();

    assert!(db.is_frozen());
    // Reads still work after freezing.
    assert_eq!(db.get(b"key000050").unwrap().as_deref(), Some(&b"v0"[..]));
    // Writes are refused.
    assert!(matches!(db.insert(b"x", b"y"), Err(Error::Frozen)));
    assert!(matches!(db.delete(b"key000000"), Err(Error::Frozen)));
    assert!(matches!(db.sync(), Err(Error::Frozen)));
}

#[test]
fn overlay_reads_base_and_leaves_snapshot_untouched() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");
    let work = tmp.path("work.db");

    // Build a tree with several pages, then snapshot it.
    {
        let mut db = Db::open(&main).unwrap();
        fill(&mut db, 0..2000, "base");
        db.checkpoint(&snap).unwrap();
    }
    let snap_before = std::fs::read(&snap).unwrap();

    // Overlay the snapshot and mutate heavily.
    {
        let mut ov = Db::open_overlay(&work, &snap).unwrap();
        // Read-through: every base key is visible before we touch anything.
        for i in (0..2000).step_by(53) {
            assert_eq!(
                ov.get(format!("key{i:06}").as_bytes()).unwrap().as_deref(),
                Some(&b"base"[..]),
                "read-through failed for {i}"
            );
        }
        // Overwrite the first half, delete a slice, and add new keys.
        fill(&mut ov, 0..1000, "edited");
        for i in 1500..1600 {
            assert!(ov.delete(format!("key{i:06}").as_bytes()).unwrap());
        }
        fill(&mut ov, 2000..2500, "fresh");

        // Overlay reflects all changes layered over the base.
        assert_eq!(
            ov.get(b"key000123").unwrap().as_deref(),
            Some(&b"edited"[..])
        );
        assert_eq!(ov.get(b"key001234").unwrap().as_deref(), Some(&b"base"[..]));
        assert_eq!(
            ov.get(b"key002345").unwrap().as_deref(),
            Some(&b"fresh"[..])
        );
        assert_eq!(ov.get(b"key001550").unwrap(), None);
    }

    // The snapshot file must be byte-for-byte identical: the overlay never wrote it.
    let snap_after = std::fs::read(&snap).unwrap();
    assert_eq!(snap_before, snap_after, "overlay mutated the snapshot");

    // A fresh overlay over the snapshot still sees the pristine base data.
    let check = Db::open_overlay(tmp.path("check.db"), &snap).unwrap();
    assert_eq!(
        check.get(b"key000123").unwrap().as_deref(),
        Some(&b"base"[..])
    );
    assert_eq!(
        check.get(b"key001550").unwrap().as_deref(),
        Some(&b"base"[..])
    );
    assert_eq!(check.scan(None, None).unwrap().len(), 2000);
}

#[test]
fn overlay_persists_across_reopen() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");
    let work = tmp.path("work.db");

    {
        let mut db = Db::open(&main).unwrap();
        fill(&mut db, 0..1000, "base");
        db.checkpoint(&snap).unwrap();
    }
    // Create overlay, mutate, then close it (flushes copied pages + bitmap).
    {
        let mut ov = Db::open_overlay(&work, &snap).unwrap();
        fill(&mut ov, 0..200, "edited");
        fill(&mut ov, 1000..1100, "fresh");
    }
    // Reopen the overlay by path; it must rediscover its base and copied pages.
    let ov = Db::open(&work).unwrap();
    assert_eq!(
        ov.get(b"key000050").unwrap().as_deref(),
        Some(&b"edited"[..])
    ); // copied up
    assert_eq!(ov.get(b"key000500").unwrap().as_deref(), Some(&b"base"[..])); // read-through
    assert_eq!(
        ov.get(b"key001050").unwrap().as_deref(),
        Some(&b"fresh"[..])
    ); // native
    assert_eq!(ov.scan(None, None).unwrap().len(), 1100);
}

#[test]
fn chained_snapshots_resolve_through_the_base_chain() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let s1 = tmp.path("s1.db");
    let s2 = tmp.path("s2.db");
    let w1 = tmp.path("w1.db");
    let w2 = tmp.path("w2.db");

    // Layer 0: everything "v0", snapshot to s1.
    {
        let mut db = Db::open(&main).unwrap();
        fill(&mut db, 0..300, "v0");
        db.checkpoint(&s1).unwrap();
    }
    // Layer 1: overlay s1, rewrite the first third to "v1", snapshot to s2.
    {
        let mut o1 = Db::open_overlay(&w1, &s1).unwrap();
        fill(&mut o1, 0..100, "v1");
        o1.checkpoint(&s2).unwrap(); // syncs w1, then hard-links it to s2
    }
    // Layer 2: overlay s2 (which itself overlays s1). Resolution walks the chain.
    let o2 = Db::open_overlay(&w2, &s2).unwrap();
    assert_eq!(o2.get(b"key000050").unwrap().as_deref(), Some(&b"v1"[..])); // from layer 1
    assert_eq!(o2.get(b"key000200").unwrap().as_deref(), Some(&b"v0"[..])); // from layer 0
    assert_eq!(o2.scan(None, None).unwrap().len(), 300);

    // The original snapshot is still pure "v0".
    let base_view = Db::open_overlay(tmp.path("v.db"), &s1).unwrap();
    assert_eq!(
        base_view.get(b"key000050").unwrap().as_deref(),
        Some(&b"v0"[..])
    );
}

#[test]
fn checkpoint_refuses_to_clobber_existing_snapshot() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");

    let mut db = Db::open(&main).unwrap();
    fill(&mut db, 0..10, "v0");
    db.checkpoint(&snap).unwrap();

    // A second checkpoint to the same path must fail (hard link target exists)
    // rather than silently overwrite the snapshot.
    let mut db2 = Db::open(tmp.path("other.db")).unwrap();
    fill(&mut db2, 0..10, "v0");
    assert!(db2.checkpoint(&snap).is_err());
}

#[test]
fn deep_checkpoint_chain_resolves_every_layer() {
    // checkpoint of a checkpoint of a checkpoint... Each level owns one band of
    // keys; the deepest band exists only in the original base. Reads must walk
    // the whole chain to find the right copy. No flat list of bases is needed:
    // each file points only to its immediate parent.
    let tmp = TmpDir::new();

    // Level 0: 0..500 all "v0".
    {
        let mut db = Db::open(tmp.path("L0.db")).unwrap();
        fill(&mut db, 0..500, "v0");
        db.checkpoint(tmp.path("c0.db")).unwrap();
    }
    // Levels 1..=4: each overlays the previous checkpoint and rewrites its own
    // 100-key band, then checkpoints (except the last).
    for lvl in 1..=4u32 {
        let work = tmp.path(&format!("L{lvl}.db"));
        let base = tmp.path(&format!("c{}.db", lvl - 1));
        let mut ov = Db::open_overlay(&work, &base).unwrap();
        let band = (lvl - 1) * 100..lvl * 100;
        fill(&mut ov, band, &format!("v{lvl}"));
        if lvl < 4 {
            ov.checkpoint(tmp.path(&format!("c{lvl}.db"))).unwrap();
        }
    }

    // The top overlay (L4, a 4-deep chain) must resolve each band to the layer
    // that wrote it, and the untouched 400..500 band to the original base.
    let top = Db::open(tmp.path("L4.db")).unwrap();
    for lvl in 1..=4u32 {
        let i = (lvl - 1) * 100 + 50;
        assert_eq!(
            top.get(format!("key{i:06}").as_bytes()).unwrap().as_deref(),
            Some(format!("v{lvl}").as_bytes()),
            "band written at level {lvl} did not resolve"
        );
    }
    assert_eq!(top.get(b"key000450").unwrap().as_deref(), Some(&b"v0"[..]));
    assert_eq!(top.scan(None, None).unwrap().len(), 500);
}

#[test]
fn five_overlays_share_one_base_without_disturbing_it() {
    let tmp = TmpDir::new();
    let main = tmp.path("main.db");
    let snap = tmp.path("snap.db");
    const N: u32 = 1000;
    const OVERLAYS: u32 = 5;

    // Build the base image and snapshot it.
    {
        let mut db = Db::open(&main).unwrap();
        fill(&mut db, 0..N, "base");
        db.checkpoint(&snap).unwrap();
    }
    let snap_before = std::fs::read(&snap).unwrap();

    // Spin up five independent overlays over the same snapshot, all live at once.
    let mut overlays: Vec<Db> = (0..OVERLAYS)
        .map(|j| Db::open_overlay(tmp.path(&format!("ov{j}.db")), &snap).unwrap())
        .collect();

    // Each overlay rewrites every key, deletes its own slice, and adds its own
    // fresh keys. Interleave the work so all overlays are mutating concurrently.
    for round in 0..N {
        for (j, ov) in overlays.iter_mut().enumerate() {
            let j = j as u32;
            // Overwrite key `round` with a value unique to this overlay.
            ov.insert(
                format!("key{round:06}").as_bytes(),
                format!("ov{j}").as_bytes(),
            )
            .unwrap();
        }
    }
    for (j, ov) in overlays.iter_mut().enumerate() {
        let j = j as u32;
        // Delete a slice unique to this overlay.
        for i in (j * 100)..(j * 100 + 50) {
            assert!(ov.delete(format!("key{i:06}").as_bytes()).unwrap());
        }
        // Insert fresh keys unique to this overlay.
        for i in N..(N + 100) {
            ov.insert(
                format!("key{i:06}").as_bytes(),
                format!("new{j}").as_bytes(),
            )
            .unwrap();
        }
    }

    // Every overlay sees only its own edits, never a sibling's.
    for (j, ov) in overlays.iter().enumerate() {
        let j = j as u32;
        let want = format!("ov{j}");
        // A rewritten key reflects this overlay's value.
        assert_eq!(
            ov.get(b"key000777").unwrap().as_deref(),
            Some(want.as_bytes()),
            "overlay {j} lost its own write"
        );
        // This overlay's deleted slice is gone here...
        assert_eq!(
            ov.get(format!("key{:06}", j * 100).as_bytes()).unwrap(),
            None
        );
        // ...but a sibling's deleted slice is still present here (as our rewrite).
        let sibling = (j + 1) % OVERLAYS;
        assert_eq!(
            ov.get(format!("key{:06}", sibling * 100).as_bytes())
                .unwrap()
                .as_deref(),
            Some(want.as_bytes()),
            "overlay {j} saw sibling {sibling}'s delete"
        );
        // This overlay's fresh key carries its own marker.
        assert_eq!(
            ov.get(format!("key{:06}", N).as_bytes())
                .unwrap()
                .as_deref(),
            Some(format!("new{j}").as_bytes())
        );
        // Count: N rewritten - 50 deleted + 100 fresh.
        assert_eq!(ov.scan(None, None).unwrap().len(), (N - 50 + 100) as usize);
    }

    // Flush and close all overlays.
    drop(overlays);

    // The shared base image is byte-for-byte unchanged.
    let snap_after = std::fs::read(&snap).unwrap();
    assert_eq!(
        snap_before, snap_after,
        "base snapshot changed after five overlays mutated it"
    );

    // And it still reads back as the original, pristine data.
    let check = Db::open_overlay(tmp.path("check.db"), &snap).unwrap();
    assert_eq!(check.scan(None, None).unwrap().len(), N as usize);
    for i in (0..N).step_by(37) {
        assert_eq!(
            check
                .get(format!("key{i:06}").as_bytes())
                .unwrap()
                .as_deref(),
            Some(&b"base"[..]),
            "base key {i} was disturbed"
        );
    }
}
