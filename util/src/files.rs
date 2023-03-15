use std::{fs::File, path::Path};

/// Create a regular file.
pub fn create_file<P: AsRef<Path>>(path: P) -> Result<File, std::io::Error> {
    let file = File::create(path)?;

    Ok(file)
}

/// Create a file that will only be readable by the user that created it.
///
/// This is specially convenient for storing credentials, such as master keys.
#[cfg(unix)]
pub fn create_private_file<P: AsRef<Path>>(path: P) -> Result<File, std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let file = create_file(path)?;
    let mut perms = file.metadata()?.permissions();
    // -rw-------
    perms.set_mode(0o600);
    file.set_permissions(perms)?;

    Ok(file)
}

/// Create a file that will only be readable by the user that created it (not supported in this
/// architecture, will just create a regular file).
#[cfg(not(unix))]
pub fn create_private_file<P: AsRef<Path>>(path: P) -> Result<File, std::io::Error> {
    create_file(path)
}
