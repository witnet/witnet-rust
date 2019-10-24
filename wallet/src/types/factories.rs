use std::{env, fs, path};

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

impl Factory for State {
    fn factory() -> Self {
        Self {
            db: db::Database::in_memory(),
            db_path: path::PathBuf::factory(),
            wallets_config: WalletsConfig::default(),
            sign_engine: SignEngine::signing_only(),
            rng: Arc::new(Mutex::new(rand::rngs::OsRng)),
            sessions: Default::default(),
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.db_path);
    }
}

impl Factory for path::PathBuf {
    fn factory() -> Self {
        let path = mktemp::Temp::new_dir()
            .expect("mktemp failed")
            .to_path_buf();
        fs::create_dir_all(&path).expect("create tmp dir failed");

        path
    }
}
