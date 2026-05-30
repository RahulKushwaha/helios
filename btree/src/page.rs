//! 4 KiB slotted page encode/decode for leaf and internal nodes.
//!
//! Every page mutation in this B+Tree follows the same pattern: decode the raw
//! page bytes into an owned `Leaf`/`Internal`, mutate the plain `Vec`, then
//! re-encode. That trades a little CPU for a lot of simplicity: there is no
//! in-place slot juggling, no fragmentation, and no compaction. It also means
//! we never hold two raw page borrows at once, so splits and merges are just
//! `Vec` operations.
//!
//! Page byte layout (both node kinds share a 16-byte header):
//!
//! ```text
//!   0      u8   page type: LEAF / INTERNAL / FREE
//!   1      u8   padding
//!   2..4   u16  number of keys / entries
//!   4..6   u16  cell_top: lowest used cell offset (informational)
//!   6..8   u16  padding
//!   8..16  u64  extra: leaf -> next-leaf page id; internal -> leftmost child
//!  16..    [u16; nkeys] slot array: each slot is the byte offset of its cell
//!   ...    cells, packed downward from the end of the page
//! ```
//!
//! Leaf cell:     u16 key_len, u16 val_len, key bytes, value bytes.
//! Internal cell: u16 key_len, key bytes, u64 right-child page id.

pub type PageId = u64;

pub const PAGE_SIZE: usize = 4096;
pub const HEADER: usize = 16;

pub const FREE: u8 = 0;
pub const INTERNAL: u8 = 1;
pub const LEAF: u8 = 2;

/// A page is at most half empty before we try to rebalance it on delete.
pub const MIN_FILL: usize = PAGE_SIZE / 2;

/// Largest key we accept. Keeps separators comfortably under a page so an
/// internal node can always hold a copied-up key.
pub const MAX_KEY: usize = PAGE_SIZE / 4;

/// A decoded leaf: sorted key/value pairs plus the next-leaf link for scans.
#[derive(Debug, Clone, Default)]
pub struct Leaf {
    pub next: PageId,
    pub entries: Vec<(Vec<u8>, Vec<u8>)>,
}

/// A decoded internal node. `leftmost` is child 0; `entries[i]` is
/// (separator key, right child). The separator is the smallest key in its
/// right child's subtree. Children: leftmost, entries[0].1, entries[1].1, ...
#[derive(Debug, Clone, Default)]
pub struct Internal {
    pub leftmost: PageId,
    pub entries: Vec<(Vec<u8>, PageId)>,
}

#[derive(Debug)]
pub enum Node {
    Leaf(Leaf),
    Internal(Internal),
}

/// Per-entry on-disk cost of a leaf pair: 2 (slot) + 2 (key_len) + 2 (val_len).
const LEAF_OVERHEAD: usize = 6;
/// Per-entry on-disk cost of an internal pair: 2 (slot) + 2 (key_len) + 8 (child).
const INTERNAL_OVERHEAD: usize = 12;

impl Leaf {
    /// Encoded size in bytes if this leaf were written to a page right now.
    pub fn size(&self) -> usize {
        HEADER
            + self
                .entries
                .iter()
                .map(|(k, v)| LEAF_OVERHEAD + k.len() + v.len())
                .sum::<usize>()
    }

    pub fn is_overfull(&self) -> bool {
        self.size() > PAGE_SIZE
    }

    pub fn is_underfull(&self) -> bool {
        self.size() < MIN_FILL
    }

    /// Byte cost of a single entry, including its slot.
    pub fn entry_cost(k: &[u8], v: &[u8]) -> usize {
        LEAF_OVERHEAD + k.len() + v.len()
    }
}

impl Internal {
    pub fn size(&self) -> usize {
        HEADER
            + self
                .entries
                .iter()
                .map(|(k, _)| INTERNAL_OVERHEAD + k.len())
                .sum::<usize>()
    }

    pub fn is_overfull(&self) -> bool {
        self.size() > PAGE_SIZE
    }

    pub fn is_underfull(&self) -> bool {
        self.size() < MIN_FILL
    }

    pub fn entry_cost(k: &[u8]) -> usize {
        INTERNAL_OVERHEAD + k.len()
    }

    /// The child page id at child index `ci` (0 == leftmost).
    pub fn child_at(&self, ci: usize) -> PageId {
        if ci == 0 {
            self.leftmost
        } else {
            self.entries[ci - 1].1
        }
    }

    /// Child index to descend into for `key`. Returns a value in `0..=len`.
    pub fn child_index(&self, key: &[u8]) -> usize {
        self.entries
            .partition_point(|(sep, _)| sep.as_slice() <= key)
    }
}

// --- little-endian helpers -------------------------------------------------

fn put_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn put_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}
fn get_u16(buf: &[u8], off: usize) -> usize {
    u16::from_le_bytes([buf[off], buf[off + 1]]) as usize
}
fn get_u64(buf: &[u8], off: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&buf[off..off + 8]);
    u64::from_le_bytes(b)
}

// --- encode / decode -------------------------------------------------------

