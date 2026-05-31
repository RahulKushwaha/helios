//! A page-based B+Tree backed by a single file.
//!
//! - **Page based.** Fixed 4 KiB pages in one file. See [`page`].
//! - **Bytes -> Bytes.** Variable-length keys and values, stored sorted.
//! - **Persistent.** Everything lives in an external file. See [`pager`].
//! - **No WAL.** Mutations buffer in memory and flush on [`Db::sync`]/drop.
//!   A crash mid-sync can corrupt the file; that is the accepted trade.
//!
//! Design choices:
//! - Insert uses **iterative descent**: `find_leaf` collects the path of
//!   internal page ids from root to the target leaf, then `propagate_split`
//!   walks that path bottom-up inserting separator keys into parents. No
//!   recursion; splits never re-read pages they already traversed.
//! - Delete uses **recursive descent** because rebalancing (borrow / merge)
//!   needs the parent node in hand to update separators after touching a child.
//!   Tree height is small in practice, so the stack depth is trivial.
//! - Every page op is decode -> mutate an owned `Vec` -> re-encode. No
//!   in-place slot editing, no fragmentation, no compaction.
//! - A single entry must fit in a quarter page; oversized entries return
//!   [`Error::EntryTooLarge`]. No overflow pages.
//! - Leaves are singly linked for ordered scans.
//! - Deletes rebalance with the textbook borrow-from-sibling / merge dance,
//!   leaving a node under-packed only when no size-safe option exists.
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

/// Maximum byte budget for one entry (key + value + per-entry overhead).
/// Capping at PAGE_SIZE/4 guarantees a midpoint split always yields two
/// sub-page halves, so overflow pages are never needed.
const MAX_ENTRY: usize = PAGE_SIZE / 4;

// ─── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// Key + value is too big to store inline. `limit` is the combined budget.
    EntryTooLarge {
        key: usize,
        value: usize,
        limit: usize,
    },
    /// Key alone exceeds [`page::MAX_KEY`].
    KeyTooLarge { key: usize, limit: usize },
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

// ─── Db ──────────────────────────────────────────────────────────────────────

/// A B+Tree backed by a single file.
pub struct Db {
    pager: Pager,
    /// Set after [`Db::checkpoint`]: the file is now shared with a frozen
    /// snapshot, so further writes (which would corrupt it) are refused.
    frozen: bool,
}

