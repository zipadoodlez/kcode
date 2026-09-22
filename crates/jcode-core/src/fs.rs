use std::path::Path;

/// Set file permissions to owner-only read/write (0o600).
pub fn set_permissions_owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
}

/// Set directory permissions to owner-only read/write/execute (0o700).
pub fn set_directory_permissions_owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, perms)
}
