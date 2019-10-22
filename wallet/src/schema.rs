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
        idx -> Integer,
        internal_key -> Binary,
        internal_chain_code -> Binary,
        external_key -> Binary,
        external_chain_code -> Binary,
    }
}
