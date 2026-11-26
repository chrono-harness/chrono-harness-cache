//! The release build uses the cache process's own plan, replay, error and journal.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;

verus! {
fn plan_matches(wanted: &String, version: &String, consumer: &String, operations: &[String], key: &String) -> (valid: bool)
    ensures valid == (version@ == wanted@ && consumer@ == "build"@ && operations@.len() == 1
        && operations@[0]@ == "build.cache"@ && key@ != ""@),
{
    *version == *wanted && *consumer == "build".to_string() && operations.len() == 1
        && operations[0] == "build.cache".to_string() && *key != "".to_string()
}

fn rejected_unknown(code: i32, error_size: u64) -> (valid: bool)
    ensures valid == (code == 2 && error_size > 0),
{ code == 2 && error_size > 0 }

fn receipt_matches(code: i64, joined: bool, pid: u64, launch_pid: u64, digest: &String, launch_digest: &String, expected: &String, observed: &String) -> (valid: bool)
    ensures valid == (code == 0 && joined && pid > 0 && pid == launch_pid
        && digest@ == launch_digest@ && expected@ == observed@),
{ code == 0 && joined && pid > 0 && pid == launch_pid && *digest == *launch_digest && *expected == *observed }

fn planned_event(producer: &String, event: &String) -> (valid: bool)
    ensures valid == (producer@ == "chrono-cache"@ && event@ == "cache.plan.prepared"@),
{ *producer == "chrono-cache".to_string() && *event == "cache.plan.prepared".to_string() }

fn same_replay(first: &[u8], replay: &[u8], inputs: usize) -> (valid: bool)
    ensures valid == (inputs == 2 && first@ == replay@),
{
    if inputs != 2 || first.len() != replay.len() { return false; }
    let mut i = 0;
    while i < first.len()
        invariant first.len() == replay.len(), i <= first.len(), forall|j: int| 0 <= j < i ==> first@[j] == replay@[j],
        decreases first.len() - i,
    {
        if first[i] != replay[i] { return false; }
        i += 1;
    }
    assert(first@ =~= replay@);
    true
}

fn expect_release(condition: bool)
    requires condition,
{
    if !condition { panic!("release observation differs"); }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn release_observer_rejects_different_inputs_and_processes() {
    broadcast use vstd::array::group_array_axioms;
    proof {
        reveal_strlit("build"); reveal_strlit("other"); reveal_strlit("build.cache");
        reveal_strlit("key"); reveal_strlit(""); reveal_strlit("chrono-cache"); reveal_strlit("cache.plan.prepared");
        assert("key"@.len() != ""@.len());
        assert("key"@.len() != "other"@.len());
        assert("other"@.len() != "chrono-cache"@.len());
        assert("build"@[0] != "other"@[0]);
    }
    let build = "build".to_string();
    let wrong = "other".to_string();
    let op = "build.cache".to_string();
    let ops = [op];
    let key = "key".to_string();
    expect_release(plan_matches(&key, &key, &build, &ops, &key));
    expect_release(!plan_matches(&key, &wrong, &build, &ops, &key));
    expect_release(!plan_matches(&key, &key, &wrong, &ops, &key));
    expect_release(!plan_matches(&key, &key, &build, &ops, &"".to_string()));
    expect_release(rejected_unknown(2, 1));
    expect_release(!rejected_unknown(0, 1));
    expect_release(!rejected_unknown(2, 0));
    expect_release(receipt_matches(0, true, 7, 7, &key, &key, &key, &key));
    expect_release(!receipt_matches(0, true, 7, 8, &key, &key, &key, &key));
    expect_release(!receipt_matches(0, false, 7, 7, &key, &key, &key, &key));
    expect_release(!receipt_matches(0, true, 7, 7, &key, &wrong, &key, &key));
    expect_release(!receipt_matches(0, true, 7, 7, &key, &key, &key, &wrong));
    expect_release(planned_event(&"chrono-cache".to_string(), &"cache.plan.prepared".to_string()));
    expect_release(!planned_event(&wrong, &"cache.plan.prepared".to_string()));
}

fn deployed_workflow() -> (source: &'static str) { include_str!("../../../.github/workflows/release.yml") }
}

#[cfg(not(verus_keep_ghost))]
mod native {
    use super::*;
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::{error::Error, fs, path::Path};

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    fn text(value: &Value) -> Result<String> {
        Ok(value
            .as_str()
            .ok_or("expected string in release observation")?
            .to_owned())
    }

    fn document(path: &Path) -> Result<Value> {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub(super) fn check(version: &String, host: &Path, code: i32) -> Result<()> {
        let plan = document(&host.join("plan.json"))?;
        let operations: Vec<String> =
            serde_json::from_value(plan["consumer_operations"]["operations"].clone())?;
        if !plan_matches(
            version,
            &text(&plan["planner"]["version"])?,
            &text(&plan["consumer"])?,
            &operations,
            &text(&plan["caches"]["cargo"]["key"])?,
        ) {
            return Err("release plan differs".into());
        }
        if !rejected_unknown(code, fs::metadata(host.join("error"))?.len()) {
            return Err("unknown consumer observation differs".into());
        }
        let first = fs::read(host.join("first.json"))?;
        let replay = fs::read(host.join("replay.json"))?;
        let parsed: Value = serde_json::from_slice(&first)?;
        let inputs = parsed["inputs"]
            .as_object()
            .ok_or("missing replay inputs")?;
        if !same_replay(&first, &replay, inputs.len()) {
            return Err("replayed output or input count differs".into());
        }
        for input in ["lock", "manifest"] {
            let original = &inputs.get(input).ok_or("missing input")?["original"];
            let path = host.join(text(&original["path"])?);
            let bytes = fs::read(&path)?;
            let receipt: Value = serde_json::from_slice(&bytes)?;
            let launch = document(
                &path
                    .parent()
                    .ok_or("missing receipt directory")?
                    .join("launch.json"),
            )?;
            if !receipt_matches(
                receipt["exit_code"].as_i64().ok_or("missing exit")?,
                receipt["owned_work_joined"]
                    .as_bool()
                    .ok_or("missing ownership")?,
                receipt["child_pid"].as_u64().ok_or("missing PID")?,
                launch["child_pid"].as_u64().ok_or("missing launch PID")?,
                &text(&receipt["sha256"])?,
                &text(&launch["sha256"])?,
                &text(&original["sha256"])?,
                &format!("{:x}", Sha256::digest(&bytes)),
            ) {
                return Err(format!("{input} process receipt differs").into());
            }
        }
        let mut found = false;
        for entry in fs::read_dir(host.join(".chrono-harness/logs"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                for event in
                    serde_json::Deserializer::from_slice(&fs::read(path)?).into_iter::<Value>()
                {
                    let event = event?;
                    if let (Some(p), Some(e)) =
                        (event["producer"].as_str(), event["event"].as_str())
                    {
                        found |= planned_event(&p.to_owned(), &e.to_owned());
                    }
                }
            }
        }
        if !found {
            return Err("missing prepared event".into());
        }
        Ok(())
    }

    #[test]
    fn release_runs_observer_before_packaging() {
        assert!(deployed_workflow().contains("--example release_observation -- \"${product_version%%-*}\" \"$host\" \"$code\"\n          mkdir \"$RUNNER_TEMP/dist\""));
    }
}

#[cfg(not(verus_keep_ghost))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let [_, version, host, code]: [String; 4] = std::env::args()
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| "expected version, host and unknown-consumer exit")?;
    native::check(&version, std::path::Path::new(&host), code.parse()?)
}
