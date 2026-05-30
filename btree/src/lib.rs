//! The simplest page-based B+Tree I could write and still call complete.
//!
//! - **Page based.** Fixed 4 KiB pages in one file. See [`page`].
//! - **Bytes -> Bytes.** Variable-length keys and values, stored sorted.
//! - **Persistent.** Everything lives in an external file. See [`pager`].
//! - **No WAL.** Mutations buffer in memory and flush on [`Db::sync`]/drop.
//!   A crash mid-sync can corrupt the file; that is the accepted trade.
//!
//! Design choices that keep it small:
//! - Every page op is decode -> mutate an owned `Vec` -> re-encode. No in-place
//!   slot editing, no fragmentation, no compaction.
//! - A single entry (key + value) must fit in a quarter page; oversized entries
//!   return [`Error::EntryTooLarge`]. No overflow pages.
//! - Leaves are singly linked for ordered scans.
//! - Deletes rebalance with the textbook borrow-from-sibling / merge dance, but
//!   when large entries make neither size-safe we simply leave a node
//!   under-packed (still a valid tree, just looser).
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

mod page;
mod pager;

use std::fmt;
use std::io;
use std::path::Path;

use page::{Internal, Leaf, Node, PageId, HEADER, MAX_KEY, PAGE_SIZE};
use pager::Pager;

/// Largest accepted entry cost (key + value + per-entry overhead). Capping at a
/// quarter page guarantees a midpoint split always yields two sub-page halves,
/// so we never need overflow pages.
const MAX_ENTRY: usize = PAGE_SIZE / 4;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// Key + value is too big to store inline. `limit` is the byte budget for
    /// key + value combined.
    EntryTooLarge {
        key: usize,
        value: usize,
        limit: usize,
    },
    /// Key alone exceeds [`page::MAX_KEY`].
    KeyTooLarge {
        key: usize,
        limit: usize,
    },
    /// A mutation was attempted on a database frozen by [`Db::checkpoint`].
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

/// A B+Tree backed by a single file.
pub struct Db {
    pager: Pager,
    /// Set after [`Db::checkpoint`]: the file is now shared with a frozen
    /// snapshot, so further writes (which would corrupt it) are refused.
    frozen: bool,
}

