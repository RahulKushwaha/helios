//! Tests for `full_snapshot`, which writes a standalone copy of the tree with
//! no base file, collapsing any overlay chain.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use btree::Db;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TmpDir(PathBuf);
impl TmpDir {
    fn new() -> TmpDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("btree_full_{}_{}", std::process::id(), n));
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
fn full_snapshot_of_a_deep_chain_stands_alone_after_deleting_every_base() {
    let chain = TmpDir::new();
    let out = TmpDir::new();
    let flat = out.path("flat.db");

    // Build a 4-deep overlay chain, each level owning a 100-key band; 400..500
    // exists only in the original base.
    {
        let mut db = Db::open(chain.path("L0.db")).unwrap();
        fill(&mut db, 0..500, "v0");
        db.checkpoint(chain.path("c0.db")).unwrap();
    }
    for lvl in 1..=4u32 {
        let mut ov = Db::open_overlay(
            chain.path(&format!("L{lvl}.db")),
            chain.path(&format!("c{}.db", lvl - 1)),
        )
        .unwrap();
        fill(&mut ov, (lvl - 1) * 100..lvl * 100, &format!("v{lvl}"));
        if lvl < 4 {
            ov.checkpoint(chain.path(&format!("c{lvl}.db"))).unwrap();
        }
    }

    // Snapshot the top of the chain into a standalone file.
    {
        let top = Db::open(chain.path("L4.db")).unwrap();
        top.full_snapshot(&flat).unwrap();
    }

    // Nuke the entire chain: every base, checkpoint, and overlay file is gone.
    std::fs::remove_dir_all(&chain.0).unwrap();

    // The flattened snapshot still opens and resolves every band on its own.
    let db = Db::open(&flat).unwrap();
    for lvl in 1..=4u32 {
        let i = (lvl - 1) * 100 + 50;
        assert_eq!(
            db.get(format!("key{i:06}").as_bytes()).unwrap().as_deref(),
            Some(format!("v{lvl}").as_bytes()),
            "band {lvl} missing from standalone snapshot"
        );
    }
    assert_eq!(db.get(b"key000450").unwrap().as_deref(), Some(&b"v0"[..]));
    assert_eq!(db.scan(None, None).unwrap().len(), 500);
}

#[test]
fn full_snapshot_does_not_freeze_or_alter_the_source() {
    let tmp = TmpDir::new();
    let snap = tmp.path("snap.db");

    let mut db = Db::open(tmp.path("main.db")).unwrap();
    fill(&mut db, 0..300, "v0");
    db.full_snapshot(&snap).unwrap();

    // Source is still writable (not frozen) and keeps evolving.
    assert!(!db.is_frozen());
    fill(&mut db, 300..400, "v1");
    assert_eq!(db.get(b"key000350").unwrap().as_deref(), Some(&b"v1"[..]));
    drop(db);

    // The snapshot froze the point-in-time state: only 0..300 existed then.
    let snap_db = Db::open(&snap).unwrap();
    assert_eq!(snap_db.scan(None, None).unwrap().len(), 300);
    assert_eq!(snap_db.get(b"key000350").unwrap(), None);
}

#[test]
fn full_snapshot_is_a_real_writable_standalone_db() {
    let tmp = TmpDir::new();
    let snap = tmp.path("snap.db");

    // Snapshot an overlay so the source had a base, then prove the copy is a
    // normal, writable, base-free database.
    {
        let mut base = Db::open(tmp.path("base.db")).unwrap();
        fill(&mut base, 0..200, "base");
        base.checkpoint(tmp.path("c.db")).unwrap();
        let mut ov = Db::open_overlay(tmp.path("ov.db"), tmp.path("c.db")).unwrap();
        fill(&mut ov, 0..50, "edited");
        ov.full_snapshot(&snap).unwrap();
    }

    let mut db = Db::open(&snap).unwrap();
    assert_eq!(
        db.get(b"key000010").unwrap().as_deref(),
        Some(&b"edited"[..])
    );
    assert_eq!(db.get(b"key000100").unwrap().as_deref(), Some(&b"base"[..]));
    // Fully writable: insert, overwrite, delete all work.
    fill(&mut db, 200..250, "new");
    db.insert(b"key000010", b"again").unwrap();
    assert!(db.delete(b"key000100").unwrap());
    assert_eq!(
        db.get(b"key000010").unwrap().as_deref(),
        Some(&b"again"[..])
    );
    assert_eq!(db.get(b"key000100").unwrap(), None);
    assert_eq!(db.scan(None, None).unwrap().len(), 200 - 1 + 50);
}

