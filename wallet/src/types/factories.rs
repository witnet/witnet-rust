use super::*;

pub trait Factory {
    fn factory() -> Self;
}

impl Factory for Account {
    fn factory() -> Self {
        let internal_key = ExtendedSK::factory();
        let external_key = ExtendedSK::factory();
        let index = rand::random();

        Self {
            index,
            internal_key,
            external_key,
        }
    }
}

impl Factory for ExtendedSK {
    fn factory() -> Self {
        let key = SK::from_slice(&rand::random::<[u8; 32]>()).unwrap();
        let chain_code = Protected::new(rand::random::<[u8; 32]>().as_ref());

        ExtendedSK::new(key, chain_code)
    }
}