/// Encode a leaf into a fresh page buffer. Caller must have split first if the
/// leaf was overfull; we debug-assert it fits.
pub fn encode_leaf(leaf: &Leaf) -> Box<[u8; PAGE_SIZE]> {
    debug_assert!(leaf.size() <= PAGE_SIZE, "leaf overfull at encode");
    let mut buf = Box::new([0u8; PAGE_SIZE]);
    buf[0] = LEAF;
    put_u16(&mut *buf, 2, leaf.entries.len() as u16);
    put_u64(&mut *buf, 8, leaf.next);

    let mut top = PAGE_SIZE;
    for (i, (k, v)) in leaf.entries.iter().enumerate() {
        let cell = 4 + k.len() + v.len();
        top -= cell;
        put_u16(&mut *buf, top, k.len() as u16);
        put_u16(&mut *buf, top + 2, v.len() as u16);
        buf[top + 4..top + 4 + k.len()].copy_from_slice(k);
        buf[top + 4 + k.len()..top + 4 + k.len() + v.len()].copy_from_slice(v);
        put_u16(&mut *buf, HEADER + i * 2, top as u16);
    }
    put_u16(&mut *buf, 4, top as u16);
    debug_assert!(
        HEADER + leaf.entries.len() * 2 <= top,
        "slots overrun cells"
    );
    buf
}

pub fn encode_internal(node: &Internal) -> Box<[u8; PAGE_SIZE]> {
    debug_assert!(node.size() <= PAGE_SIZE, "internal overfull at encode");
    let mut buf = Box::new([0u8; PAGE_SIZE]);
    buf[0] = INTERNAL;
    put_u16(&mut *buf, 2, node.entries.len() as u16);
    put_u64(&mut *buf, 8, node.leftmost);

    let mut top = PAGE_SIZE;
    for (i, (k, child)) in node.entries.iter().enumerate() {
        let cell = 2 + k.len() + 8;
        top -= cell;
        put_u16(&mut *buf, top, k.len() as u16);
        buf[top + 2..top + 2 + k.len()].copy_from_slice(k);
        put_u64(&mut *buf, top + 2 + k.len(), *child);
        put_u16(&mut *buf, HEADER + i * 2, top as u16);
    }
    put_u16(&mut *buf, 4, top as u16);
    debug_assert!(
        HEADER + node.entries.len() * 2 <= top,
        "slots overrun cells"
    );
    buf
}

pub fn decode(buf: &[u8]) -> Node {
    match buf[0] {
        LEAF => Node::Leaf(decode_leaf(buf)),
        INTERNAL => Node::Internal(decode_internal(buf)),
        other => panic!("decode: not a node page (type {other})"),
    }
}

pub fn decode_leaf(buf: &[u8]) -> Leaf {
    debug_assert_eq!(buf[0], LEAF);
    let n = get_u16(buf, 2);
    let next = get_u64(buf, 8);
    let mut entries = Vec::with_capacity(n);
    for i in 0..n {
        let off = get_u16(buf, HEADER + i * 2);
        let klen = get_u16(buf, off);
        let vlen = get_u16(buf, off + 2);
        let k = buf[off + 4..off + 4 + klen].to_vec();
        let v = buf[off + 4 + klen..off + 4 + klen + vlen].to_vec();
        entries.push((k, v));
    }
    Leaf { next, entries }
}

pub fn decode_internal(buf: &[u8]) -> Internal {
    debug_assert_eq!(buf[0], INTERNAL);
    let n = get_u16(buf, 2);
    let leftmost = get_u64(buf, 8);
    let mut entries = Vec::with_capacity(n);
    for i in 0..n {
        let off = get_u16(buf, HEADER + i * 2);
        let klen = get_u16(buf, off);
        let k = buf[off + 2..off + 2 + klen].to_vec();
        let child = get_u64(buf, off + 2 + klen);
        entries.push((k, child));
    }
    Internal { leftmost, entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_roundtrip() {
        let leaf = Leaf {
            next: 42,
            entries: vec![
                (b"a".to_vec(), b"1".to_vec()),
                (b"bb".to_vec(), b"".to_vec()),
                (b"ccc".to_vec(), b"value".to_vec()),
            ],
        };
        let buf = encode_leaf(&leaf);
        let got = decode_leaf(&*buf);
        assert_eq!(got.next, 42);
        assert_eq!(got.entries, leaf.entries);
    }

    #[test]
    fn internal_roundtrip() {
        let node = Internal {
            leftmost: 7,
            entries: vec![(b"m".to_vec(), 8), (b"t".to_vec(), 9)],
        };
        let buf = encode_internal(&node);
        let got = decode_internal(&*buf);
        assert_eq!(got.leftmost, 7);
        assert_eq!(got.entries, node.entries);
    }

    #[test]
    fn child_routing() {
        // seps [m, t] => children c0(<m), c1[m,t), c2(>=t)
        let node = Internal {
            leftmost: 100,
            entries: vec![(b"m".to_vec(), 101), (b"t".to_vec(), 102)],
        };
        assert_eq!(node.child_at(node.child_index(b"a")), 100);
        assert_eq!(node.child_at(node.child_index(b"m")), 101);
        assert_eq!(node.child_at(node.child_index(b"p")), 101);
        assert_eq!(node.child_at(node.child_index(b"t")), 102);
        assert_eq!(node.child_at(node.child_index(b"z")), 102);
    }
}
