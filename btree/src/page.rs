use crate::key::Key;
use crate::tuple::TupleBuilder;
use crate::byte::*;

pub const PAGE_SIZE: usize = 8 * 1024;
pub const HEADER_SIZE: usize = 20;
pub const PAGE_MAGIC: u32 = 0x504E5554; // 'PNUT'

const HDR_MAGIC_OFF: usize = 0;
const HDR_PAGE_ID_OFF: usize = 4;
const HDR_SLOT_CNT_OFF: usize = 8;
const HDR_FREE_START_OFF: usize = 10;
const HDR_FREE_END_OFF: usize = 12;
const HDR_DEAD_BYTES_OFF: usize = 14;
const HDR_NEXT_LEAF_PAGE_ID_OFF: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchResult {
    Found(usize),
    NotFound(usize),
}

#[derive(Debug, Clone)]
pub struct Page {
    data: [u8; PAGE_SIZE],
    dead_tuple_compact_percent: u8,
}

impl Page {
    pub const SLOT_SIZE: usize = 2;
    pub const DEFAULT_DEAD_TUPLE_COMPACT_PERCENT: u8 = 75;

    pub fn new(page_id: u32) -> Self {
        let mut p = Page {
            data: [0u8; PAGE_SIZE],
            dead_tuple_compact_percent: Self::DEFAULT_DEAD_TUPLE_COMPACT_PERCENT,
        };
        write_u32(&mut p.data, HDR_MAGIC_OFF, PAGE_MAGIC);
        write_u32(&mut p.data, HDR_PAGE_ID_OFF, page_id);
        write_u16(&mut p.data, HDR_SLOT_CNT_OFF, 0);
        write_u16(&mut p.data, HDR_FREE_START_OFF, HEADER_SIZE as u16);
        write_u16(&mut p.data, HDR_FREE_END_OFF, PAGE_SIZE as u16);
        write_u16(&mut p.data, HDR_DEAD_BYTES_OFF, 0);
        write_u32(&mut p.data, HDR_NEXT_LEAF_PAGE_ID_OFF, 0);
        p
    }

    /// Reconstruct a Page from raw bytes read from disk.
    /// `dead_tuple_compact_percent` is reset to the default since it is not persisted.
    pub fn from_bytes(src: &[u8]) -> Self {
        let mut p = Page {
            data: [0u8; PAGE_SIZE],
            dead_tuple_compact_percent: Self::DEFAULT_DEAD_TUPLE_COMPACT_PERCENT,
        };
        p.data.copy_from_slice(&src[..PAGE_SIZE]);
        p
    }

    pub fn set_dead_tuple_compact_percent(&mut self, percent: u8) {
        self.dead_tuple_compact_percent = percent.clamp(1, 100);
    }

    pub fn slot_count(&self) -> u16 {
        read_u16(&self.data, HDR_SLOT_CNT_OFF)
    }

    pub fn free_start(&self) -> u16 {
        read_u16(&self.data, HDR_FREE_START_OFF)
    }

    pub fn free_end(&self) -> u16 {
        read_u16(&self.data, HDR_FREE_END_OFF)
    }

    pub fn dead_tuple_bytes(&self) -> u16 {
        read_u16(&self.data, HDR_DEAD_BYTES_OFF)
    }

    fn set_dead_tuple_bytes(&mut self, dead_bytes: u16) {
        write_u16(&mut self.data, HDR_DEAD_BYTES_OFF, dead_bytes);
    }

    pub fn next_leaf_page_id(&self) -> Option<u32> {
        match read_u32(&self.data, HDR_NEXT_LEAF_PAGE_ID_OFF) {
            0 => None,
            v => Some(v),
        }
    }

    pub fn set_next_leaf_page_id(&mut self, next: Option<u32>) {
        write_u32(&mut self.data, HDR_NEXT_LEAF_PAGE_ID_OFF, next.unwrap_or(0));
    }

    pub fn page_id(&self) -> u32 {
        read_u32(&self.data, HDR_PAGE_ID_OFF)
    }

    pub fn free_space_bytes(&self) -> usize {
        self.free_end() as usize - self.free_start() as usize
    }

    fn tuple_region_used_bytes(&self) -> usize {
        PAGE_SIZE - self.free_end() as usize
    }

    fn tuple_total_len(&self, off: usize) -> usize {
        Self::TUP_HDR_SIZE + self.read_tuple_key_len(off) + self.read_tuple_val_len(off)
    }

