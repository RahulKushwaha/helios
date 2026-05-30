//! The pager owns the file and hands the B+Tree decoded pages.
//!
//! There is **no write-ahead log**. Mutations land in an in-memory `dirty` map
//! keyed by page id; `sync` writes every dirty data page, fsyncs, then writes
//! the meta page and fsyncs again. Writing data before meta is the only
//! crash-safety we get: if we die mid-`sync`, the on-disk meta may still point
//! at the previous tree, but a torn data page can corrupt it. That is the
//! accepted cost of "no WAL".
//!
//! Pages are addressed by id; page `id` lives at file offset `id * PAGE_SIZE`.
//! Page 0 is the meta page. Freed pages form a singly linked stack threaded
//! through their own bytes (`free_head` in meta -> next in the freed page).
//!
//! ## Overlays (copy-on-write on top of a frozen base)
//!
//! A pager may sit on top of a read-only **base** pager, like an overlay
//! filesystem. The boundary is a single number, `base_offset`:
//!
//! - page id `< base_offset`: a *base page*. Read it from the base, unless it
//!   has been copied up into this file (tracked by the `copied` bitmap).
//! - page id `>= base_offset`: *native*. Allocated after the overlay was
//!   created; it only ever lives in this file.
//!
//! Every write goes to this file (copy-up); reads fall through to the base for
//! pages we have not touched. The base never changes, so a hard link of it is a
//! cheap, frozen snapshot. The B+Tree code is oblivious to all of this: it just
//! reads and writes page ids.
//!
//! The `copied` bitmap is `base_offset` bits, written at the tail of the file
//! (just past the page region) on every `sync`.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use crate::page::{
    decode, encode_internal, encode_leaf, Internal, Leaf, Node, PageId, FREE, PAGE_SIZE,
};

const MAGIC: &[u8; 8] = b"HELIOSB1";

/// Decoded meta page (page 0).
#[derive(Debug, Clone)]
pub struct Meta {
    pub root: PageId,
    pub num_pages: u64,
    pub free_head: PageId,
    /// Page ids below this come from `base_path` unless copied up. 0 for a
    /// standalone (non-overlay) database.
    pub base_offset: u64,
    /// The base file this overlay reads through to, if any.
    pub base_path: Option<PathBuf>,
}

pub struct Pager {
    file: File,
    path: PathBuf,
    pub meta: Meta,
    /// Pages modified since the last `sync`. The source of truth until flushed.
    dirty: BTreeMap<PageId, Box<[u8; PAGE_SIZE]>>,
    /// Read-only base for ids `< base_offset` that have not been copied up.
    base: Option<Box<Pager>>,
    base_offset: u64,
    /// `copied[id]` is true once base page `id` has been written into this file.
    /// Length is exactly `base_offset` (empty for a standalone database).
    copied: Vec<bool>,
}

impl Pager {
    /// Open `path` read-write, creating an empty standalone tree if it is new.
    /// If the file's meta names a base, it is opened as a read-through overlay.
    pub fn open(path: &Path) -> io::Result<Pager> {
        Pager::open_inner(path, true)
    }

