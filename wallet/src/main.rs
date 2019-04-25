use env_logger;
use witnet_wallet as wallet;

fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .default_format_timestamp(false)
        .default_format_module_path(false)
        .filter_level(log::LevelFilter::Info)
        .init();

    wallet::run()
}
