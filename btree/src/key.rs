use std::fmt::Display;

pub struct Key<D> {
    data: D,
}

impl<D> Key<D> {
    pub fn from(data: D) -> Self {
        Self { data }
    }
}

impl<D: AsRef<[u8]>> Display for Key<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key({:?})", self.data.as_ref())
    }
}

impl<D: AsRef<[u8]>> PartialEq for Key<D> {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl<D: AsRef<[u8]>> Eq for Key<D> {}

impl<D: AsRef<[u8]>> PartialOrd for Key<D> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<D: AsRef<[u8]>> Ord for Key<D> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.data.as_ref().cmp(other.data.as_ref())
    }
}

impl<D: AsRef<[u8]>> Key<D> {
    pub fn bytes(&self) -> &D {
        &self.data
    }

    pub fn len(&self) -> usize {
        self.data.as_ref().len()
    }
}
