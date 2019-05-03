//! Platform-specific application paths.

use std::env;
use std::path::PathBuf;

/// Find a configuration from standard paths.
///
/// In GNU/Linux:
///     current directory | $XDG_CONFIG_HOME | /etc/witnet/witnet.toml
///
/// In MacOS:
///     current directory | $HOME/Library/Preferences/io.witnet/witnet.toml | /etc/witnet/witnet.toml
///
/// In Windows:
///     current directory | C:\Users\Alice\AppData\Roaming\witnet\witnet.toml
pub fn find_config() -> Option<PathBuf> {
    let mut config_dirs = Vec::with_capacity(3);

    if let Ok(dir) = env::current_dir() {
        config_dirs.push(dir);
    }

    if let Some(dir) = directories::ProjectDirs::from("io", "witnet", "witnet") {
        config_dirs.push(dir.config_dir().into());
    }

    if cfg!(unix) {
        config_dirs.push("/etc/witnet".into());
    }

    config_dirs
        .into_iter()
        .map(|path| path.join("witnet.toml"))
        .find(|path| path.exists())
}

/// Returns a platform-specific path for storing application data.
///
/// In GNU/Linux:
///     $XDG_CONFIG_HOME/witnet
///
/// In MacOS:
///     $HOME/Library/Preferences/witnet
///
/// In Windows:
///     current directory | C:\Users\Alice\AppData\Local\witnet
///
/// Defaults to current directory.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "witnet", "witnet")
        .map(|dir| dir.data_local_dir().into())
        .unwrap_or_else(|| env::current_dir().expect("Unable to store wallet data"))
}
