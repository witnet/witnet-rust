pub mod server;
use env_logger::Builder;

fn main() {
    // Init app logger
    Builder::from_default_env()
        // Remove comments to sprint demo
        //.default_format_timestamp(false)
        //.default_format_module_path(false)
        .init();
    server::websockets_actix_poc();
}
