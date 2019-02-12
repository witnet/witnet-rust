pub trait TryFrom<T>: Sized {
    type Error;

    fn try_from(value: T) -> Result<Self, Self::Error>;
}

pub trait TryInto<T: Sized> {
    type Error;

    fn try_into(self) -> Result<T, Self::Error>;
}

impl<A, B> TryInto<B> for A
where
    B: TryFrom<A>,
{
    type Error = B::Error;

    fn try_into(self) -> Result<B, Self::Error> {
        B::try_from(self)
    }
}
