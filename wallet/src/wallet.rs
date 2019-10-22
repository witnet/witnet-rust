use std::fs;

use diesel::prelude::*;

use crate::*;

embed_migrations!();

pub fn create(db_url: &str, password: &str, account: &types::Account) -> result::Result<()> {
    log::trace!("Creating database {}", db_url);
    let db = db::Database::open(db_url)?;
    let conn = db.get()?;
    let task = set_db_password(&conn, password)
        .and_then(|_| migrate_db(&conn))
        .and_then(|_| create_account(&conn, account));

    if let Err(err) = task {
        log::error!("Database creation failed, rolling back: {}", err);

        if let Err(err) = fs::remove_file(db_url) {
            log::error!("Couldn't remove uninitialized database: {}", err);
        }

        return Err(err);
    }

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

fn create_account(conn: &impl db::Connection, account: &types::Account) -> result::Result<()> {
    use schema::accounts;

    let internal_key = &account.internal_key.secret();
    let internal_chain_code = &account.internal_key.chain_code();
    let external_key = &account.external_key.secret();
    let external_chain_code = &account.external_key.chain_code();

    let new_account = models::NewAccount {
        idx: account.index as i32,
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

#[cfg(test)]
mod tests {
    use super::*;
    use types::factories::Factory as _;

    #[test]
    fn test_set_db_password() {
        let conn = db::Database::in_memory();
        let result = set_db_password(&conn, "123");

        assert!(result.is_ok(), format!("{:?}", result));
    }

    #[test]
    fn test_create_account() {
        let conn = db::Database::in_memory();

        migrate_db(&conn).unwrap();

        let account = types::Account::factory();
        let result = create_account(&conn, &account);

        assert!(result.is_ok(), format!("{:?}", result));
    }
}
