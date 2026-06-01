use crate::page::Page;

pub struct LeafPage {
    page: Page,
}

pub struct LeafPageMut<'a> {
    page: &'a mut Page,
}

pub struct LeafSplit {
    pub separator_key: Vec<u8>,
    pub right_page: LeafPage,
}

impl LeafPage {
    pub fn new(page_id: u32) -> Self {
        Self { page: Page::new(page_id) }
    }

    pub fn from_page(p: Page) -> Self {
        Self { page: p }
    }

    pub fn into_page(self) -> Page {
        self.page
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.page.get(key)
    }

    pub fn slot_count(&self) -> u16 {
        self.page.slot_count()
    }

    pub fn key_val_at(&self, slot_id: usize) -> Option<(&[u8], &[u8])> {
        self.page.get_key_value_at_slot(slot_id)
    }

    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::with_capacity(self.slot_count() as usize);
        for i in 0..self.slot_count() as usize {
            if let Some((k, v)) = self.key_val_at(i) {
                out.push((k.to_vec(), v.to_vec()));
            }
        }
        out
    }

    pub fn page_id(&self) -> u32 {
        self.page.page_id()
    }

    pub fn next_leaf_page_id(&self) -> Option<u32> {
        self.page.next_leaf_page_id()
    }
}

impl<'a> LeafPageMut<'a> {
    pub fn new(page: &'a mut Page) -> Self {
        Self { page }
    }

    pub fn page(&self) -> &Page {
        self.page
    }

    pub fn insert(&mut self, key: &[u8], val: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        self.page.put(key, val)
    }

    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.page.remove(key)
    }

    pub fn slot_count(&self) -> u16 {
        self.page.slot_count()
    }

    pub fn key_value_at(&self, slot_idx: usize) -> Option<(&[u8], &[u8])> {
        self.page.get_key_value_at_slot(slot_idx)
    }

    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::with_capacity(self.slot_count() as usize);
        for i in 0..self.slot_count() as usize {
            if let Some((k, v)) = self.key_value_at(i) {
                out.push((k.to_vec(), v.to_vec()));
            }
        }
        out
    }

    pub fn page_id(&self) -> u32 {
        self.page.page_id()
    }

    pub fn next_leaf_page_id(&self) -> Option<u32> {
        self.page.next_leaf_page_id()
    }

    pub fn set_next_leaf_page_id(&mut self, next: Option<u32>) {
        self.page.set_next_leaf_page_id(next);
    }

    pub fn insert_or_split(
        &mut self,
        key: &[u8],
        val: &[u8],
        new_right_page_id: u32,
    ) -> Result<Option<LeafSplit>, &'static str> {
        match self.insert(key, val) {
            Ok(_) => Ok(None),
            Err("page full") => {
                let mut all = self.entries();
                // entries() is already sorted; maintain that by inserting at the
                // correct position, or updating in-place for an existing key.
                match all.binary_search_by(|(k, _)| k.as_slice().cmp(key)) {
                    Ok(i) => all[i].1 = val.to_vec(),
                    Err(i) => all.insert(i, (key.to_vec(), val.to_vec())),
                }

                let mid = all.len() / 2;
                let left_entries = &all[..mid];
                let right_entries = &all[mid..];
                let separator_key = right_entries[0].0.clone();
                let old_next = self.page.next_leaf_page_id();

                let mut new_left = Page::new(self.page.page_id());
                let mut new_right = Page::new(new_right_page_id);
                for (k, v) in left_entries {
                    new_left.put(k, v)?;
                }
                for (k, v) in right_entries {
                    new_right.put(k, v)?;
                }
                new_left.set_next_leaf_page_id(Some(new_right_page_id));
                new_right.set_next_leaf_page_id(old_next);
                *self.page = new_left;

                Ok(Some(LeafSplit {
                    separator_key,
                    right_page: LeafPage::from_page(new_right),
                }))
            }
            Err(e) => Err(e),
        }
    }
}
