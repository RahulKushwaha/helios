//! A B+Tree backed by a single file, using simpledb's 8KB slotted-page format.
//!
//! - **Page format.** 8KB slotted pages with a binary-search slot array.
//! - **Bytes -> Bytes.** Variable-length keys and values, stored sorted.
//! - **Persistent.** All pages live in a file managed by the pager.
//! - **No WAL.** Mutations buffer in memory and flush on [`Db::sync`]/drop.
//! - **Overlays.** A writable overlay can be layered over a frozen snapshot.
//!
//! ```
//! # use btree::Db;
//! # let path = std::env::temp_dir().join("btree_doctest.db");
//! # let _ = std::fs::remove_file(&path);
//! let mut db = Db::open(&path).unwrap();
//! db.insert(b"apple", b"red").unwrap();
//! db.insert(b"banana", b"yellow").unwrap();
//! assert_eq!(db.get(b"apple").unwrap().as_deref(), Some(&b"red"[..]));
//! assert!(db.delete(b"apple").unwrap());
//! assert_eq!(db.get(b"apple").unwrap(), None);
//! # let _ = std::fs::remove_file(&path);
//! ```

mod byte;
mod internal;
mod key;
mod leaf;
mod page;
mod pager;
mod tuple;

use std::fmt;
use std::io;
use std::path::Path;

use internal::{InternalPage, InternalPageMut, InternalSplit};
use leaf::{LeafPageMut, LeafSplit};
use page::PAGE_SIZE;
use pager::FilePager;

/// Per-entry overhead in the 8KB slotted page: slot (2) + tuple header (5).
const LEAF_ENTRY_OVERHEAD: usize = 7;
/// Maximum combined byte cost for one key+value entry.
const MAX_ENTRY: usize = PAGE_SIZE / 4;
/// Maximum key length.
const MAX_KEY: usize = 1024;

