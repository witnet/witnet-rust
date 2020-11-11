use crate::actors::app;
use crate::*;
use bech32::ToBase32;
use std::string::ToString;

#[test]
fn test_validate_mnemonics() {
    let seed_data: types::Password =
        "day voice lake monkey suit bread occur own cattle visit object ordinary"
            .to_string()
            .into();
    let seed_source = "mnemonics".to_string();
    let password: types::Password = "12345678".to_string().into();
    let result = app::methods::validate(password, seed_data, seed_source, None, None, None, None);
    assert!(result.is_ok());
}

#[test]
fn test_validate_mnemonics_err() {
    let seed_data: types::Password =
        "day lake lake monkey suit bread occur own cattle visit object ordinary"
            .to_string()
            .into();
    let seed_source = "mnemonics".to_string();
    let password: types::Password = "12345678".to_string().into();
    let result =
        match app::methods::validate(password, seed_data, seed_source, None, None, None, None) {
            Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
            Err(e) => e.into_parts(),
        };
    let expected =
        app::validation_error(app::field_error("seed_data", "invalid checksum")).into_parts();
    assert_eq!(expected, result);
}

#[test]
fn test_validate_not_valid_format() {
    let seed_data: types::Password = "xprvblablabla".to_string().into();
    let seed_source = "invalid".to_string();
    let password: types::Password = "12345678".to_string().into();
    let result =
        match app::methods::validate(password, seed_data, seed_source, None, None, None, None) {
            Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
            Err(e) => e.into_parts(),
        };
    let expected = app::validation_error(app::field_error(
        "seed_source",
        "Seed source has to be mnemonics|xprv",
    ))
    .into_parts();
    assert_eq!(expected, result);
}

#[test]
fn test_not_valid_password_length() {
    let seed_data: types::Password =
        "day voice lake monkey suit bread occur own cattle visit object ordinary"
            .to_string()
            .into();
    let seed_source = "mnemonics".to_string();
    let password: types::Password = "11".to_string().into();
    let (name, description, overwrite, backup_password) = (None, None, None, None);
    let result = match app::methods::validate(
        password,
        seed_data,
        seed_source,
        name,
        description,
        overwrite,
        backup_password,
    ) {
        Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
        Err(e) => e.into_parts(),
    };
    let expected = app::validation_error(app::field_error(
        "password",
        "Password must be at least 8 characters",
    ))
    .into_parts();
    assert_eq!(expected, result);
}

#[test]
fn test_validate_xprv_no_password_inserted() {
    let seed_data: types::Password = "xprvblablabla".to_string().into();
    let backup_password = None;
    let result = match app::methods::validate_xprv(seed_data, backup_password) {
        Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
        Err(e) => e.into_parts(),
    };
    let expected = app::validation_error(app::field_error(
        "backup_password",
        "Backup password not found for XPRV key",
    ))
    .into_parts();

    assert_eq!(expected, result);
}

#[test]
fn test_invalid_bech32_key_inserted() {
    let seed_data: types::Password = "invalidblablabla".to_string().into();
    let backup_password = Some("12345678".to_string().into());
    let result = match app::methods::validate_xprv(seed_data, backup_password) {
        Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
        Err(e) => e.into_parts(),
    };
    let expected =
        app::validation_error(app::field_error("seed_data", "Could not decode bech32 key"))
            .into_parts();

    assert_eq!(expected, result);
}

#[test]
fn test_invalid_prefix() {
    let seed_data = "testkey".to_string();
    let encoded_data = bech32::encode("xprvxprv", seed_data.as_bytes().to_base32()).unwrap();
    let backup_password = Some("12345678".to_string().into());
    let result = match app::methods::validate_xprv(encoded_data.into(), backup_password) {
        Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
        Err(e) => e.into_parts(),
    };
    let expected = app::validation_error(app::field_error("seed_data", "Invalid seed data prefix"))
        .into_parts();

    assert_eq!(expected, result);
}

#[test]
fn test_invalid_decryption() {
    let seed_data: types::Password = "xprv1506zvl8u2r23zq8a3ayuzncwjawrx8etu5r2vqgz6r8ncdgt032w6ars0lc6jm47mj9tmcwg6wsg539992vhpqglamzqcpcq23h2ljvexsltv480utty2ma4lzmmuqy6zqfjkprnefr2kcu85lr006u9dmqxs84wx5lecr2c6lpwcg2atwvd60e295eqx245a9h7h72gt5r7gceg6avldxcejpt45ugl9cqe0aqzgsjpssmg23hxrglfu5vmu0f5my0xmqn5kmtq3m3wrgqkatf6uydwnnlp".to_string().into();
    let backup_password = Some("12345678".to_string().into());
    let result = match app::methods::validate_xprv(seed_data, backup_password) {
        Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
        Err(e) => e.into_parts(),
    };
    let expected =
        app::validation_error(app::field_error("seed_data", "Could not decrypt seed data"))
            .into_parts();

    assert_eq!(expected, result);
}

