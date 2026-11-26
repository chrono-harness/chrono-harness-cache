//! Cache input bytes, keys and paths in the host checkout.
use crate::verified_planner::{
    ComponentError, accepts_relative, admit_component, publication_target,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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
    let mut hash = digest_io::IoWrapper(Sha256::new());
    let size = io::copy(&mut input, &mut hash).map_err(|e| e.to_string())?;
    Ok((hex::encode(hash.0.finalize()), size))
}

pub fn relative_path(value: &str) -> Result<(), String> {
    if !accepts_relative(value.as_bytes()) {
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
        let observation = fs::symlink_metadata(&path)
            .map(|meta| (meta.is_symlink(), meta.is_dir(), meta.is_file()))
            .map_err(|error| (error.kind() as u32, error));
        admit_component(
            observation,
            io::ErrorKind::NotFound as u32,
            publication,
            parts.peek().is_some(),
        )
        .map_err(|error| match error {
            ComponentError::Link => format!("symlink path is not allowed: {}", path.display()),
            ComponentError::Parent => {
                format!("publication parent is not a directory: {}", path.display())
            }
            ComponentError::Destination => {
                format!("publication destination is not a file: {}", path.display())
            }
            ComponentError::Observation(original) => original.to_string(),
        })?;
    }
    Ok(path)
}
pub fn no_symlink_parents(root: &Path, relative: &str) -> Result<PathBuf, String> {
    checked(root, relative, false)
}
pub fn prepare_publication(root: &Path, relative: &str) -> Result<PathBuf, String> {
    publication_target(
        (),
        |(), publication| ((), checked(root, relative, publication)),
        |(), target| {
            let result = target
                .parent()
                .ok_or_else(|| "publication destination has no parent".to_string())
                .and_then(|parent| fs::create_dir_all(parent).map_err(|e| e.to_string()));
            ((), result)
        },
    )
    .1
}
