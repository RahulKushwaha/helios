use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use crate::byte::{read_u32, write_u32};
use crate::page::{Page, PAGE_SIZE};

const MAGIC: &[u8; 8] = b"SIMPLEDB";

/// Decoded meta page (page 0).
pub struct Meta {
    pub root: u32,
    pub num_pages: u32,
    /// Tree height: 0 = empty, 1 = root is leaf, 2+ = has internal levels.
    pub height: u32,
    /// Page ids below this belong to the base file (0 for standalone).
    pub base_offset: u32,
    pub base_path: Option<PathBuf>,
}

pub struct FilePager {
    file: File,
    path: PathBuf,
    pub meta: Meta,
    /// Pages written since the last sync. Source of truth until flushed.
    dirty: BTreeMap<u32, Page>,
    /// Read-only base for ids < base_offset that have not been copied up.
    base: Option<Box<FilePager>>,
    base_offset: u32,
    /// copied[id] is true once base page `id` has been written into this file.
    copied: Vec<bool>,
}

impl FilePager {
    /// Open or create a standalone database file.
    pub fn open(path: &Path) -> io::Result<FilePager> {
        FilePager::open_inner(path, true)
    }

    /// Create a fresh overlay file at `top` that reads through `base_path`.
    pub fn create_overlay(top: &Path, base_path: &Path) -> io::Result<FilePager> {
        let base = FilePager::open_inner(base_path, false)?;
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
            height: base.meta.height,
            base_offset,
            base_path: Some(canon),
        };
        let mut pager = FilePager {
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

    fn open_inner(path: &Path, writable: bool) -> io::Result<FilePager> {
        let mut opts = OpenOptions::new();
        opts.read(true);
        if writable {
            opts.write(true).create(true);
        }
        let file = opts.open(path)?;
        let len = file.metadata()?.len();

        if writable && len == 0 {
            // Fresh standalone file: page 0 = meta, page 1 = empty root leaf.
            let meta = Meta {
                root: 1,
                num_pages: 2,
                height: 1,
                base_offset: 0,
                base_path: None,
            };
            let mut pager = FilePager {
                file,
                path: path.to_path_buf(),
                meta,
                dirty: BTreeMap::new(),
                base: None,
                base_offset: 0,
                copied: Vec::new(),
            };
            pager.write_page(1, Page::new(1));
            pager.sync()?;
            return Ok(pager);
        }

        // Read existing meta page.
        let mut buf = vec![0u8; PAGE_SIZE];
        file.read_exact_at(&mut buf, 0)?;
        if &buf[0..8] != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad magic"));
        }
        let root = read_u32(&buf, 8);
        let num_pages = read_u32(&buf, 12);
        let height = read_u32(&buf, 16);
        let base_offset = read_u32(&buf, 20);
        let bp_len = read_u32(&buf, 24) as usize;
        let base_path = if bp_len > 0 {
            let s = String::from_utf8_lossy(&buf[28..28 + bp_len]).into_owned();
            Some(PathBuf::from(s))
        } else {
            None
        };

        let meta = Meta { root, num_pages, height, base_offset, base_path: base_path.clone() };

        let (base, copied) = match &base_path {
            Some(bp) => {
                let base = FilePager::open_inner(bp, false)?;
                let nbits = base_offset as usize;
                let nbytes = nbits.div_ceil(8);
                let mut bm = vec![0u8; nbytes];
                if nbytes > 0 {
                    file.read_exact_at(&mut bm, num_pages as u64 * PAGE_SIZE as u64)?;
                }
                (Some(Box::new(base)), unpack_bits(&bm, nbits))
            }
            None => (None, Vec::new()),
        };

        Ok(FilePager {
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

    /// Load raw page bytes, resolving the overlay chain.
    fn load_raw(&self, id: u32) -> io::Result<Vec<u8>> {
        let in_top = id >= self.base_offset
            || self.copied.get(id as usize).copied().unwrap_or(false);

        if in_top {
            if let Some(page) = self.dirty.get(&id) {
                return Ok(page.as_bytes().to_vec());
            }
            let mut buf = vec![0u8; PAGE_SIZE];
            self.file.read_exact_at(&mut buf, id as u64 * PAGE_SIZE as u64)?;
            Ok(buf)
        } else {
            self.base.as_ref().expect("overlay without base").load_raw(id)
        }
    }

    /// Read a page, resolving the overlay. Returns an owned clone.
    pub fn read_page(&self, id: u32) -> io::Result<Page> {
        let in_top = id >= self.base_offset
            || self.copied.get(id as usize).copied().unwrap_or(false);

        if in_top {
            if let Some(page) = self.dirty.get(&id) {
                return Ok(page.clone());
            }
            let mut buf = vec![0u8; PAGE_SIZE];
            self.file.read_exact_at(&mut buf, id as u64 * PAGE_SIZE as u64)?;
            Ok(Page::from_bytes(&buf))
        } else {
            self.base.as_ref().expect("overlay without base").read_page(id)
        }
    }

    /// Load a page into the dirty cache and return a mutable reference.
    /// Base pages are marked as copied-up on first write.
    pub fn page_mut(&mut self, id: u32) -> io::Result<&mut Page> {
        if !self.dirty.contains_key(&id) {
            let page = self.read_page(id)?;
            self.dirty.insert(id, page);
        }
        if id < self.base_offset {
            self.copied[id as usize] = true;
        }
        Ok(self.dirty.get_mut(&id).unwrap())
    }

    /// Buffer a page directly (e.g. a freshly split page).
    pub fn write_page(&mut self, id: u32, page: Page) {
        if id < self.base_offset {
            self.copied[id as usize] = true;
        }
        self.dirty.insert(id, page);
    }

    /// Allocate a fresh page id (always a native id >= base_offset).
    pub fn alloc(&mut self) -> u32 {
        let id = self.meta.num_pages;
        self.meta.num_pages += 1;
        id
    }

    /// Allocate and register a new empty leaf page.
    pub fn new_page(&mut self) -> u32 {
        let id = self.alloc();
        self.write_page(id, Page::new(id));
        id
    }

    /// Flush dirty pages, the copied bitmap, then the meta page (with fsyncs).
    pub fn sync(&mut self) -> io::Result<()> {
        for (id, page) in &self.dirty {
            self.file
                .write_all_at(page.as_bytes(), *id as u64 * PAGE_SIZE as u64)?;
        }
        self.write_bitmap()?;
        self.file.sync_data()?;

        self.file.write_all_at(&self.encode_meta(), 0)?;
        self.file.sync_data()?;

        self.dirty.clear();
        Ok(())
    }

    /// Force-rewrite every resident page (not just dirty ones) and fsync.
    pub fn flush(&mut self) -> io::Result<()> {
        for id in 1..self.meta.num_pages {
            let resident =
                id >= self.base_offset || self.copied.get(id as usize).copied().unwrap_or(false);
            if !resident {
                continue;
            }
            let raw = self.load_raw(id)?;
            self.file.write_all_at(&raw, id as u64 * PAGE_SIZE as u64)?;
        }
        self.write_bitmap()?;
        self.file.sync_all()?;

        self.file.write_all_at(&self.encode_meta(), 0)?;
        self.file.sync_all()?;

        self.dirty.clear();
        Ok(())
    }

    /// Write a complete standalone copy of the whole tree to `path`, collapsing
    /// the overlay chain so the result has no base dependency.
    pub fn full_snapshot(&self, path: &Path) -> io::Result<()> {
        let out = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;

        for id in 1..self.meta.num_pages {
            let raw = self.load_raw(id)?;
            out.write_all_at(&raw, id as u64 * PAGE_SIZE as u64)?;
        }

        // Standalone meta: same tree, no base.
        let mut meta = vec![0u8; PAGE_SIZE];
        meta[0..8].copy_from_slice(MAGIC);
        write_u32(&mut meta, 8, self.meta.root);
        write_u32(&mut meta, 12, self.meta.num_pages);
        write_u32(&mut meta, 16, self.meta.height);
        // base_offset = 0, base_path_len = 0 (standalone)
        out.write_all_at(&meta, 0)?;
        out.sync_all()?;
        Ok(())
    }

    fn write_bitmap(&self) -> io::Result<()> {
        if self.base_offset > 0 {
            let bm = pack_bits(&self.copied);
            self.file
                .write_all_at(&bm, self.meta.num_pages as u64 * PAGE_SIZE as u64)?;
        }
        Ok(())
    }

    fn encode_meta(&self) -> Vec<u8> {
        let mut buf = vec![0u8; PAGE_SIZE];
        buf[0..8].copy_from_slice(MAGIC);
        write_u32(&mut buf, 8, self.meta.root);
        write_u32(&mut buf, 12, self.meta.num_pages);
        write_u32(&mut buf, 16, self.meta.height);
        write_u32(&mut buf, 20, self.meta.base_offset);
        if let Some(bp) = &self.meta.base_path {
            let bytes = bp.to_string_lossy();
            let bytes = bytes.as_bytes();
            assert!(28 + bytes.len() <= PAGE_SIZE, "base path too long for meta");
            write_u32(&mut buf, 24, bytes.len() as u32);
            buf[28..28 + bytes.len()].copy_from_slice(bytes);
        }
        buf
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
        .map(|i| bytes.get(i / 8).is_some_and(|&byte| byte & (1 << (i % 8)) != 0))
        .collect()
}