    fn add_dead_tuple_bytes(&mut self, dead_len: usize) {
        let current = self.dead_tuple_bytes() as usize;
        let next = current.saturating_add(dead_len).min(u16::MAX as usize) as u16;
        self.set_dead_tuple_bytes(next);
    }

    fn should_compact_dead_tuples(&self) -> bool {
        let used = self.tuple_region_used_bytes();
        if used == 0 {
            return false;
        }
        (self.dead_tuple_bytes() as usize) * 100 >= used * self.dead_tuple_compact_percent as usize
    }

    fn compact_live_tuples(&mut self) -> Result<(), &'static str> {
        let saved_next = self.next_leaf_page_id();
        let mut live = Vec::with_capacity(self.slot_count() as usize);
        for i in 0..self.slot_count() as usize {
            let off = self.read_slot(i) as usize;
            if self.read_tuple_tombstone(off) == 1 {
                continue;
            }
            live.push((self.read_key(off).to_vec(), self.read_tuple_val(off).to_vec()));
        }
        let mut compacted = Page::new(self.page_id());
        compacted.set_dead_tuple_compact_percent(self.dead_tuple_compact_percent);
        for (k, v) in live {
            compacted.put(&k, &v)?;
        }
        compacted.set_next_leaf_page_id(saved_next);
        self.data = compacted.data;
        Ok(())
    }

    fn maybe_compact_dead_tuples(&mut self) -> Result<(), &'static str> {
        if self.should_compact_dead_tuples() {
            self.compact_live_tuples()?;
        }
        Ok(())
    }

    fn slot_byte_off(i: usize) -> usize {
        HEADER_SIZE + i * Self::SLOT_SIZE
    }

    pub fn read_slot(&self, i: usize) -> u16 {
        read_u16(&self.data, Self::slot_byte_off(i))
    }

    pub fn write_slot(&mut self, i: usize, tuple_off: u16) {
        write_u16(&mut self.data, Self::slot_byte_off(i), tuple_off);
    }

    pub fn alloc_slot(&mut self) -> Result<usize, &'static str> {
        let cnt = self.slot_count() as usize;
        let new_cnt = cnt + 1;
        let new_free_start = HEADER_SIZE + new_cnt * Self::SLOT_SIZE;
        if new_free_start > self.free_end() as usize {
            return Err("no space for slot");
        }
        write_u16(&mut self.data, HDR_SLOT_CNT_OFF, new_cnt as u16);
        write_u16(&mut self.data, HDR_FREE_START_OFF, new_free_start as u16);
        Ok(cnt)
    }

    const TUP_HDR_SIZE: usize = 1 + 2 + 2;

    pub fn tuple_len(key_len: usize, val_len: usize) -> usize {
        Self::TUP_HDR_SIZE + key_len + val_len
    }

    pub fn alloc_tuple(&mut self, len: usize) -> Result<u16, &'static str> {
        let fe = self.free_end() as usize;
        let fs = self.free_start() as usize;
        if len > fe - fs {
            return Err("no space for tuple");
        }
        let new_fe = fe - len;
        write_u16(&mut self.data, HDR_FREE_END_OFF, new_fe as u16);
        Ok(new_fe as u16)
    }

    pub fn write_tuple(&mut self, off: u16, tombstone: u8, key: &[u8], val: &[u8]) {
        let o = off as usize;
        self.data[o] = tombstone;
        write_u16(&mut self.data, o + 1, key.len() as u16);
        write_u16(&mut self.data, o + 3, val.len() as u16);
        let ks = o + Self::TUP_HDR_SIZE;
        self.data[ks..ks + key.len()].copy_from_slice(key);
        let vs = ks + key.len();
        self.data[vs..vs + val.len()].copy_from_slice(val);
    }

    pub fn read_key<'a>(&'a self, off: usize) -> &'a [u8] {
        let klen = read_u16(&self.data, off + 1) as usize;
        let start = off + Self::TUP_HDR_SIZE;
        &self.data[start..start + klen]
    }

    pub fn as_bytes(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }

    pub fn read_tuple_tombstone(&self, off: usize) -> u8 {
        self.data[off]
    }

    pub fn read_tuple_key_len(&self, off: usize) -> usize {
        read_u16(&self.data, off + 1) as usize
    }

    pub fn read_tuple_val_len(&self, off: usize) -> usize {
        read_u16(&self.data, off + 3) as usize
    }

    pub fn read_tuple_val<'a>(&'a self, off: usize) -> &'a [u8] {
        let klen = self.read_tuple_key_len(off);
        let vlen = self.read_tuple_val_len(off);
        let start = off + Self::TUP_HDR_SIZE + klen;
        &self.data[start..start + vlen]
    }

    fn find_slot(&self, key: &[u8]) -> SearchResult {
        let search_key = Key::from(key);
        let mut lo = 0usize;
        let mut hi = self.slot_count() as usize;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let tuple_off = self.read_slot(mid) as usize;
            let mid_key = Key::from(self.read_key(tuple_off));
            match mid_key.cmp(&search_key) {
                std::cmp::Ordering::Equal => return SearchResult::Found(mid),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        SearchResult::NotFound(lo)
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        match self.find_slot(key) {
            SearchResult::Found(i) => {
                let off = self.read_slot(i) as usize;
                if self.read_tuple_tombstone(off) == 1 {
                    None
                } else {
                    Some(self.read_tuple_val(off))
                }
            }
            SearchResult::NotFound(_) => None,
        }
    }

    pub fn put(&mut self, key: &[u8], val: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        let new_tuple = TupleBuilder::new().flags(0).key(key).value(val).build();
        let tuple_bytes = new_tuple.as_bytes();

        match self.find_slot(key) {
            SearchResult::Found(i) => {
                let needed = tuple_bytes.len();
                if self.free_space_bytes() < needed {
                    self.maybe_compact_dead_tuples()?;
                    if self.free_space_bytes() < needed {
                        return Err("page full");
                    }
                }
                let old_off = self.read_slot(i) as usize;
                let old_val = self.read_tuple_val(old_off).to_vec();
                self.data[old_off] = 1;
                self.add_dead_tuple_bytes(self.tuple_total_len(old_off));
                let new_off = self.alloc_tuple(tuple_bytes.len())? as usize;
                self.data[new_off..new_off + tuple_bytes.len()].copy_from_slice(tuple_bytes);
                self.write_slot(i, new_off as u16);
                self.maybe_compact_dead_tuples()?;
                Ok(Some(old_val))
            }
            SearchResult::NotFound(pos) => {
                let needed = tuple_bytes.len() + Self::SLOT_SIZE;
                if self.free_space_bytes() < needed {
                    self.maybe_compact_dead_tuples()?;
                    if self.free_space_bytes() < needed {
                        return Err("page full");
                    }
                }
                let new_off = self.alloc_tuple(tuple_bytes.len())? as usize;
                self.data[new_off..new_off + tuple_bytes.len()].copy_from_slice(tuple_bytes);
                let slot_idx = self.alloc_slot()?;
                for i in (pos..slot_idx).rev() {
                    let v = self.read_slot(i);
                    self.write_slot(i + 1, v);
                }
                self.write_slot(pos, new_off as u16);
                Ok(None)
            }
        }
    }

    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        match self.find_slot(key) {
            SearchResult::Found(i) => {
                let off = self.read_slot(i) as usize;
                let old_val = self.read_tuple_val(off).to_vec();
                self.data[off] = 1;
                self.add_dead_tuple_bytes(self.tuple_total_len(off));
                let cnt = self.slot_count() as usize;
                for j in i + 1..cnt {
                    let v = self.read_slot(j);
                    self.write_slot(j - 1, v);
                }
                let new_cnt = cnt - 1;
                write_u16(&mut self.data, HDR_SLOT_CNT_OFF, new_cnt as u16);
                write_u16(
                    &mut self.data,
                    HDR_FREE_START_OFF,
                    (HEADER_SIZE + new_cnt * Self::SLOT_SIZE) as u16,
                );
                let _ = self.maybe_compact_dead_tuples();
                Some(old_val)
            }
            SearchResult::NotFound(_) => None,
        }
    }

    pub fn get_key_value_at_slot(&self, slot_idx: usize) -> Option<(&[u8], &[u8])> {
        if slot_idx >= self.slot_count() as usize {
            return None;
        }
        let off = self.read_slot(slot_idx) as usize;
        if self.read_tuple_tombstone(off) == 1 {
            return None;
        }
        Some((self.read_key(off), self.read_tuple_val(off)))
    }
}
