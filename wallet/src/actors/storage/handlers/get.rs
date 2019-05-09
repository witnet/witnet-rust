//! # `Get` message and message handler
//!
//! The `Get` message is used to query the database.
use std::marker::PhantomData;
use std::sync::Arc;

use actix::prelude::*;
use bincode::{deserialize, serialize};
use serde::{de::DeserializeOwned, Serialize};

use crate::actors::storage::{error::Error, Storage};

/// Message for getting a value from the database.
pub struct Get<Key, Value> {
    key: Arc<Key>,
    t: PhantomData<Value>,
}

impl<Key, Value> Get<Key, Value>
where
    Key: Serialize,
{
    /// Construct a Get message using ownership.
    #[allow(dead_code)]
    pub fn with_key(key: Key) -> Self {
        Self {
            key: key.into(),
            t: PhantomData,
        }
    }

    /// Construct a Get message using borrowing.
    #[allow(dead_code)]
    pub fn with_shared_key(key: Arc<Key>) -> Self {
        Self {
            key,
            t: PhantomData,
        }
    }
}

impl<Value> Get<&'static str, Value> {
    /// Construct a Get message using a string literal.
    pub fn with_static_key(key_str: &'static str) -> Self {
        Self {
            key: key_str.into(),
            t: PhantomData,
        }
    }
}

impl<Key, Value> Message for Get<Key, Value>
where
    Value: 'static,
{
    type Result = Result<Option<Value>, Error>;
}

impl<Key, Value> Handler<Get<Key, Value>> for Storage
where
    Key: Serialize,
    Value: DeserializeOwned + 'static,
{
    type Result = Result<Option<Value>, Error>;

    fn handle(&mut self, msg: Get<Key, Value>, _ctx: &mut Self::Context) -> Self::Result {
        let db = self.wallets.as_ref().map_err(|e| Error::Db(e.clone()))?;
        let key = serialize(msg.key.as_ref()).map_err(Error::Serialization)?;
        let result = db.get(key).map_err(Error::Db)?;

        if let Some(db_vec) = result {
            let value = deserialize(db_vec.as_ref()).map_err(Error::Serialization)?;

            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
}
