//! Cache input bytes, keys and paths in the host checkout.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn digest<T: Serialize>(input: &T) -> Result<String, String> {
    serde_jcs::to_vec(input)
        .map(|raw| sha256(&raw))
        .map_err(|e| e.to_string())
}

pub fn file_identity(path: &Path) -> Result<(String, u64), String> {
    if !fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .is_file()
    {
        return Err(format!("not a regular input file: {}", path.display()));
    }
    let mut input = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !input.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("not a regular input file: {}", path.display()));
    }
    let mut hash = Sha256::new();
    let size = io::copy(&mut input, &mut hash).map_err(|e| e.to_string())?;
    Ok((format!("{:x}", hash.finalize()), size))
}

pub fn relative_path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.starts_with('/')
        || value == "."
        || value.starts_with("./")
        || value.contains(['\0', '\n', '\r', '\\'])
        || value.split('/').any(|component| component == "..")
    {
        return Err(format!("invalid relative path: {value:?}"));
    }
    Ok(())
}

fn checked(root: &Path, relative: &str, publication: bool) -> Result<PathBuf, String> {
    relative_path(relative)?;
    let mut path = root.to_path_buf();
    let mut parts = Path::new(relative).components().peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_symlink() => {
                return Err(format!("symlink path is not allowed: {}", path.display()));
            }
            Ok(metadata) if publication && parts.peek().is_some() && !metadata.is_dir() => {
                return Err(format!(
                    "publication parent is not a directory: {}",
                    path.display()
                ));
            }
            Ok(metadata) if publication && parts.peek().is_none() && !metadata.is_file() => {
                return Err(format!(
                    "publication destination is not a file: {}",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}
pub fn no_symlink_parents(root: &Path, relative: &str) -> Result<PathBuf, String> {
    checked(root, relative, false)
}
pub fn prepare_publication(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let target = checked(root, relative, true)?;
    fs::create_dir_all(
        target
            .parent()
            .ok_or("publication destination has no parent")?,
    )
    .map_err(|e| e.to_string())?;
    no_symlink_parents(root, relative)
}