impl Db {
    /// Open the tree at `path`, creating an empty one if the file does not exist.
    /// If `path` is an overlay, its base is opened and read through automatically.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Db> {
        Ok(Db {
            pager: Pager::open(path.as_ref())?,
            frozen: false,
        })
    }

    /// Create a new writable database at `top` that overlays the read-only base
    /// at `base`. Reads fall through to the base; writes copy pages up into
    /// `top`, leaving the base untouched. This is how you open a writable clone
    /// of a [`checkpoint`](Db::checkpoint) snapshot.
    pub fn open_overlay<P: AsRef<Path>, Q: AsRef<Path>>(top: P, base: Q) -> Result<Db> {
        Ok(Db {
            pager: Pager::create_overlay(top.as_ref(), base.as_ref())?,
            frozen: false,
        })
    }

    /// Take a point-in-time snapshot by hard-linking this database's file to
    /// `snapshot`. The link is O(1) and shares the data on disk.
    ///
    /// Because the snapshot shares this file's bytes, the database is **frozen**
    /// afterwards: it can still be read, but mutations return [`Error::Frozen`]
    /// (writing in place would corrupt the snapshot). To keep writing, open an
    /// overlay over the snapshot with [`Db::open_overlay`].
    pub fn checkpoint<P: AsRef<Path>>(&mut self, snapshot: P) -> Result<()> {
        self.pager.sync()?;
        std::fs::hard_link(self.pager.path(), snapshot.as_ref())?;
        self.frozen = true;
        Ok(())
    }

    /// Write a complete, standalone copy of this database to `snapshot`,
    /// collapsing any overlay chain so the result depends on **no base file**.
    ///
    /// Unlike [`Db::checkpoint`] (an O(1) hard link that freezes this handle and
    /// stays layered on its base), this copies every page (O(size)) into a fresh
    /// file and leaves this database untouched and writable. The result opens as
    /// an ordinary standalone database even if every base file is deleted.
    pub fn full_snapshot<P: AsRef<Path>>(&self, snapshot: P) -> Result<()> {
        self.pager.full_snapshot(snapshot.as_ref())?;
        Ok(())
    }

    /// Whether this database has been frozen by [`Db::checkpoint`].
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Look up `key`, returning its value if present.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let leaf = self.pager.read_leaf(self.find_leaf(key)?)?;
        Ok(
            match leaf
                .entries
                .binary_search_by(|(k, _)| k.as_slice().cmp(key))
            {
                Ok(i) => Some(leaf.entries[i].1.clone()),
                Err(_) => None,
            },
        )
    }

    /// Insert or overwrite `key` with `value`.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        if key.len() > MAX_KEY {
            return Err(Error::KeyTooLarge {
                key: key.len(),
                limit: MAX_KEY,
            });
        }
        if Leaf::entry_cost(key, value) > MAX_ENTRY {
            return Err(Error::EntryTooLarge {
                key: key.len(),
                value: value.len(),
                limit: MAX_ENTRY - 6,
            });
        }
        let root = self.pager.meta.root;
        if let Some((sep, new_id)) = self.insert_rec(root, key, value)? {
            // Root split: grow a new root one level up.
            let new_root = self.pager.alloc()?;
            let node = Internal {
                leftmost: root,
                entries: vec![(sep, new_id)],
            };
            self.pager.write_internal(new_root, &node);
            self.pager.meta.root = new_root;
        }
        Ok(())
    }

    /// Delete `key`. Returns whether it was present.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        let existed = self.delete_rec(self.pager.meta.root, key)?;
        // If the root collapsed to a single child, shrink the tree's height.
        loop {
            let root = self.pager.meta.root;
            match self.pager.read_node(root)? {
                Node::Internal(n) if n.entries.is_empty() => {
                    self.pager.meta.root = n.leftmost;
                    self.pager.free(root);
                }
                _ => break,
            }
        }
        Ok(existed)
    }

    /// Collect all entries with `lower <= key < upper`, in key order. `None`
    /// bounds mean unbounded. Materializes the range into a `Vec`.
    pub fn scan(
        &self,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut leaf_id = match lower {
            Some(k) => self.find_leaf(k)?,
            None => self.leftmost_leaf()?,
        };
        let mut out = Vec::new();
        loop {
            let leaf = self.pager.read_leaf(leaf_id)?;
            for (k, v) in &leaf.entries {
                if let Some(lo) = lower {
                    if k.as_slice() < lo {
                        continue;
                    }
                }
                if let Some(hi) = upper {
                    if k.as_slice() >= hi {
                        return Ok(out);
                    }
                }
                out.push((k.clone(), v.clone()));
            }
            if leaf.next == 0 {
                return Ok(out);
            }
            leaf_id = leaf.next;
        }
    }

    /// Flush all buffered changes to disk. A no-op on a frozen database.
    pub fn sync(&mut self) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        self.pager.sync()?;
        Ok(())
    }

    /// Force-write every page that lives in this file to disk (not just the
    /// dirty ones) and fsync. Where [`Db::sync`] persists only what changed,
    /// `flush` rewrites all resident pages. For an overlay, base pages that have
    /// not been copied up stay in the base.
    pub fn flush(&mut self) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        self.pager.flush()?;
        Ok(())
    }

    // --- traversal helpers -------------------------------------------------

    fn find_leaf(&self, key: &[u8]) -> io::Result<PageId> {
        let mut id = self.pager.meta.root;
        loop {
            match self.pager.read_node(id)? {
                Node::Leaf(_) => return Ok(id),
                Node::Internal(n) => id = n.child_at(n.child_index(key)),
            }
        }
    }

    fn leftmost_leaf(&self) -> io::Result<PageId> {
        let mut id = self.pager.meta.root;
        loop {
            match self.pager.read_node(id)? {
                Node::Leaf(_) => return Ok(id),
                Node::Internal(n) => id = n.leftmost,
            }
        }
    }

    // --- insert ------------------------------------------------------------

    /// Returns `Some((separator, new_right_page))` if this subtree split.
    fn insert_rec(
        &mut self,
        id: PageId,
        key: &[u8],
        value: &[u8],
    ) -> Result<Option<(Vec<u8>, PageId)>> {
        match self.pager.read_node(id)? {
            Node::Leaf(mut leaf) => {
                match leaf
                    .entries
                    .binary_search_by(|(k, _)| k.as_slice().cmp(key))
                {
                    Ok(i) => leaf.entries[i].1 = value.to_vec(),
                    Err(i) => leaf.entries.insert(i, (key.to_vec(), value.to_vec())),
                }
                if leaf.is_overfull() {
                    let new_id = self.pager.alloc()?;
                    let (right, sep) = split_leaf(&mut leaf, new_id);
                    self.pager.write_leaf(id, &leaf);
                    self.pager.write_leaf(new_id, &right);
                    Ok(Some((sep, new_id)))
                } else {
                    self.pager.write_leaf(id, &leaf);
                    Ok(None)
                }
            }
            Node::Internal(mut node) => {
                let child = node.child_at(node.child_index(key));
                if let Some((sep, new_child)) = self.insert_rec(child, key, value)? {
                    let pos = node
                        .entries
                        .partition_point(|(k, _)| k.as_slice() < sep.as_slice());
                    node.entries.insert(pos, (sep, new_child));
                    if node.is_overfull() {
                        let new_id = self.pager.alloc()?;
                        let (right, up) = split_internal(&mut node);
                        self.pager.write_internal(id, &node);
                        self.pager.write_internal(new_id, &right);
                        return Ok(Some((up, new_id)));
                    }
                    self.pager.write_internal(id, &node);
                }
                Ok(None)
            }
        }
    }

    // --- delete ------------------------------------------------------------

    fn delete_rec(&mut self, id: PageId, key: &[u8]) -> Result<bool> {
        match self.pager.read_node(id)? {
            Node::Leaf(mut leaf) => {
                match leaf
                    .entries
                    .binary_search_by(|(k, _)| k.as_slice().cmp(key))
                {
                    Ok(i) => {
                        leaf.entries.remove(i);
                        self.pager.write_leaf(id, &leaf);
                        Ok(true)
                    }
                    Err(_) => Ok(false),
                }
            }
            Node::Internal(mut node) => {
                let ci = node.child_index(key);
                let child = node.child_at(ci);
                let existed = self.delete_rec(child, key)?;
                if existed && self.is_underfull(child)? {
                    self.rebalance(&mut node, ci)?;
                    self.pager.write_internal(id, &node);
                }
                Ok(existed)
            }
        }
    }

    fn is_underfull(&self, id: PageId) -> io::Result<bool> {
        Ok(match self.pager.read_node(id)? {
            Node::Leaf(l) => l.is_underfull(),
            Node::Internal(n) => n.is_underfull(),
        })
    }

    /// Fix an under-full child at index `ci` by borrowing from or merging with a
    /// sibling. May leave the child under-full if neither is size-safe.
    fn rebalance(&mut self, node: &mut Internal, ci: usize) -> io::Result<()> {
        match self.pager.read_node(node.child_at(ci))? {
            Node::Leaf(child) => self.rebalance_leaf(node, ci, child),
            Node::Internal(child) => self.rebalance_internal(node, ci, child),
        }
    }

    fn rebalance_leaf(
        &mut self,
        node: &mut Internal,
        ci: usize,
        mut child: Leaf,
    ) -> io::Result<()> {
        let n = node.entries.len();
        let child_id = node.child_at(ci);

        // Borrow from left: move its last entry to the front of child.
        if ci > 0 {
            let left_id = node.child_at(ci - 1);
            let mut left = self.pager.read_leaf(left_id)?;
            let (lk, lv) = left.entries.last().cloned().unwrap();
            if left.entries.len() >= 2 && left.size() - Leaf::entry_cost(&lk, &lv) >= page::MIN_FILL
            {
                left.entries.pop();
                child.entries.insert(0, (lk, lv));
                node.entries[ci - 1].0 = child.entries[0].0.clone();
                self.pager.write_leaf(left_id, &left);
                self.pager.write_leaf(child_id, &child);
                return Ok(());
            }
        }
        // Borrow from right: move its first entry to the end of child.
        if ci < n {
            let right_id = node.child_at(ci + 1);
            let mut right = self.pager.read_leaf(right_id)?;
            let (rk, rv) = right.entries.first().cloned().unwrap();
            if right.entries.len() >= 2
                && right.size() - Leaf::entry_cost(&rk, &rv) >= page::MIN_FILL
            {
                right.entries.remove(0);
                child.entries.push((rk, rv));
                node.entries[ci].0 = right.entries[0].0.clone();
                self.pager.write_leaf(right_id, &right);
                self.pager.write_leaf(child_id, &child);
                return Ok(());
            }
        }
        // Merge with left.
        if ci > 0 {
            let left_id = node.child_at(ci - 1);
            let mut left = self.pager.read_leaf(left_id)?;
            if left.size() + child.size() - HEADER <= PAGE_SIZE {
                left.entries.append(&mut child.entries);
                left.next = child.next;
                self.pager.write_leaf(left_id, &left);
                self.pager.free(child_id);
                node.entries.remove(ci - 1);
                return Ok(());
            }
        }
        // Merge with right.
        if ci < n {
            let right_id = node.child_at(ci + 1);
            let mut right = self.pager.read_leaf(right_id)?;
            if child.size() + right.size() - HEADER <= PAGE_SIZE {
                child.entries.append(&mut right.entries);
                child.next = right.next;
                self.pager.write_leaf(child_id, &child);
                self.pager.free(right_id);
                node.entries.remove(ci);
                return Ok(());
            }
        }
        Ok(()) // leave under-full
    }

    fn rebalance_internal(
        &mut self,
        node: &mut Internal,
        ci: usize,
        mut child: Internal,
    ) -> io::Result<()> {
        let n = node.entries.len();
        let child_id = node.child_at(ci);

        // Borrow from left: rotate the separator down, left's last key up.
        if ci > 0 {
            let left_id = node.child_at(ci - 1);
            let mut left = self.pager.read_internal(left_id)?;
            let donor = left.entries.last().unwrap().0.clone();
            let old_sep = node.entries[ci - 1].0.clone();
            let node_after = node.size() - old_sep.len() + donor.len();
            if left.entries.len() >= 2
                && left.size() - Internal::entry_cost(&donor) >= page::MIN_FILL
                && node_after <= PAGE_SIZE
            {
                let (sep_l, child_l) = left.entries.pop().unwrap();
                child.entries.insert(0, (old_sep, child.leftmost));
                child.leftmost = child_l;
                node.entries[ci - 1].0 = sep_l;
                self.pager.write_internal(left_id, &left);
                self.pager.write_internal(child_id, &child);
                return Ok(());
            }
        }
        // Borrow from right: rotate the separator down, right's first key up.
        if ci < n {
            let right_id = node.child_at(ci + 1);
            let mut right = self.pager.read_internal(right_id)?;
            let donor = right.entries.first().unwrap().0.clone();
            let old_sep = node.entries[ci].0.clone();
            let node_after = node.size() - old_sep.len() + donor.len();
            if right.entries.len() >= 2
                && right.size() - Internal::entry_cost(&donor) >= page::MIN_FILL
                && node_after <= PAGE_SIZE
            {
                let (sep_r, child_r) = right.entries.remove(0);
                child.entries.push((old_sep, right.leftmost));
                right.leftmost = child_r;
                node.entries[ci].0 = sep_r;
                self.pager.write_internal(right_id, &right);
                self.pager.write_internal(child_id, &child);
                return Ok(());
            }
        }
        // Merge with left, pulling the separator down between them.
        if ci > 0 {
            let left_id = node.child_at(ci - 1);
            let mut left = self.pager.read_internal(left_id)?;
            let sep = node.entries[ci - 1].0.clone();
            if left.size() + child.size() - HEADER + Internal::entry_cost(&sep) <= PAGE_SIZE {
                left.entries.push((sep, child.leftmost));
                left.entries.append(&mut child.entries);
                self.pager.write_internal(left_id, &left);
                self.pager.free(child_id);
                node.entries.remove(ci - 1);
                return Ok(());
            }
        }
        // Merge with right.
        if ci < n {
            let right_id = node.child_at(ci + 1);
            let mut right = self.pager.read_internal(right_id)?;
            let sep = node.entries[ci].0.clone();
            if child.size() + right.size() - HEADER + Internal::entry_cost(&sep) <= PAGE_SIZE {
                child.entries.push((sep, right.leftmost));
                child.entries.append(&mut right.entries);
                self.pager.write_internal(child_id, &child);
                self.pager.free(right_id);
                node.entries.remove(ci);
                return Ok(());
            }
        }
        Ok(()) // leave under-full
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // A frozen database shares its file with a snapshot; never write it.
        if !self.frozen {
            let _ = self.pager.sync();
        }
    }
}

