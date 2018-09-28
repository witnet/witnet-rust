use log::info;

use std::io;
use std::io::{Read, Write};
use std::net;
use std::process::exit;

pub fn run(address: &str, callback: fn()) -> io::Result<()> {
    let listener = net::TcpListener::bind(address).expect("error run");
    info!("Witnet server listening on {}", address);
    callback();

    for stream in listener.incoming() {
        handle_connection(stream?).expect("Error handling connection")
    }
    Ok(())
}

pub fn close() {
    println!();
    info!("Closing server");
    exit(0);
}

fn handle_connection(mut stream: net::TcpStream) -> io::Result<()> {
    info!(
        "Incoming connection from: {}",
        stream
            .peer_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| String::from("unknown"))
    );
    let mut buf = [0; 512];
    loop {
        let bytes_read = stream.read(&mut buf)?;
        if bytes_read == 0 {
            return Ok(());
        }
        stream.write_all(&buf[..bytes_read])?;
    }
}
