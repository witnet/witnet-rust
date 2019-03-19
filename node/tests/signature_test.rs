use actix;
use futures::Future;

use witnet_node::signature_mngr;
use witnet_wallet::key::SK;

fn ignore<T>(_: T) {}

#[test]
fn test_sign_without_key() {
    actix::System::run(|| {
        signature_mngr::start();

        let data = [0 as u8; 32];

        let fut = signature_mngr::sign(&data)
            .and_then(|_| Ok(()))
            .then(|result| {
                assert!(result.is_err());

                actix::System::current().stop();
                futures::future::result(result)
            });

        actix::Arbiter::spawn(fut.map_err(ignore));
    });
}

#[test]
fn test_sign_with_key() {
    actix::System::run(|| {
        signature_mngr::start();

        let key = SK::from_slice(&[1 as u8; 32]).unwrap();

        let fut = signature_mngr::set_key(key)
            .and_then(|_| {
                let data = [0 as u8; 32];
                signature_mngr::sign(&data)
            })
            .and_then(|signature| {
                assert_eq!(144, signature.to_string().len());
                Ok(())
            })
            .then(|r| {
                actix::System::current().stop();
                futures::future::result(r)
            });

        actix::Arbiter::spawn(fut.map_err(ignore));
    });
}
