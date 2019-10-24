use std::path;

use diesel::r2d2;

pub trait Connection: diesel::Connection<Backend = diesel::sqlite::Sqlite> {}

impl<T> Connection for T where T: diesel::Connection<Backend = diesel::sqlite::Sqlite> {}

type ManagedConnection = r2d2::ConnectionManager<diesel::SqliteConnection>;

/// Return a suitable database url.
pub fn url(path: &path::Path, name: &str) -> String {
    path.join(format!("{}.db", name))
        .to_str()
        .map(|s| s.to_string())
        .expect("db path to url failed")
}

/// Return a path to the given database.
pub fn path(path: &path::Path, name: &str) -> path::PathBuf {
    path.join(format!("{}.db", name))
}

#[derive(Clone)]
pub struct Database {
    pool: r2d2::Pool<ManagedConnection>,
}

impl Database {
    pub fn open(url: &str) -> Result<Self, r2d2::PoolError> {
        let pool = r2d2::Pool::builder().build(r2d2::ConnectionManager::new(url))?;

        Ok(Self { pool })
    }

    pub fn get(&self) -> Result<r2d2::PooledConnection<ManagedConnection>, r2d2::PoolError> {
        self.pool.get()
    }
}

#[cfg(test)]
impl Database {
    pub fn in_memory() -> Self {
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(r2d2::ConnectionManager::new(":memory:"))
            .expect("Error stablishing connection to in-memory database");

        Self { pool }
    }
}
