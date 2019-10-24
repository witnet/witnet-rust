use diesel::prelude::*;

use crate::*;

static MIGRATIONS: &[&str] =
    &["create table if not exists wallets(id integer primary key, name varchar not null, caption varchar)"];

pub fn migrate_db(conn: &impl db::Connection) -> result::Result<()> {
    log::debug!("Starting wallets-database migrations.");
    for migration in MIGRATIONS {
        conn.execute(migration)?;
    }
    log::debug!("Finished wallets-database migrations.");

    Ok(())
}

pub fn list(conn: &impl db::Connection) -> result::Result<Vec<models::WalletInfo>> {
    use schema::wallets::dsl::*;

    let infos = wallets.limit(100).load(conn)?;

    Ok(infos)
}

pub fn find(
    conn: &impl db::Connection,
    wallet_id: i32,
) -> result::Result<Option<models::WalletInfo>> {
    use schema::wallets::dsl::*;

    let info = match wallets.find(wallet_id).first(conn) {
        Ok(info) => Ok(Some(info)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(err) => Err(err),
    }?;

    Ok(info)
}

pub fn create(
    conn: &impl db::Connection,
    name: &str,
    caption: Option<&String>,
) -> result::Result<i32> {
    let new_wallet = models::NewWallet { name, caption };
    let id = conn.transaction(|| {
        use schema::wallets::dsl::*;

        diesel::insert_into(wallets)
            .values(&new_wallet)
            .execute(conn)?;

        wallets.order(id.desc()).select(id).first(conn)
    })?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create() {
        let conn = db::Database::in_memory().get().unwrap();

        migrate_db(&conn).unwrap();

        assert_eq!(1, create(&conn, "wallet", None).unwrap());
        assert_eq!(2, create(&conn, "another wallet", None).unwrap());
    }

    #[test]
    fn test_find() {
        let conn = db::Database::in_memory().get().unwrap();

        migrate_db(&conn).unwrap();
        create(&conn, "wallet", None).unwrap();
        create(&conn, "another wallet", None).unwrap();

        let info = find(&conn, 2).unwrap().unwrap();

        assert_eq!("another wallet", &info.name);
        assert_eq!(None, info.caption);

        assert!(find(&conn, 3).unwrap().is_none())
    }
}
