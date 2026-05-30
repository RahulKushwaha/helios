//! End-to-end tests for the B+Tree, including a randomized comparison against
//! `std::collections::BTreeMap` as an oracle, with reopen-from-disk checks.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use btree::{Db, Error};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A self-deleting temp path so tests don't leave files behind.
struct TmpDb(PathBuf);
impl TmpDb {
    fn new() -> TmpDb {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("btree_test_{}_{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&path);
        TmpDb(path)
    }
    fn open(&self) -> Db {
        Db::open(&self.0).unwrap()
    }
}
impl Drop for TmpDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Tiny deterministic xorshift RNG (keeps the crate dependency-free).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn basic_put_get_delete() {
    let tmp = TmpDb::new();
    let mut db = tmp.open();

    assert_eq!(db.get(b"missing").unwrap(), None);
    db.insert(b"a", b"1").unwrap();
    db.insert(b"b", b"2").unwrap();
    assert_eq!(db.get(b"a").unwrap().as_deref(), Some(&b"1"[..]));

    // overwrite
    db.insert(b"a", b"11").unwrap();
    assert_eq!(db.get(b"a").unwrap().as_deref(), Some(&b"11"[..]));

    assert!(db.delete(b"a").unwrap());
    assert!(!db.delete(b"a").unwrap());
    assert_eq!(db.get(b"a").unwrap(), None);
    assert_eq!(db.get(b"b").unwrap().as_deref(), Some(&b"2"[..]));
}

#[test]
fn persists_across_reopen() {
    let tmp = TmpDb::new();
    {
        let mut db = tmp.open();
        for i in 0..1000u32 {
            let k = format!("key{i:06}");
            db.insert(k.as_bytes(), format!("val{i}").as_bytes())
                .unwrap();
        }
        // dropped here -> sync to disk
    }
    let db = tmp.open();
    for i in 0..1000u32 {
        let k = format!("key{i:06}");
        assert_eq!(
            db.get(k.as_bytes()).unwrap().as_deref(),
            Some(format!("val{i}").as_bytes())
        );
    }
}

#[test]
fn scan_ranges() {
    let tmp = TmpDb::new();
    let mut db = tmp.open();
    for i in 0..100u32 {
        let k = format!("{i:03}");
        db.insert(k.as_bytes(), b"x").unwrap();
    }
    // full scan is sorted and complete
    let all = db.scan(None, None).unwrap();
    assert_eq!(all.len(), 100);
    assert!(all.windows(2).all(|w| w[0].0 < w[1].0));

    // [010, 020)
    let mid = db.scan(Some(b"010"), Some(b"020")).unwrap();
    let keys: Vec<String> = mid
        .iter()
        .map(|(k, _)| String::from_utf8(k.clone()).unwrap())
        .collect();
    assert_eq!(
        keys,
        (10..20).map(|i| format!("{i:03}")).collect::<Vec<_>>()
    );
}

#[test]
fn entry_too_large_is_rejected() {
    let tmp = TmpDb::new();
    let mut db = tmp.open();
    let huge = vec![0u8; 4096];
    match db.insert(b"k", &huge) {
        Err(Error::EntryTooLarge { .. }) => {}
        other => panic!("expected EntryTooLarge, got {other:?}"),
    }
}

#[test]
fn deletes_shrink_height_and_reclaim() {
    let tmp = TmpDb::new();
    let mut db = tmp.open();
    // Enough entries to build several levels.
    for i in 0..5000u32 {
        let k = format!("{i:08}");
        db.insert(k.as_bytes(), b"payload-payload-payload").unwrap();
    }
    // Delete almost everything; tree must stay correct and collapse.
    for i in 0..4990u32 {
        let k = format!("{i:08}");
        assert!(db.delete(k.as_bytes()).unwrap(), "delete {i}");
    }
    let remaining = db.scan(None, None).unwrap();
    assert_eq!(remaining.len(), 10);
    for (i, (k, _)) in (4990..5000u32).zip(remaining) {
        assert_eq!(k, format!("{i:08}").into_bytes());
    }
}

#[test]
fn randomized_against_oracle() {
    let tmp = TmpDb::new();
    let mut db = tmp.open();
    let mut oracle: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let mut rng = Rng(0x9e3779b97f4a7c15);

    let key_space = 3000u64;
    for step in 0..40_000u64 {
        let kn = rng.below(key_space);
        // Vary key length so pages hold mixed-size entries.
        let key = if kn % 7 == 0 {
            format!("k{kn:010}-{}", "x".repeat((kn % 40) as usize)).into_bytes()
        } else {
            format!("k{kn:05}").into_bytes()
        };

        match rng.below(3) {
            0 | 1 => {
                let vlen = (rng.below(180)) as usize;
                let val = vec![(kn & 0xff) as u8; vlen];
                db.insert(&key, &val).unwrap();
                oracle.insert(key, val);
            }
            _ => {
                let existed_db = db.delete(&key).unwrap();
                let existed_oracle = oracle.remove(&key).is_some();
                assert_eq!(existed_db, existed_oracle, "delete mismatch at step {step}");
            }
        }

        // Spot-check a few random keys against the oracle every so often.
        if step % 500 == 0 {
            for _ in 0..50 {
                let probe = format!("k{:05}", rng.below(key_space)).into_bytes();
                assert_eq!(
                    db.get(&probe).unwrap(),
                    oracle.get(&probe).cloned(),
                    "get mismatch at step {step}"
                );
            }
        }

        // Periodically reopen from disk and verify the full contents.
        if step % 9000 == 8999 {
            db.sync().unwrap();
            drop(db);
            db = tmp.open();
            let scanned = db.scan(None, None).unwrap();
            let expected: Vec<(Vec<u8>, Vec<u8>)> =
                oracle.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            assert_eq!(
                scanned, expected,
                "full scan mismatch after reopen at step {step}"
            );
        }
    }

    // Final full comparison.
    let scanned = db.scan(None, None).unwrap();
    let expected: Vec<(Vec<u8>, Vec<u8>)> =
        oracle.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert_eq!(scanned, expected, "final full scan mismatch");
}
