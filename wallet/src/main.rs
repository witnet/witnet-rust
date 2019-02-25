pub mod server;

fn main() {
    println!("Witnet wallet");
    server::websockets_actix_poc();
}
