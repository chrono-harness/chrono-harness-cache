//! Cache input bytes, keys and paths in the host checkout.
#![allow(unexpected_cfgs)]
use vstd::prelude::*;

verus! {
/// Rust paths on the supported Unix hosts admit internal empty and dot components.
pub open spec fn invalid_relative_byte(raw: Seq<u8>, i: int) -> bool
    recommends 0 <= i < raw.len(),
{
    raw[i] == 0 || raw[i] == 10 || raw[i] == 13 || raw[i] == 92
    || (raw[i] == 46 && (i == 0 || raw[i - 1] == 47)
        && i + 1 < raw.len() && raw[i + 1] == 46
        && (i + 2 == raw.len() || raw[i + 2] == 47))
}

pub open spec fn host_relative(raw: Seq<u8>) -> bool {
    raw.len() > 0 && raw[0] != 47
    && !(raw[0] == 46 && (raw.len() == 1 || raw[1] == 47))
    && forall|i:int| 0 <= i < raw.len() ==> !invalid_relative_byte(raw, i)
}

pub fn accepts_relative(raw: &[u8]) -> (accepted: bool)
    ensures accepted == host_relative(raw@),
{
    if raw.len() == 0 || raw[0] == 47
        || (raw[0] == 46 && (raw.len() == 1 || raw[1] == 47))
    { return false; }

    let mut i = 0;
    while i < raw.len()
        invariant i <= raw.len(),
            forall|j:int| 0 <= j < i ==> !invalid_relative_byte(raw@, j),
        decreases raw.len() - i,
    {
        let byte = raw[i];
        if byte == 0 || byte == 10 || byte == 13 || byte == 92
            || (byte == 46 && (i == 0 || raw[i-1] == 47)
                && raw.len() - i >= 2 && raw[i+1] == 46
                && (raw.len() - i == 2 || raw[i+2] == 47))
        {
            assert(invalid_relative_byte(raw@, i as int));
            return false;
        }
        i += 1;
    }
    true
}
}

#[cfg(not(verus_keep_ghost))]
use crate::verified_planner::{ComponentError, admit_component, publication_target};
#[cfg(not(verus_keep_ghost))]
use serde::Serialize;
#[cfg(not(verus_keep_ghost))]
use sha2::{Digest, Sha256};
#[cfg(not(verus_keep_ghost))]
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(not(verus_keep_ghost))]
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(not(verus_keep_ghost))]
pub fn digest<T: Serialize>(input: &T) -> Result<String, String> {
    serde_jcs::to_vec(input)
        .map(|raw| sha256(&raw))
        .map_err(|e| e.to_string())
}

#[cfg(not(verus_keep_ghost))]
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

#[cfg(not(verus_keep_ghost))]
pub fn relative_path(value: &str) -> Result<(), String> {
    if !accepts_relative(value.as_bytes()) {
        return Err(format!("invalid relative path: {value:?}"));
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
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
#[cfg(not(verus_keep_ghost))]
pub fn no_symlink_parents(root: &Path, relative: &str) -> Result<PathBuf, String> {
    checked(root, relative, false)
}
#[cfg(not(verus_keep_ghost))]
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
