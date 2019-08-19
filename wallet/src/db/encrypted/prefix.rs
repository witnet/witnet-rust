#[derive(Clone)]
pub struct Prefixer {
    prefix: Vec<u8>,
}

impl Prefixer {
    pub fn new(prefix: Vec<u8>) -> Self {
        Self { prefix }
    }

    pub fn prefix<K>(&self, key: &K) -> Vec<u8>
    where
        K: AsRef<[u8]> + ?Sized,
    {
        [self.prefix.as_slice(), key.as_ref()].concat()
    }
}
