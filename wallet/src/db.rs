use diesel::r2d2;

pub trait Connection: diesel::Connection<Backend = diesel::sqlite::Sqlite> {}

impl<T> Connection for T where T: diesel::Connection<Backend = diesel::sqlite::Sqlite> {}

type ManagedConnection = r2d2::ConnectionManager<diesel::SqliteConnection>;

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
