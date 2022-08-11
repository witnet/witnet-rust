/// Holds witnessing configuration after it has been validated.
///
/// This is ready to use with `witnet_node::actors::RadManager::from_config` or in
/// `witnet_wallet::Params`.
#[derive(Clone, Debug)]
pub struct WitnessingConfig {
    pub transports: Vec<Option<String>>,
    pub paranoid_threshold: f32,
}

impl Default for WitnessingConfig {
    fn default() -> Self {
        Self {
            transports: vec![None],
            paranoid_threshold: 0.51,
        }
    }
}
