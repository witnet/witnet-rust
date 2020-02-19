use std::{fs::File, path::Path};

/// Create a file to store credentials securely: it will only be readable by
/// the user that created it
#[cfg(unix)]
pub fn create_credentials_file<P: AsRef<Path>>(path: P) -> Result<File, std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let file = File::create(path)?;
    let mut perms = file.metadata()?.permissions();
    // -rw-------
    perms.set_mode(0o600);
    file.set_permissions(perms)?;

    Ok(file)
}

/// Create a file to store credentials securely (not supported in this architecture, will just
/// create a normal file)
#[cfg(not(unix))]
pub fn create_credentials_file<P: AsRef<Path>>(path: P) -> Result<File, std::io::Error> {
    let file = File::create(path)?;

    Ok(file)
}