#[test]
fn test_valid_xprv_decryption() {
    let seed_data: types::Password = "xprv1506zvl8u2r23zq8a3ayuzncwjawrx8etu5r2vqgz6r8ncdgt032w6ars0lc6jm47mj9tmcwg6wsg539992vhpqglamzqcpcq23h2ljvexsltv480utty2ma4lzmmuqy6zqfjkprnefr2kcu85lr006u9dmqxs84wx5lecr2c6lpwcg2atwvd60e295eqx245a9h7h72gt5r7gceg6avldxcejpt45ugl9cqe0aqzgsjpssmg23hxrglfu5vmu0f5my0xmqn5kmtq3m3wrgqkatf6uydwnnlp".to_string().into();

    let expected_result = "xprv1qrpn0320zxwzwjuan7pyz72wq24hxktxgmznvgy0kr3fdup02fstgqzf4zz5afxg5du8xnwwcjzwv9eqa3n69e2f5nrq3k5caj0wax9hxvcv9cx0".to_string().into();
    let backup_password = Some("password".to_string().into());
    let result = app::methods::validate_xprv(seed_data, backup_password).unwrap();
    if let types::SeedSource::Xprv(key_string) = result {
        assert_eq!(key_string, expected_result)
    } else {
        panic!("Failed")
    }
}

#[test]
fn test_valid_xprv_double_decryption() {
    let seed_data: types::Password = "xprvdouble1ae5gfvwm339antauxg9zf7ads86ex6nj00syqghdsw5fmgu4lfx0s4zs6tt2txhznq8g47tzdhwh6pq2xmq2r92qed5cyykh2wesgzhldyzusksclcf6uq54jyzcm86f3p3nu5jmqm0vdhdf7hmac9ylkgjjlhs3fc3vd7tqsn53evszlseslxrztp00lg5vxrsj2l8caskv6xrs5gw8xnnlhzw5pq0j4yd0rvgw422fz6xeteru54n0lwfprmnwu6zl2e7nktarr6dh5n22ztk305veu4eegnxvr7a96dcrgm7cdqde2gmf2jgveppp77hpzkulx4af0kz2mawcmyxe97csqsjm5h2fx9aw8anfgsn3jp40h8gjy5ap5fgddfr808k7ldspf3xvxfkw8elx9rshhlwuyk29cmnsd3sazak27dndnumdwj9hp34kh7g86kgtarzcsr5dzl9".to_string().into();
    let expected_internal = "xprv1qrmya03vnt8sa4mlgnnwahv22hlx57gqvjxakyj96y9kn7exnrxhsqrzrjjkm5dd90e569azx4hvtkr9amf5ezvmq9ltkazy0um0t5pq8qm7ea7l".to_string().into();
    let expected_external = "xprv1qrzzrpdng7henalmkjl5r782r6d3hec0rfzs0a7tuchfnay84a90gqxsmjc2hfchjek9d9q45xssrqw5uyhzqrxsme89kvxnvtfsvdhdjgkv3zf6".to_string().into();
    let backup_password = Some("password".to_string().into());
    let result = app::methods::validate_xprv(seed_data, backup_password).unwrap();
    if let types::SeedSource::XprvDouble((internal, external)) = result {
        assert_eq!(internal, expected_internal);
        assert_eq!(external, expected_external);
    } else {
        panic!("Failed")
    }
}

#[test]
fn test_split_xprv_more_than_two_xprv_ocurrences() {
    let seed_data = "xprv1qrmya03vnt8sa4mlgnnwahv22hlx57gqvjxakyj96y9kn7exnrxhsqrzrjjkm5dd90e569azx4hvtkr9amf5ezvmq9ltkazy0um0t5pq8qm7ea7lxprv1qrzzrpdng7henalmkjl5r782r6d3hec0rfzs0a7tuchfnay84a90gqxsmjc2hfchjek9d9q45xssrqw5uyhzqrxsme89kvxnvtfsvdhdjgkv3zf6xprv".to_string();
    let result = match app::methods::split_xprv_double(seed_data) {
        Ok(_) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
        Err(e) => e.into_parts(),
    };
    let expected = app::validation_error(app::field_error(
        "seed_data",
        "Invalid number of XPRV keys found for xprvDouble type",
    ))
    .into_parts();

    assert_eq!(expected, result);
}