#[test]
fn snapshot_holds_exactly_the_records_every_overlay_contributed() {
    let chain = TmpDir::new();
    let out = TmpDir::new();
    let flat = out.path("flat.db");

    // `oracle` records every key/value that should exist after all overlays.
    // Every write/delete is mirrored into it so we know the exact expected set.
    let mut oracle: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let key = |i: u32| format!("key{i:08}").into_bytes();

    let put = |db: &mut Db, oracle: &mut BTreeMap<Vec<u8>, Vec<u8>>, k: Vec<u8>, v: Vec<u8>| {
        db.insert(&k, &v).unwrap();
        oracle.insert(k, v);
    };
    let del = |db: &mut Db, oracle: &mut BTreeMap<Vec<u8>, Vec<u8>>, k: &[u8]| {
        assert_eq!(db.delete(k).unwrap(), oracle.remove(k).is_some());
    };

    // Layer 0: the base, keys 0..200.
    {
        let mut db = Db::open(chain.path("L0.db")).unwrap();
        for i in 0..200u32 {
            put(&mut db, &mut oracle, key(i), b"base".to_vec());
        }
        db.checkpoint(chain.path("c0.db")).unwrap();
    }

    // Layers 1..=4, each contributing a disjoint, recorded set of changes:
    //   - 50 brand-new keys in its own band (1000*lvl ..)
    //   - overwrites 20 base keys to "edit{lvl}"
    //   - deletes 10 base keys
    for lvl in 1..=4u32 {
        let mut ov = Db::open_overlay(
            chain.path(&format!("L{lvl}.db")),
            chain.path(&format!("c{}.db", lvl - 1)),
        )
        .unwrap();

        for i in 0..50u32 {
            put(
                &mut ov,
                &mut oracle,
                key(1000 * lvl + i),
                format!("add{lvl}").into_bytes(),
            );
        }
        for i in (lvl - 1) * 20..(lvl - 1) * 20 + 20 {
            put(
                &mut ov,
                &mut oracle,
                key(i),
                format!("edit{lvl}").into_bytes(),
            );
        }
        for i in 100 + (lvl - 1) * 10..100 + (lvl - 1) * 10 + 10 {
            del(&mut ov, &mut oracle, &key(i));
        }

        if lvl < 4 {
            ov.checkpoint(chain.path(&format!("c{lvl}.db"))).unwrap();
        }
    }

    // Snapshot the top of the chain into a standalone file.
    {
        let top = Db::open(chain.path("L4.db")).unwrap();
        top.full_snapshot(&flat).unwrap();
    }

    // Delete every base file: the snapshot must need none of them.
    std::fs::remove_dir_all(&chain.0).unwrap();

    // The standalone snapshot holds exactly the recorded records, no more, no less.
    let db = Db::open(&flat).unwrap();
    let got = db.scan(None, None).unwrap();
    let expected: Vec<(Vec<u8>, Vec<u8>)> =
        oracle.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert_eq!(
        got, expected,
        "snapshot contents differ from the recorded oracle"
    );

    // Sanity on the size: 200 base - 40 deleted + 200 added = 360.
    assert_eq!(got.len(), 360);
    assert_eq!(got.len(), oracle.len());
}