impl Db {
    /// Open the tree at `path`, creating an empty one if the file does not
    /// exist. If `path` is an overlay file its base is opened automatically.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Db> {
        Ok(Db {
            pager: Pager::open(path.as_ref())?,
            frozen: false,
        })
    }

    /// Create a new writable database at `top` that overlays the read-only
    /// base at `base`. Reads fall through to the base; writes copy pages up
    /// into `top`, leaving the base untouched.
    pub fn open_overlay<P: AsRef<Path>, Q: AsRef<Path>>(top: P, base: Q) -> Result<Db> {
        Ok(Db {
            pager: Pager::create_overlay(top.as_ref(), base.as_ref())?,
            frozen: false,
        })
    }

    /// Hard-link this database's file to `snapshot` (O(1)) and freeze this
    /// handle. Reads still work; any mutation returns [`Error::Frozen`].
    pub fn checkpoint<P: AsRef<Path>>(&mut self, snapshot: P) -> Result<()> {
        self.pager.sync()?;
        std::fs::hard_link(self.pager.path(), snapshot.as_ref())?;
        self.frozen = true;
        Ok(())
    }

    /// Write a complete standalone copy of this database to `snapshot`,
    /// collapsing any overlay chain. Does not freeze this handle.
    pub fn full_snapshot<P: AsRef<Path>>(&self, snapshot: P) -> Result<()> {
        self.pager.full_snapshot(snapshot.as_ref())?;
        Ok(())
    }

    /// Whether this database has been frozen by [`Db::checkpoint`].
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    // ── reads ─────────────────────────────────────────────────────────────────

    /// Look up `key`, returning its value if present.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let (leaf_id, _) = self.find_leaf(key)?;
        let leaf = self.pager.read_leaf(leaf_id)?;
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

    /// Collect all entries with `lower <= key < upper` in key order.
    /// `None` bounds are unbounded. Materialises the range into a `Vec`.
    pub fn scan(
        &self,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut leaf_id = match lower {
            Some(k) => self.find_leaf(k)?.0,
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

    // ── writes ────────────────────────────────────────────────────────────────

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

        // Descend iteratively, collecting the path of internal page ids.
        let (leaf_id, path) = self.find_leaf(key)?;
        let mut leaf = self.pager.read_leaf(leaf_id)?;

        // Insert or update in the leaf.
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
            self.pager.write_leaf(leaf_id, &leaf);
            self.pager.write_leaf(new_id, &right);
            // Walk the recorded path bottom-up, inserting separators.
            self.propagate_split(path, sep, new_id)?;
        } else {
            self.pager.write_leaf(leaf_id, &leaf);
        }
        Ok(())
    }

    /// Delete `key`. Returns whether it was present.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        let existed = self.delete_rec(self.pager.meta.root, key)?;
        // Collapse empty internal root nodes to shrink the tree's height.
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

    /// Flush dirty pages to disk, then sync the meta page. No-op on a frozen db.
    pub fn sync(&mut self) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        self.pager.sync()?;
        Ok(())
    }

    /// Force-write every resident page (not just dirty ones) and fsync.
    pub fn flush(&mut self) -> Result<()> {
        if self.frozen {
            return Err(Error::Frozen);
        }
        self.pager.flush()?;
        Ok(())
    }

    // ── iterative traversal (simpledb-style) ──────────────────────────────────

    /// Descend from the root to the leaf that should contain `key`.
    ///
    /// Returns `(leaf_page_id, path)` where `path` is the sequence of internal
    /// page ids visited, from root down to the leaf's direct parent. The path
    /// is consumed by `propagate_split` to insert split separators bottom-up
    /// without any additional page reads.
    fn find_leaf(&self, key: &[u8]) -> io::Result<(PageId, Vec<PageId>)> {
        let mut path = Vec::new();
        let mut current = self.pager.meta.root;
        loop {
            match self.pager.read_node(current)? {
                Node::Leaf(_) => return Ok((current, path)),
                Node::Internal(n) => {
                    path.push(current);
                    current = n.child_at(n.child_index(key));
                }
            }
        }
    }

    /// Return the page id of the leftmost (minimum-key) leaf.
    fn leftmost_leaf(&self) -> io::Result<PageId> {
        let mut current = self.pager.meta.root;
        loop {
            match self.pager.read_node(current)? {
                Node::Leaf(_) => return Ok(current),
                Node::Internal(n) => current = n.leftmost,
            }
        }
    }

    /// Walk `path` in reverse (leaf's parent first, root last), inserting
    /// `(sep, right_child)` into each internal node. If a node overflows it
    /// splits in turn, producing a new separator to push further up.
    ///
    /// When the path is exhausted and a split still has no home, a new root
    /// internal node is allocated and the tree grows one level.
    fn propagate_split(
        &mut self,
        path: Vec<PageId>,
        mut sep: Vec<u8>,
        mut right_child: PageId,
    ) -> io::Result<()> {
        for parent_id in path.into_iter().rev() {
            let mut node = self.pager.read_internal(parent_id)?;
            let pos = node
                .entries
                .partition_point(|(k, _)| k.as_slice() < sep.as_slice());
            node.entries.insert(pos, (sep, right_child));

            if node.is_overfull() {
                let new_id = self.pager.alloc()?;
                let (right_node, up) = split_internal(&mut node);
                self.pager.write_internal(parent_id, &node);
                self.pager.write_internal(new_id, &right_node);
                sep = up;
                right_child = new_id;
            } else {
                self.pager.write_internal(parent_id, &node);
                return Ok(());
            }
        }

        // The split bubbled all the way up: grow the tree by one level.
        let new_root_id = self.pager.alloc()?;
        let old_root = self.pager.meta.root;
        self.pager.write_internal(
            new_root_id,
            &Internal {
                leftmost: old_root,
                entries: vec![(sep, right_child)],
            },
        );
        self.pager.meta.root = new_root_id;
        Ok(())
    }

    // ── recursive delete + rebalancing ───────────────────────────────────────
    //
    // Delete uses recursive descent because rebalancing needs the parent node
    // in hand to update separator keys after borrowing from or merging with a
    // sibling. The recursion depth equals the tree height, which is O(log n)
    // and tiny in practice.

    fn delete_rec(&mut self, id: PageId, key: &[u8]) -> io::Result<bool> {
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

    /// Fix an under-full child at index `ci` by borrowing from or merging with
    /// a sibling. Leaves the child under-full when no size-safe option exists.
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
            if left.entries.len() >= 2
                && left.size() - Leaf::entry_cost(&lk, &lv) >= page::MIN_FILL
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

        // Merge with left sibling.
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

        // Merge with right sibling.
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

        // Borrow from left: rotate separator down, left's last entry up.
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

        // Borrow from right: rotate separator down, right's first entry up.
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

// ─── split helpers ────────────────────────────────────────────────────────────

/// Split an over-full leaf at its byte midpoint. `leaf` keeps the left half;
/// returns the new right leaf and the separator (its first key). The leaf
/// sibling chain is threaded through `new_id`.
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

/// Split an over-full internal node at its byte midpoint. The middle key is
/// pushed up (not copied); its child becomes the new right node's leftmost child.
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