// ─── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    EntryTooLarge { key: usize, value: usize, limit: usize },
    KeyTooLarge { key: usize, limit: usize },
    Frozen,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::EntryTooLarge { key, value, limit } => write!(
                f,
                "entry too large: key {key} + value {value} bytes exceeds {limit}"
            ),
            Error::KeyTooLarge { key, limit } => {
                write!(f, "key too large: {key} bytes exceeds {limit}")
            }
            Error::Frozen => write!(f, "database is frozen (read-only after checkpoint)"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// ─── Db ──────────────────────────────────────────────────────────────────────

/// A B+Tree backed by a single file.
pub struct Db {
    pager: FilePager,
    frozen: bool,
}

impl Db {
    /// Open the tree at `path`, creating an empty one if the file does not exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Db> {
        Ok(Db { pager: FilePager::open(path.as_ref())?, frozen: false })
    }

    /// Create a writable database at `top` that overlays the read-only base at `base`.
    pub fn open_overlay<P: AsRef<Path>, Q: AsRef<Path>>(top: P, base: Q) -> Result<Db> {
        Ok(Db {
            pager: FilePager::create_overlay(top.as_ref(), base.as_ref())?,
            frozen: false,
        })
    }

    /// Hard-link this file to `snapshot` and freeze this handle.
    pub fn checkpoint<P: AsRef<Path>>(&mut self, snapshot: P) -> Result<()> {
        self.pager.sync()?;
        std::fs::hard_link(self.pager.path(), snapshot.as_ref())?;
        self.frozen = true;
        Ok(())
    }

    /// Write a complete standalone copy collapsing any overlay chain. Does not freeze.
    pub fn full_snapshot<P: AsRef<Path>>(&self, snapshot: P) -> Result<()> {
        self.pager.full_snapshot(snapshot.as_ref())?;
        Ok(())
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    // ── reads ─────────────────────────────────────────────────────────────────

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if self.height() == 0 {
            return Ok(None);
        }
        let (leaf_id, _) = self.find_leaf(key)?;
        let page = self.pager.read_page(leaf_id)?;
        Ok(page.get(key).map(|v| v.to_vec()))
    }

    pub fn scan(
        &self,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        if self.height() == 0 {
            return Ok(Vec::new());
        }
        let mut leaf_id = match lower {
            Some(k) => self.find_leaf(k)?.0,
            None => self.find_leftmost_leaf()?,
        };
        let mut out = Vec::new();
        loop {
            let page = self.pager.read_page(leaf_id)?;
            for i in 0..page.slot_count() as usize {
                if let Some((k, v)) = page.get_key_value_at_slot(i) {
                    if let Some(lo) = lower {
                        if k < lo {
                            continue;
                        }
                    }
                    if let Some(hi) = upper {
                        if k >= hi {
                            return Ok(out);
                        }
                    }
                    out.push((k.to_vec(), v.to_vec()));
                }
            }
            match page.next_leaf_page_id() {
                Some(next) => leaf_id = next,
                None => return Ok(out),
            }
        }
    }

    // ── writes ────────────────────────────────────────────────────────────────

    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        if key.len() > MAX_KEY {
            return Err(Error::KeyTooLarge { key: key.len(), limit: MAX_KEY });
        }
        let entry_cost = LEAF_ENTRY_OVERHEAD + key.len() + value.len();
        if entry_cost > MAX_ENTRY {
            return Err(Error::EntryTooLarge {
                key: key.len(),
                value: value.len(),
                limit: MAX_ENTRY - LEAF_ENTRY_OVERHEAD,
            });
        }

        if self.height() == 0 {
            let root_id = self.pager.new_page();
            self.pager.meta.root = root_id;
            self.pager.meta.height = 1;
        }

        let (leaf_id, path) = self.find_leaf(key)?;

        // Phase 1: attempt insert without pre-allocating a right-page ID.
        let needs_split = {
            let page = self.pager.page_mut(leaf_id)?;
            let mut leaf = LeafPageMut::new(page);
            match leaf.insert(key, value) {
                Ok(_) => false,
                Err("page full") => true,
                Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e).into()),
            }
        };

        if !needs_split {
            return Ok(());
        }

        // Phase 2: leaf is full — allocate now and split.
        let new_right_leaf_id = self.pager.alloc();
        let split = {
            let page = self.pager.page_mut(leaf_id)?;
            let mut leaf = LeafPageMut::new(page);
            leaf.insert_or_split(key, value, new_right_leaf_id)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                .unwrap() // guaranteed: leaf is full
        };

        self.pager.write_page(new_right_leaf_id, split.right_page.into_page());
        self.propagate_split(path, split.separator_key, new_right_leaf_id)?;
        Ok(())
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<bool> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        if self.height() == 0 {
            return Ok(false);
        }
        let (leaf_id, _) = self.find_leaf(key)?;
        let existed = {
            let page = self.pager.page_mut(leaf_id)?;
            LeafPageMut::new(page).remove(key).is_some()
        };
        Ok(existed)
    }

    pub fn sync(&mut self) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        self.pager.sync()?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        self.pager.flush()?;
        Ok(())
    }

    // ── traversal ────────────────────────────────────────────────────────────

    fn height(&self) -> u32 {
        self.pager.meta.height
    }

    /// Iterative descent to the leaf that should contain `key`.
    /// Returns `(leaf_page_id, path of internal page_ids from root to parent)`.
    fn find_leaf(&self, key: &[u8]) -> io::Result<(u32, Vec<u32>)> {
        let mut path = Vec::new();
        let mut current = self.pager.meta.root;
        for _ in 1..self.height() {
            path.push(current);
            let page = self.pager.read_page(current)?;
            let node = InternalPage::from_page(page);
            current = node.find_child(key);
        }
        Ok((current, path))
    }

    fn find_leftmost_leaf(&self) -> io::Result<u32> {
        let mut current = self.pager.meta.root;
        for _ in 1..self.height() {
            let page = self.pager.read_page(current)?;
            let node = InternalPage::from_page(page);
            current = node.leftmost_child();
        }
        Ok(current)
    }

    /// Walk `path` bottom-up, inserting split separators into parent internal nodes.
    /// When the path is exhausted a new root is created and height increments.
    fn propagate_split(
        &mut self,
        path: Vec<u32>,
        mut sep_key: Vec<u8>,
        mut right_child: u32,
    ) -> io::Result<()> {
        for parent_id in path.into_iter().rev() {
            // Phase 1: try inserting the separator without pre-allocating.
            let needs_split = {
                let page = self.pager.page_mut(parent_id)?;
                let mut node = InternalPageMut::new(page);
                match node.insert(&sep_key, right_child) {
                    Ok(_) => false,
                    Err("page full") => true,
                    Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
                }
            };

            if !needs_split {
                return Ok(());
            }

            // Phase 2: internal node full — allocate and split.
            let new_right_id = self.pager.alloc();
            let split = {
                let page = self.pager.page_mut(parent_id)?;
                let mut node = InternalPageMut::new(page);
                node.insert_or_split(&sep_key, right_child, new_right_id)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                    .unwrap() // guaranteed: node is full
            };

            self.pager.write_page(new_right_id, split.right_page.into_page());
            sep_key = split.separator_key;
            right_child = new_right_id;
        }

        // Split reached the root — grow the tree by one level.
        let new_root_id = self.pager.new_page();
        let old_root = self.pager.meta.root;
        {
            let page = self.pager.page_mut(new_root_id)?;
            let mut root = InternalPageMut::new(page);
            root.set_leftmost_child(old_root);
            root.insert(&sep_key, right_child)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        }
        self.pager.meta.root = new_root_id;
        self.pager.meta.height += 1;
        Ok(())
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        if !self.frozen {
            let _ = self.pager.sync();
        }
    }
}
