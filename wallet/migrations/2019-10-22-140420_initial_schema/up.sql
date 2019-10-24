-- Your SQL goes here
CREATE TABLE IF NOT EXISTS accounts (
  id INTEGER PRIMARY KEY NOT NULL,
  idx INTEGER UNIQUE,
  balance INTEGER DEFAULT 0,
  internal_key BLOB NOT NULL,
  internal_chain_code BLOB NOT NULL,
  external_key BLOB NOT NULL,
  external_chain_code BLOB NOT NULL
);
