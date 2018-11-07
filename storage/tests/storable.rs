use witnet_storage::backends::in_memory::InMemoryStorage;
use witnet_storage::error::StorageResult;
use witnet_storage::storage::{Storable, Storage, StorageHelper};

#[test]
fn storable_types() -> StorageResult<()> {
    // Create a new type and implement the storable trait
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct Foo {
        data: u16,
    };
    impl Storable for Foo {
        // Converts a u16 into a Vec<u8>
        fn to_bytes(&self) -> StorageResult<Vec<u8>> {
            let mut v = vec![];
            v.push(self.data as u8);
            v.push((self.data >> 8) as u8);
            Ok(v)
        }
        fn from_bytes(x: &[u8]) -> StorageResult<Self> {
            let data = x[0] as u16 + ((x[1] as u16) << 8);
            Ok(Foo { data })
        }
    }

    // Test with InMemoryStorage
    let mut s = InMemoryStorage::new(())?;

    let f = Foo { data: 10 };
    s.put(b"a", f.to_bytes().unwrap())?;
    assert_eq!(Foo::from_bytes(&s.get(b"a")?.unwrap()).unwrap(), f);

    // Using the helper trait
    assert_eq!(s.get_t::<Foo>(b"a")?.unwrap(), f);

    let f2 = Foo { data: 2 };
    s.put_t(b"c", f2)?;
    let also_f2: Foo = s.get_t(b"c")?.unwrap();
    assert_eq!(also_f2, f2);

    // We can work with anything that can be serialized by serde
    let f3 = format!("Hello, world");
    s.put_t(b"string", f3.clone())?;
    assert_eq!(s.get_t::<String>(b"string")?.unwrap(), f3);

    // Enums and structs are supported, as long as they implement Serialize/Deserialize
    let f4 = Some(56i32);
    s.put_t(b"some_int", f4)?;
    assert_eq!(s.get_t::<Option<i32>>(b"some_int")?.unwrap(), f4);

    Ok(())
}
