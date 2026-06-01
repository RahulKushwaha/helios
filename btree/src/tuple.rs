use crate::key::Key;
use std::fmt;
use std::fmt::{Display, Formatter};

pub type OwningTuple = Tuple<Vec<u8>>;

impl OwningTuple {
    pub fn to_ref(&self) -> Tuple<&[u8]> {
        Tuple::from(&self.data[..])
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Default)]
pub struct TupleBuilder {
    flags: u8,
    key: Option<Vec<u8>>,
    value: Option<Vec<u8>>,
}

impl TupleBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn flags(mut self, flags: u8) -> Self {
        self.flags = flags;
        self
    }

    pub fn key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn value(mut self, value: impl Into<Vec<u8>>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Layout: flags (1B) + key_len (2B LE) + val_len (2B LE) + key + value
    pub fn build(self) -> OwningTuple {
        let key = self.key.unwrap_or_default();
        let value = self.value.unwrap_or_default();

        let mut data = vec![0u8; 5 + key.len() + value.len()];
        data[0] = self.flags;
        let key_len = key.len() as u16;
        data[1] = key_len as u8;
        data[2] = (key_len >> 8) as u8;
        let value_len = value.len() as u16;
        data[3] = value_len as u8;
        data[4] = (value_len >> 8) as u8;
        data[5..5 + key.len()].copy_from_slice(&key);
        data[5 + key.len()..].copy_from_slice(&value);

        OwningTuple { data }
    }
}

#[derive(Clone)]
pub struct Tuple<A> {
    pub data: A,
}

impl<A> Tuple<A> {
    pub fn from(data: A) -> Self {
        Self { data }
    }
}

impl<A: AsRef<[u8]>> Tuple<A> {
    const HEADER_SIZE: usize = 5;

    pub fn len(&self) -> usize {
        self.data.as_ref().len()
    }

    pub fn header(&self) -> TupleHeader<&[u8]> {
        TupleHeader::from(&self.data.as_ref()[..Self::HEADER_SIZE])
    }

    pub fn key(&self) -> Key<&[u8]> {
        let key_len = self.header().key_len() as usize;
        let d = &self.data.as_ref()[Self::HEADER_SIZE..Self::HEADER_SIZE + key_len];
        Key::from(d)
    }
}

impl<A: AsRef<[u8]>> Display for Tuple<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Tuple {{ header: {}, key: {} }}", self.header(), self.key())
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> Tuple<A> {
    pub fn header_mut(&mut self) -> TupleHeader<&mut [u8]> {
        TupleHeader::from(&mut self.data.as_mut()[..Self::HEADER_SIZE])
    }
}

pub struct TupleHeader<A> {
    pub data: A,
}

impl<A> TupleHeader<A> {
    fn from(data: A) -> Self {
        Self { data }
    }
}

impl<A: AsRef<[u8]>> TupleHeader<A> {
    const FLAGS_OFFSET: usize = 0;
    const KEY_LEN_OFFSET: usize = 1;
    const VALUE_LEN_OFFSET: usize = 3;

    pub fn key_len(&self) -> u16 {
        u16::from_le_bytes(
            self.data.as_ref()[Self::KEY_LEN_OFFSET..Self::KEY_LEN_OFFSET + 2]
                .try_into()
                .unwrap(),
        )
    }

    pub fn value_len(&self) -> u16 {
        u16::from_le_bytes(
            self.data.as_ref()[Self::VALUE_LEN_OFFSET..Self::VALUE_LEN_OFFSET + 2]
                .try_into()
                .unwrap(),
        )
    }

    pub fn flags(&self) -> u8 {
        self.data.as_ref()[Self::FLAGS_OFFSET]
    }
}

impl<A: AsRef<[u8]>> Display for TupleHeader<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TupleHeader {{ flags: {:02X}, key_len: {}, value_len: {} }}",
            self.flags(),
            self.key_len(),
            self.value_len()
        )
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> TupleHeader<A> {
    pub fn set_key_len(&mut self, len: u16) {
        self.data.as_mut()[Self::KEY_LEN_OFFSET..Self::KEY_LEN_OFFSET + 2]
            .copy_from_slice(&len.to_le_bytes());
    }

    pub fn set_value_len(&mut self, len: u16) {
        self.data.as_mut()[Self::VALUE_LEN_OFFSET..Self::VALUE_LEN_OFFSET + 2]
            .copy_from_slice(&len.to_le_bytes());
    }

    pub fn set_flags(&mut self, flags: u8) {
        self.data.as_mut()[Self::FLAGS_OFFSET] = flags;
    }
}
