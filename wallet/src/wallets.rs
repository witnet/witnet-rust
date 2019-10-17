use diesel::prelude::*;

use crate::*;

static MIGRATIONS: &[&str] =
    &["create table if not exists wallets(id integer primary key, name varchar not null, caption varchar)"];

pub fn migrate<C>(db: &C) -> result::Result<()>
where
    C: db::Connection,
{
    log::debug!("Starting wallets-database migrations.");
    for migration in MIGRATIONS {
        db.execute(migration)?;
    }
    log::debug!("Finished wallets-database migrations.");

    Ok(())
}

pub fn list<C>(db: &C) -> result::Result<Vec<models::WalletInfo>>
where
    C: db::Connection,
{
    use schema::wallets::dsl::*;

    let infos = wallets.limit(100).load::<models::WalletInfo>(db)?;

    Ok(infos)
}
