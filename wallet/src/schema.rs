table! {
    wallets {
        id -> Integer,
        name -> VarChar,
        caption -> Nullable<VarChar>,
    }
}

table! {
    accounts {
        id -> Integer,
        #[sql_name = "idx"]
        index -> Integer,
        balance -> BigInt,
        internal_key -> Binary,
        internal_chain_code -> Binary,
        external_key -> Binary,
        external_chain_code -> Binary,
    }
}
