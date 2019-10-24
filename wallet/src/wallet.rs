use std::fs;

use diesel::prelude::*;

use crate::*;

embed_migrations!();

pub fn accounts(conn: &impl db::Connection) -> result::Result<Vec<models::AccountInfo>> {
    use schema::accounts::dsl::*;

    let infos = accounts.select((index, balance)).limit(100).load(conn)?;

    Ok(infos)
}

pub fn unlock_db(db_url: &str, password: &str) -> result::Result<db::Database> {
    let db = db::Database::open(db_url)?;
    let conn = db.get()?;

    set_db_password(&conn, password)?;
    check_db_password(&conn)?;

    Ok(db)
}

pub fn create(
    db_url: &str,
    password: &str,
    default_account: &types::Account,
) -> result::Result<()> {
    log::trace!("Creating database {}", db_url);
    let db = db::Database::open(db_url)?;
    let conn = db.get()?;
    let task = set_db_password(&conn, password)
        .and_then(|_| migrate_db(&conn))
        .and_then(|_| create_account(&conn, default_account));

    if let Err(err) = task {
        log::error!("Database creation failed, rolling back: {}", err);

        if let Err(err) = fs::remove_file(db_url) {
            log::error!("Couldn't remove uninitialized database: {}", err);
        }

        return Err(err);
    }

    Ok(())
}

pub fn create_account(conn: &impl db::Connection, account: &types::Account) -> result::Result<()> {
    use schema::accounts;

    let internal_key = &account.internal_key.secret();
    let internal_chain_code = &account.internal_key.chain_code();
    let external_key = &account.external_key.secret();
    let external_chain_code = &account.external_key.chain_code();

    let new_account = models::NewAccount {
        index: account.index as i32,
        internal_key,
        internal_chain_code,
        external_key,
        external_chain_code,
    };

    diesel::insert_into(accounts::table)
        .values(&new_account)
        .execute(conn)?;

    Ok(())
}

fn check_db_password(conn: &impl db::Connection) -> result::Result<()> {
    let query = "SELECT COUNT(*) FROM sqlite_master";

    conn.execute(&query).map_err(|_| error::Error::DbPassword)?;

    Ok(())
}

fn set_db_password(conn: &impl db::Connection, password: &str) -> result::Result<()> {
    let query = format!("PRAGMA key = '{}'", password);

    conn.execute(&query)?;

    Ok(())
}

fn migrate_db(conn: &impl db::Connection) -> result::Result<()> {
    embedded_migrations::run(conn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::factories::Factory as _;

    #[test]
    fn test_set_db_password() {
        let conn = db::Database::in_memory().get().unwrap();
        let result = set_db_password(&conn, "123");

        assert!(result.is_ok(), format!("{:?}", result));
    }

    #[test]
    fn test_create_account() {
        let conn = db::Database::in_memory().get().unwrap();

        migrate_db(&conn).unwrap();

        let account = types::Account::factory();
        let result = create_account(&conn, &account);

        assert!(result.is_ok(), format!("{:?}", result));
    }

    #[test]
    fn test_accounts() {
        let conn = db::Database::in_memory().get().unwrap();

        migrate_db(&conn).unwrap();

        assert_eq!(0, accounts(&conn).unwrap().len());

        create_account(&conn, &types::Account::factory()).unwrap();

        assert_eq!(1, accounts(&conn).unwrap().len());
    }
}