    /// Create a fresh overlay file at `top` that reads through `base_path`.
    pub fn create_overlay(top: &Path, base_path: &Path) -> io::Result<Pager> {
        let base = Pager::open_inner(base_path, false)?;
        let base_offset = base.meta.num_pages;
        let canon = std::fs::canonicalize(base_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(top)?;
        let meta = Meta {
            root: base.meta.root,
            num_pages: base_offset,
            free_head: 0,
            base_offset,
            base_path: Some(canon),
        };
        let mut pager = Pager {
            file,
            path: top.to_path_buf(),
            meta,
            dirty: BTreeMap::new(),
            base: Some(Box::new(base)),
            base_offset,
            copied: vec![false; base_offset as usize],
        };
        pager.sync()?;
        Ok(pager)
    }

    fn open_inner(path: &Path, writable: bool) -> io::Result<Pager> {
        let mut opts = OpenOptions::new();
        opts.read(true);
        if writable {
            opts.write(true).create(true);
        }
        let file = opts.open(path)?;
        let len = file.metadata()?.len();

        if writable && len == 0 {
            // Fresh standalone file: meta is page 0, root is an empty leaf.
            let meta = Meta {
                root: 1,
                num_pages: 2,
                free_head: 0,
                base_offset: 0,
                base_path: None,
            };
            let mut pager = Pager {
                file,
                path: path.to_path_buf(),
                meta,
                dirty: BTreeMap::new(),
                base: None,
                base_offset: 0,
                copied: Vec::new(),
            };
            pager.write_leaf(1, &Leaf::default());
            pager.sync()?;
            return Ok(pager);
        }

        let mut buf = vec![0u8; PAGE_SIZE];
        file.read_exact_at(&mut buf, 0)?;
        if &buf[0..8] != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad magic"));
        }
        let num_pages = u64::from_le_bytes(buf[16..24].try_into().unwrap());
        let base_offset = u64::from_le_bytes(buf[32..40].try_into().unwrap());
        let bp_len = u64::from_le_bytes(buf[40..48].try_into().unwrap()) as usize;
        let base_path = if bp_len > 0 {
            let s = String::from_utf8_lossy(&buf[48..48 + bp_len]).into_owned();
            Some(PathBuf::from(s))
        } else {
            None
        };
        let meta = Meta {
            root: u64::from_le_bytes(buf[8..16].try_into().unwrap()),
            num_pages,
            free_head: u64::from_le_bytes(buf[24..32].try_into().unwrap()),
            base_offset,
            base_path: base_path.clone(),
        };

        let (base, copied) = match &base_path {
            Some(bp) => {
                let base = Pager::open_inner(bp, false)?;
                let nbits = base_offset as usize;
                let nbytes = nbits.div_ceil(8);
                let mut bm = vec![0u8; nbytes];
                if nbytes > 0 {
                    file.read_exact_at(&mut bm, num_pages * PAGE_SIZE as u64)?;
                }
                (Some(Box::new(base)), unpack_bits(&bm, nbits))
            }
            None => (None, Vec::new()),
        };

        Ok(Pager {
            file,
            path: path.to_path_buf(),
            meta,
            dirty: BTreeMap::new(),
            base,
            base_offset,
            copied,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load raw page bytes, resolving the overlay: this file if the page is
    /// native or copied up, otherwise read through to the base.
    fn load(&self, id: PageId) -> io::Result<Box<[u8; PAGE_SIZE]>> {
        let in_top = id >= self.base_offset || self.copied[id as usize];
        if in_top {
            if let Some(buf) = self.dirty.get(&id) {
                return Ok(buf.clone());
            }
            let mut buf = Box::new([0u8; PAGE_SIZE]);
            self.file.read_exact_at(&mut *buf, id * PAGE_SIZE as u64)?;
            Ok(buf)
        } else {
            self.base.as_ref().expect("overlay without base").load(id)
        }
    }

    /// Buffer a page in this file, marking base pages as copied up.
    fn store(&mut self, id: PageId, buf: Box<[u8; PAGE_SIZE]>) {
        self.dirty.insert(id, buf);
        if id < self.base_offset {
            self.copied[id as usize] = true;
        }
    }

    pub fn read_node(&self, id: PageId) -> io::Result<Node> {
        Ok(decode(&*self.load(id)?))
    }

    pub fn read_leaf(&self, id: PageId) -> io::Result<Leaf> {
        match self.read_node(id)? {
            Node::Leaf(l) => Ok(l),
            Node::Internal(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected leaf, found internal",
            )),
        }
    }

    pub fn read_internal(&self, id: PageId) -> io::Result<Internal> {
        match self.read_node(id)? {
            Node::Internal(n) => Ok(n),
            Node::Leaf(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected internal, found leaf",
            )),
        }
    }

    pub fn write_leaf(&mut self, id: PageId, leaf: &Leaf) {
        self.store(id, encode_leaf(leaf));
    }

    pub fn write_internal(&mut self, id: PageId, node: &Internal) {
        self.store(id, encode_internal(node));
    }

    /// Allocate a page id, reusing the free list before growing the file. Reused
    /// and bump-allocated ids are always native (`>= base_offset`).
    pub fn alloc(&mut self) -> io::Result<PageId> {
        if self.meta.free_head != 0 {
            let id = self.meta.free_head;
            let buf = self.load(id)?;
            // bytes 8..16 hold the next free page id (see `free`).
            self.meta.free_head = u64::from_le_bytes(buf[8..16].try_into().unwrap());
            Ok(id)
        } else {
            let id = self.meta.num_pages;
            self.meta.num_pages += 1;
            Ok(id)
        }
    }

    /// Return a page to the free list. Base pages cannot be reused (their id
    /// still maps to the immutable base), so they are simply abandoned.
    pub fn free(&mut self, id: PageId) {
        if id < self.base_offset {
            return;
        }
        let mut buf = Box::new([0u8; PAGE_SIZE]);
        buf[0] = FREE;
        buf[8..16].copy_from_slice(&self.meta.free_head.to_le_bytes());
        self.store(id, buf);
        self.meta.free_head = id;
    }

    /// Flush dirty pages, the copied-up bitmap, then the meta page, with fsyncs.
    pub fn sync(&mut self) -> io::Result<()> {
        for (id, buf) in &self.dirty {
            debug_assert_ne!(*id, 0, "page 0 is reserved for meta");
            self.file.write_all_at(&**buf, *id * PAGE_SIZE as u64)?;
        }
        self.write_bitmap()?;
        self.file.sync_data()?;

        self.file.write_all_at(&self.encode_meta(), 0)?;
        self.file.sync_data()?;

        self.dirty.clear();
        Ok(())
    }

    /// Force-write *every* page resident in this file to disk, not just the
    /// dirty ones, and fsync. After this, each page id in `0..num_pages` that
    /// belongs to this file has been physically (re)written. Base pages that are
    /// still read through to the base are not copied up (an overlay stays an
    /// overlay); only pages that live here are rewritten.
    pub fn flush(&mut self) -> io::Result<()> {
        for id in 1..self.meta.num_pages {
            let resident = id >= self.base_offset || self.copied[id as usize];
            if !resident {
                continue;
            }
            // `load` returns the dirty buffer if present, else the on-disk bytes.
            let buf = self.load(id)?;
            self.file.write_all_at(&*buf, id * PAGE_SIZE as u64)?;
        }
        self.write_bitmap()?;
        self.file.sync_all()?;

        self.file.write_all_at(&self.encode_meta(), 0)?;
        self.file.sync_all()?;

        self.dirty.clear();
        Ok(())
    }

    /// Write a complete, standalone copy of the whole tree to `path`, resolving
    /// the overlay chain so the result has no base. Every logical page is read
    /// through to wherever it currently lives (this file, or any base) and
    /// written into the new file; the new meta records no base. This database is
    /// left untouched and remains usable.
    pub fn full_snapshot(&self, path: &Path) -> io::Result<()> {
        let out = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;

        // Copy every page in the logical id space, collapsing the chain.
        for id in 1..self.meta.num_pages {
            let buf = self.load(id)?;
            out.write_all_at(&*buf, id * PAGE_SIZE as u64)?;
        }

        // Standalone meta: same tree, but no base (base_offset = 0, no path).
        let mut meta = vec![0u8; PAGE_SIZE];
        meta[0..8].copy_from_slice(MAGIC);
        meta[8..16].copy_from_slice(&self.meta.root.to_le_bytes());
        meta[16..24].copy_from_slice(&self.meta.num_pages.to_le_bytes());
        meta[24..32].copy_from_slice(&self.meta.free_head.to_le_bytes());
        out.write_all_at(&meta, 0)?;
        out.sync_all()?;
        Ok(())
    }

    /// Write the copied-up bitmap at the tail of the file (overlays only).
    fn write_bitmap(&self) -> io::Result<()> {
        if self.base_offset > 0 {
            let bm = pack_bits(&self.copied);
            self.file
                .write_all_at(&bm, self.meta.num_pages * PAGE_SIZE as u64)?;
        }
        Ok(())
    }

    /// Serialize the meta page (page 0).
    fn encode_meta(&self) -> Vec<u8> {
        let mut meta = vec![0u8; PAGE_SIZE];
        meta[0..8].copy_from_slice(MAGIC);
        meta[8..16].copy_from_slice(&self.meta.root.to_le_bytes());
        meta[16..24].copy_from_slice(&self.meta.num_pages.to_le_bytes());
        meta[24..32].copy_from_slice(&self.meta.free_head.to_le_bytes());
        meta[32..40].copy_from_slice(&self.meta.base_offset.to_le_bytes());
        if let Some(bp) = &self.meta.base_path {
            let bytes = bp.to_string_lossy();
            let bytes = bytes.as_bytes();
            assert!(48 + bytes.len() <= PAGE_SIZE, "base path too long for meta");
            meta[40..48].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
            meta[48..48 + bytes.len()].copy_from_slice(bytes);
        }
        meta
    }
}

fn pack_bits(bits: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; bits.len().div_ceil(8)];
    for (i, &b) in bits.iter().enumerate() {
        if b {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

fn unpack_bits(bytes: &[u8], n: usize) -> Vec<bool> {
    (0..n)
        .map(|i| {
            bytes
                .get(i / 8)
                .is_some_and(|&byte| byte & (1 << (i % 8)) != 0)
        })
        .collect()
}
