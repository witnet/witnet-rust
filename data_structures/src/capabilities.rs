#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Capability {
    /// The base block mining and superblock voting capability
    Mining = 0,
    /// The universal HTTP GET / HTTP POST / WIP-0019 RNG capability
    Witnessing = 1,
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct CapabilityMap<T>
where
    T: Default,
{
    pub mining: T,
    pub witnessing: T,
}

impl<T> CapabilityMap<T>
where
    T: Copy + Default,
{
    #[inline]
    pub fn get(&self, capability: Capability) -> T {
        match capability {
            Capability::Mining => self.mining,
            Capability::Witnessing => self.witnessing,
        }
    }

    #[inline]
    pub fn update(&mut self, capability: Capability, value: T) {
        match capability {
            Capability::Mining => self.mining = value,
            Capability::Witnessing => self.witnessing = value,
        }
    }

    #[inline]
    pub fn update_all(&mut self, value: T) {
        self.mining = value;
        self.witnessing = value;
    }
}
