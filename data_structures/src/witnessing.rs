/// Holds witnessing configuration after it has been validated.
///
/// This is ready to use with `witnet_node::actors::RadManager::from_config` or in
/// `witnet_wallet::Params`.
#[derive(Clone, Debug)]
pub struct WitnessingConfig<T>
where
    T: Clone + core::fmt::Debug,
{
    pub transports: Vec<Option<T>>,
    pub paranoid_threshold: f32,
}

impl<T> Default for WitnessingConfig<T>
where
    T: Clone + core::fmt::Debug,
{
    fn default() -> Self {
        Self {
            transports: vec![None],
            paranoid_threshold: 0.51,
        }
    }
}

impl<T> WitnessingConfig<T>
where
    T: Clone + core::fmt::Debug + core::fmt::Display,
{
    pub fn transports_as<T2>(&self) -> Result<Vec<Option<T2>>, (T, <T2 as core::str::FromStr>::Err)>
    where
        T2: core::str::FromStr,
    {
        let mut transports = Vec::<Option<T2>>::new();

        for transport in self.transports.iter().cloned() {
            transports.push(match transport {
                Some(t) => Some(T2::from_str(t.to_string().as_str()).map_err(|e| (t, e))?),
                None => None,
            });
        }

        Ok(transports)
    }
}
