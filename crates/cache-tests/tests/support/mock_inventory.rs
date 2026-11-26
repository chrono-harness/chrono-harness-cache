//! Observable replies of the test custodian. Only the runner verifies its real inventory and leases.
#![allow(unexpected_cfgs)]
use vstd::prelude::*;

verus! {
/// The mock custodian admits a member only when length and full digest agree.
pub fn member_identity<F: FnOnce() -> String>(actual_len: u64, expected_len: u64,
    expected: &str, hash: F) -> (ok: bool)
    requires actual_len == expected_len ==> hash.requires(()),
    ensures if actual_len != expected_len { !ok }
        else { exists|digest: String| hash.ensures((), digest) && ok == (digest@ == expected@) },
{
    if actual_len != expected_len { return false; }
    let digest = hash();
    let ok = <&str as vstd::contrib::exec_spec::ExecSpecEq>::exec_eq(digest.as_str(), expected);
    proof { assert(hash.ensures((), digest)); }
    ok
}
}

#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use sha2::{Digest, Sha256};
#[cfg(not(verus_keep_ghost))]
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

#[cfg(not(verus_keep_ghost))]
type Records = BTreeMap<String, Value>;
#[cfg(not(verus_keep_ghost))]
type TestResult<T> = Result<T, String>;
#[cfg(not(verus_keep_ghost))]
pub fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
#[cfg(not(verus_keep_ghost))]
pub fn member_bytes(root: &Path, path: &str, digest: &str, length: u64) -> bool {
    fs::read(root.join(path))
        .is_ok_and(|raw| member_identity(raw.len() as u64, length, digest, || hash(&raw)))
}
#[cfg(not(verus_keep_ghost))]
pub struct Inventory {
    pub root: PathBuf,
}
#[cfg(not(verus_keep_ghost))]
impl Inventory {
    pub fn adopted(root: &Path) -> TestResult<Option<Self>> {
        Ok(Some(Self {
            root: root.canonicalize().map_err(|e| e.to_string())?,
        }))
    }
    pub fn update<T>(&self, f: impl FnOnce(&mut Records) -> TestResult<T>) -> TestResult<T> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join("mock-custody.json"))
            .map_err(|e| e.to_string())?;
        file.lock().map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw).map_err(|e| e.to_string())?;
        let mut state = if raw.is_empty() {
            Records::new()
        } else {
            serde_json::from_slice(&raw).map_err(|e| e.to_string())?
        };
        let result = f(&mut state);
        file.rewind().map_err(|e| e.to_string())?;
        file.set_len(0).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &state).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        result
    }
    pub fn producer_status(&self, producer: &str, path: Option<&str>) -> TestResult<Value> {
        self.update(|records| Ok(json!({"records":records.iter().filter(|(p,r)| r["producer"]==producer && path.is_none_or(|x| *p==x)).map(|(_,r)|r).collect::<Vec<_>>()})))
    }
    pub fn rooted_original(
        &self,
        producer: &str,
        role: &str,
    ) -> TestResult<Option<(String, String, String, Value)>> {
        self.update(|records| {
            Ok(records
                .iter()
                .find(|(_, r)| r["producer"] == producer && r["role"] == role)
                .map(|(p, r)| {
                    (
                        r["id"].as_str().unwrap().into(),
                        p.clone(),
                        r["phase"].as_str().unwrap().into(),
                        r["outcome"].clone(),
                    )
                }))
        })
    }
    pub fn reference_paths(&self, role: &str, paths: &[String]) -> TestResult<()> {
        self.update(|records| {
            for (path, record) in records.iter_mut() {
                if paths.contains(path) {
                    record["role"] = json!(role);
                } else if record["role"] == role {
                    record["role"] = Value::Null;
                }
            }
            Ok(())
        })
    }
    pub fn published_tree_member(
        &self,
        producer: &str,
        path: &str,
        member: &str,
        digest: &str,
        length: u64,
    ) -> TestResult<bool> {
        self.update(|records| {
            Ok(records.get(path).is_some_and(|r| {
                r["producer"] == producer
                    && r["phase"] == "released"
                    && r["members"][member] == digest
                    && member_bytes(&self.root, member, digest, length)
            }))
        })
    }
    pub fn seal(&self, path: &str, outcome: &Value) -> TestResult<()> {
        fn members(
            root: &Path,
            path: &Path,
            files: &mut BTreeMap<String, String>,
        ) -> TestResult<()> {
            for item in fs::read_dir(path).map_err(|e| e.to_string())? {
                let p = item.map_err(|e| e.to_string())?.path();
                if p.is_dir() {
                    members(root, &p, files)?;
                } else {
                    files.insert(
                        p.strip_prefix(root).unwrap().to_str().unwrap().into(),
                        hash(&fs::read(p).map_err(|e| e.to_string())?),
                    );
                }
            }
            Ok(())
        }
        let mut files = BTreeMap::new();
        members(&self.root, &self.root.join(path), &mut files)?;
        let policy: Value = serde_json::from_slice(
            &fs::read(self.root.join(".chrono-harness/retention.json"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        self.update(|records| {
            let row = records.get_mut(path).ok_or("missing fixture publication")?;
            if files.len() >= policy["max_manifest_nodes"].as_u64().unwrap_or(u64::MAX) as usize {
                row["phase"] = json!("sealing");
                return Err("tree identity manifest capacity".into());
            }
            row["phase"] = json!("released");
            row["outcome"] = outcome.clone();
            row["members"] = json!(files);
            Ok(())
        })
    }
    pub fn maintain_originals(&self, _ids: &[String]) -> TestResult<Value> {
        self.maintain_originals_at(&[], 0)
    }
    pub fn maintain_at(&self, observed: u64) -> TestResult<Value> {
        self.maintain_originals_at(&[], observed)
    }
    pub fn maintain_originals_at(&self, ids: &[String], observed: u64) -> TestResult<Value> {
        self.update(|records|{
            let mut disposed=Vec::new();
            if observed==u64::MAX {
                let pinned=records.values().filter(|r|r["role"].is_string()).map(|r|r["id"].clone()).collect::<Vec<_>>();
                let selections=records.iter().filter(|(_,r)| r["role"].is_null() && r["phase"]=="released" && (ids.is_empty()||ids.iter().any(|id|r["id"]==*id)) &&
                         r["dependencies"].as_array().is_none_or(|deps|!deps.iter().any(|id|pinned.contains(id)))).map(|(p,_)|p.clone()).collect::<Vec<_>>();
                for p in selections {
                    let path=self.root.join(&p);
                    if path.is_dir(){fs::remove_dir_all(path)}else{fs::remove_file(path)}.map_err(|e|e.to_string())?;
                    disposed.push(records.remove(&p).unwrap());
                }
            }
            Ok(json!({"disposed":disposed,"accounting_errors":[],"protected":[],"remaining_outputs":records.len()}))
        })
    }
}
