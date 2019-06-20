//! Wallet implementation for Witnet
//!
//! The way a client interacts with the Wallet is through a Websockets server. After running it you
//! can interact with it from the web-browser's javascript console:
//! ```js
//! var sock= (() => { let s = new WebSocket('ws://localhost:3030');s.addEventListener('message', (e) => {  console.log('Rcv =>', e.data) });return s; })();
//! sock.send('{"jsonrpc":"2.0","method":"getBlockChain","id":"1"}');
//! ```

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]
use actix::prelude::*;
use failure::Error;

use witnet_config::config::Config;

mod actors;
mod api;
mod signal;
mod storage;
mod wallet;

/// Run the Witnet wallet application.
pub fn run(conf: Config) -> Result<(), Error> {
    let system = System::new("witnet-wallet");
    let controller = actors::Controller::build()
        .server_addr(conf.wallet.server_addr)
        .db_path(conf.wallet.db_path)
        .node_url(conf.wallet.node_url)
        .start()?;

    signal::ctrl_c(move || {
        controller.do_send(actors::controller::Shutdown);
    });

    system.run()?;

    Ok(())
}
