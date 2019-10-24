use super::*;

impl Session {
    pub fn new(expires_in: time::Duration) -> Self {
        let expiration = time::Instant::now()
            .checked_add(expires_in)
            .expect("instant overflow");

        Self {
            expiration,
            wallets: HashMap::default(),
        }
    }

    pub fn is_expired(&self) -> bool {
        time::Instant::now() > self.expiration
    }
}