// --- split helpers ---------------------------------------------------------

/// Split an over-full leaf near its byte midpoint. `leaf` keeps the left half;
/// returns the new right leaf and the separator (its first key). `new_id` is the
/// right leaf's page id, threaded into the leaf chain.
fn split_leaf(leaf: &mut Leaf, new_id: PageId) -> (Leaf, Vec<u8>) {
    let total: usize = leaf
        .entries
        .iter()
        .map(|(k, v)| Leaf::entry_cost(k, v))
        .sum();
    let mut acc = 0;
    let mut sp = leaf.entries.len();
    for i in 0..leaf.entries.len() {
        acc += Leaf::entry_cost(&leaf.entries[i].0, &leaf.entries[i].1);
        if acc * 2 >= total {
            sp = i + 1;
            break;
        }
    }
    let sp = sp.clamp(1, leaf.entries.len() - 1);
    let right_entries = leaf.entries.split_off(sp);
    let sep = right_entries[0].0.clone();
    let right = Leaf {
        next: leaf.next,
        entries: right_entries,
    };
    leaf.next = new_id;
    (right, sep)
}

/// Split an over-full internal node. The middle key moves up (not copied); its
/// child becomes the new right node's leftmost child.
fn split_internal(node: &mut Internal) -> (Internal, Vec<u8>) {
    let total: usize = node
        .entries
        .iter()
        .map(|(k, _)| Internal::entry_cost(k))
        .sum();
    let mut acc = 0;
    let mut mid = node.entries.len() / 2;
    for i in 0..node.entries.len() {
        acc += Internal::entry_cost(&node.entries[i].0);
        if acc * 2 >= total {
            mid = i;
            break;
        }
    }
    let mid = mid.clamp(1, node.entries.len() - 2);
    let right_entries = node.entries.split_off(mid + 1);
    let (up_key, right_leftmost) = node.entries.pop().unwrap();
    let right = Internal {
        leftmost: right_leftmost,
        entries: right_entries,
    };
    (right, up_key)
}
