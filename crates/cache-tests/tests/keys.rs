#![allow(unexpected_cfgs)]

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;

verus! {

pub open spec fn named_probe_result<S, I, C, R>(source: S, inspect: I, clear: C,
    reject: R, out: (S, Result<(), String>)) -> bool
    where I: FnOnce(S) -> (S, Result<(bool, bool), String>), C: FnOnce(S) -> S,
        R: FnOnce(S) -> (S, bool),
{
    exists|observed: S, result: Result<(bool, bool), String>|
        #[trigger] inspect.ensures((source,), (observed, result))
        && match result {
            Err(error) => out == (observed, Err::<(), String>(error)),
            Ok((absolute, identity)) => if !absolute {
                out.0 == observed && match out.1 {
                    Err(error) => error@ == "named probe path is not absolute"@, _ => false,
                }
            } else if !identity {
                out.0 == observed && match out.1 {
                    Err(error) => error@ == "named probe executable differs"@, _ => false,
                }
            } else { exists|cleared: S, rejected: bool|
                clear.ensures((observed,), cleared)
                && #[trigger] reject.ensures((cleared,), (out.0, rejected))
                && if rejected { out.1 == Ok::<(), String>(()) } else { match out.1 {
                    Err(error) => error@ == "missing declared PATH fell back to ambient tools"@,
                    _ => false,
                } }
            },
        }
}

/// 先核实际绑定，再清除声明 PATH 并要求拒绝；前置失败不继续。
pub fn check_named_probe<S, I, C, R>(source: S, inspect: I, clear: C, reject: R)
    -> (out: (S, Result<(), String>))
    where I: FnOnce(S) -> (S, Result<(bool, bool), String>), C: FnOnce(S) -> S,
        R: FnOnce(S) -> (S, bool),
    requires inspect.requires((source,)), forall|state: S| clear.requires((state,)),
        forall|state: S| reject.requires((state,)),
    ensures named_probe_result(source, inspect, clear, reject, out),
{
    let (state, result) = inspect(source);
    let ghost observed = state;
    let ghost original = result;
    assert(inspect.ensures((source,), (observed, original)));
    let (absolute, identity) = match result {
        Err(error) => return (state, Err(error)),
        Ok(value) => value,
    };
    assert(original == Ok::<(bool, bool), String>((absolute, identity)));
    if !absolute { return (state, Err("named probe path is not absolute".to_string())); }
    if !identity { return (state, Err("named probe executable differs".to_string())); }
    let state = clear(state);
    let ghost cleared = state;
    let (state, rejected) = reject(state);
    assert(clear.ensures((observed,), cleared));
    assert(reject.ensures((cleared,), (state, rejected)));
    if rejected {
        let out = (state, Ok(()));
        assert(inspect.ensures((source,), (observed, original)));
        assert(clear.ensures((observed,), cleared));
        assert(reject.ensures((cleared,), (out.0, true)));
        assert(named_probe_result(source, inspect, clear, reject, out));
        out
    } else {
        let error = "missing declared PATH fell back to ambient tools".to_string();
        let out = (state, Err(error));
        assert(inspect.ensures((source,), (observed, original)));
        assert(clear.ensures((observed,), cleared));
        assert(reject.ensures((cleared,), (out.0, false)));
        assert(named_probe_result(source, inspect, clear, reject, out));
        out
    }
}

fn named_probe_expect(actual: bool)
    requires actual,
{
    if !actual { panic!(); }
}

fn named_probe_case(failed: bool, absolute: bool, identity: bool, rejected: bool) {
    let inspect = |mut trace: Vec<u8>| -> (out: (Vec<u8>, Result<(bool, bool), String>))
        ensures out.0@ == trace@.push(0),
            if failed { match out.1 { Err(error) => error@ == "original/λ\0\n"@, _ => false } }
            else { out.1 == Ok::<(bool, bool), String>((absolute, identity)) },
    {
        trace.push(0);
        (trace, if failed { Err("original/λ\0\n".to_string()) } else { Ok((absolute, identity)) })
    };
    let clear = |mut trace: Vec<u8>| -> (out: Vec<u8>)
        ensures out@ == trace@.push(1),
    { trace.push(1); trace };
    let reject = |mut trace: Vec<u8>| -> (out: (Vec<u8>, bool))
        ensures out.0@ == trace@.push(2), out.1 == rejected,
    { trace.push(2); (trace, rejected) };
    let (trace, result) = check_named_probe(Vec::new(), inspect, clear, reject);
    if failed || !absolute || !identity {
        named_probe_expect(trace.len() == 1 && trace[0] == 0);
    } else {
        named_probe_expect(trace.len() == 3 && trace[0] == 0 && trace[1] == 1 && trace[2] == 2);
    }
    if !failed && absolute && identity && rejected { named_probe_expect(result.is_ok()); }
    else { match result {
        Ok(()) => named_probe_expect(false),
        Err(error) => {
            let expected = if failed { "original/λ\0\n" }
                else if !absolute { "named probe path is not absolute" }
                else if !identity { "named probe executable differs" }
                else { "missing declared PATH fell back to ambient tools" };
            named_probe_expect(error == expected.to_string());
        },
    } }
}

#[test]
fn named_probe_driver_preserves_binding_order_and_original_errors() {
    named_probe_case(false, true, true, true);
    named_probe_case(true, false, false, false);
    named_probe_case(false, false, true, true);
    named_probe_case(false, true, false, true);
    named_probe_case(false, true, true, false);
}

}

#[path = "continuity.rs"]
#[cfg(not(verus_keep_ghost))]
mod continuity;

#[path = "recovery.rs"]
#[cfg(not(verus_keep_ghost))]
mod recovery;

#[cfg(not(verus_keep_ghost))]
#[path = "support/cache_host.rs"]
mod cache_host;
#[cfg(not(verus_keep_ghost))]
use cache_host::{Host, fixture, register_artifacts, register_consumer_contract};

#[cfg(not(verus_keep_ghost))]
use chrono_cache::{Config, prepare};
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use sha2::{Digest, Sha256};
#[cfg(not(verus_keep_ghost))]
use std::{fs, path::Path, process::Command};

#[cfg(not(verus_keep_ghost))]
fn plan(root: &Path, value: &Value, consumer: &str) -> Result<Value, String> {
    let config: Config = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let result = prepare(root, &config, consumer);
    if let Err(error) = &result {
        let original = json!({"consumer":consumer,"config":value,"error":error});
        fs::write(
            root.join("failed-plan.json"),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
    }
    result
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn failed_test_retains_probe_originals_and_success_removes_its_host() {
    let (root, mut config) = fixture();
    let retained = root.path().to_path_buf();
    config["inputs"]["compiler"] = json!({"kind":"command","command":{
        "program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"),"args":["exit"],"env":{},
        "timeout_seconds":5,"output_limit_bytes":4096},"inherit":[],"result":"stdout"});
    let failure = std::panic::catch_unwind(move || {
        plan(root.path(), &config, "check.one").unwrap();
    });
    assert!(failure.is_err());
    let original: Value =
        serde_json::from_slice(&fs::read(retained.join("failed-plan.json")).unwrap()).unwrap();
    assert_eq!(original["consumer"], "check.one");
    let error = original["error"].as_str().unwrap();
    let (_, path) = error.split_once("; original ").unwrap();
    let process: Value = serde_json::from_slice(&fs::read(retained.join(path)).unwrap()).unwrap();
    assert_eq!(process["exit_code"], 7);
    assert_eq!(process["failure"], Value::Null);
    assert_eq!(process["stderr"], "original probe failure\n");
    fs::remove_dir_all(retained).unwrap();

    let successful = Host::new();
    let removed = successful.path().to_path_buf();
    drop(successful);
    assert!(!removed.exists());
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn only_current_cache_registration_is_admitted() {
    let (root, mut config) = fixture();
    let current = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(current["schema"], "chrono-cache-plan/v2");
    assert_eq!(
        current["caches"]["project"]["key"],
        "chrono-v2-ef3e935698c7b7325b4b7e209aa4073f6f4a9ad0f07ff015ca000c9e0780e7c1-2817b202120fe2d9d5434847a76c712e5a06002285d5d0d51d404cfc72ea60e7"
    );
    // A retired registration must fail even without the current contract map.
    config
        .as_object_mut()
        .unwrap()
        .remove("consumer_operations");
    for schema in ["chrono-cache/v1", "chrono-cache/v3", ""] {
        config["schema"] = json!(schema);
        let config: Config = serde_json::from_value(config.clone()).unwrap();
        assert!(
            chrono_cache::validate(&config)
                .unwrap_err()
                .contains("E_CACHE_CONFIG"),
            "{schema}"
        );
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn source_changes_reuse_compatible_domain_and_compiler_changes_do_not() {
    let (root, config) = fixture();
    let first = plan(root.path(), &config, "check.one").unwrap();
    fs::write(root.path().join("unregistered"), b"unrelated").unwrap();
    assert_eq!(first, plan(root.path(), &config, "check.one").unwrap());
    fs::write(root.path().join("source"), b"source-two").unwrap();
    let source = plan(root.path(), &config, "check.one").unwrap();
    assert_ne!(
        first["caches"]["project"]["key"],
        source["caches"]["project"]["key"]
    );
    assert_eq!(
        first["caches"]["project"]["restore_keys"],
        source["caches"]["project"]["restore_keys"]
    );
    fs::write(root.path().join("compiler"), b"compiler-two").unwrap();
    let compiler = plan(root.path(), &config, "check.one").unwrap();
    assert_ne!(
        source["caches"]["project"]["restore_keys"],
        compiler["caches"]["project"]["restore_keys"]
    );
    let prefix = compiler["caches"]["project"]["restore_keys"][0]
        .as_str()
        .unwrap();
    assert!(
        compiler["caches"]["project"]["key"]
            .as_str()
            .unwrap()
            .starts_with(prefix)
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn consumers_select_only_registered_inputs_and_never_fall_back_to_all() {
    let (root, mut config) = fixture();
    let first = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(first["caches"].as_object().unwrap().len(), 1);
    assert_eq!(first["inputs"].as_object().unwrap().len(), 3);
    assert_eq!(
        first["caches"],
        plan(root.path(), &config, "check.two").unwrap()["caches"]
    );
    assert!(
        plan(root.path(), &config, "missing")
            .unwrap_err()
            .contains("E_CACHE_CONSUMER")
    );
    assert!(
        plan(root.path(), &config, "check.other")
            .unwrap_err()
            .contains("unavailable-other-input")
    );
    config["consumer_operations"]["check.other"][0]["registry"] =
        json!(".chrono-harness/missing-other.json");
    config["caches"]["other"]["producer"] = json!("build.changed-other");
    assert_eq!(first, plan(root.path(), &config, "check.one").unwrap());
    assert_eq!(first["execution"], "not-started");
    assert_eq!(first["input_completeness_proven"], false);
    assert_eq!(first["caches"]["project"]["status"], "prepared-unrestored");
    assert_eq!(
        first["caches"]["project"]["handling"],
        "registered-build-required"
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn adding_a_consumer_does_not_invalidate_an_unchanged_producer_cache() {
    let (root, mut config) = fixture();
    let first = plan(root.path(), &config, "check.one").unwrap();
    config["caches"]["project"]["consumers"] = json!(["check.one", "check.two", "check.new"]);
    config["consumer_operations"]["check.new"] = config["consumer_operations"]["check.one"].clone();
    assert_eq!(
        first["caches"],
        plan(root.path(), &config, "check.one").unwrap()["caches"]
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn adopted_consumer_contract_rejects_a_retired_producer_before_input_probes() {
    let (root, mut config) = fixture();
    let source = root.path().join(".chrono-harness/operations.json");
    fs::write(
        &source,
        br#"{"build":["build.project"],"other":"build.other"}"#,
    )
    .unwrap();
    config["consumer_operations"] = json!({
        "check.one":[{"registry":".chrono-harness/operations.json","pointer":"/build"}],
        "check.two":[{"registry":".chrono-harness/operations.json","pointer":"/build"}],
        "check.other":[{"registry":".chrono-harness/missing-other.json","pointer":"/other"}]
    });
    let first = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(first["consumer_operations"]["status"], "declared");
    assert_eq!(
        first["consumer_operations"]["operations"],
        json!(["build.project"])
    );
    assert_eq!(
        first["consumer_operations"]["sources"][0]["operations"],
        json!(["build.project"])
    );
    // A retained cache subscription must fail as soon as the actual registered
    // plan no longer runs its producer; unavailable compiler inputs are later.
    fs::write(&source, br#"{"build":["build.replacement"]}"#).unwrap();
    fs::remove_file(root.path().join("compiler")).unwrap();
    let error = plan(root.path(), &config, "check.one").unwrap_err();
    assert!(error.contains("E_CACHE_CONSUMER"), "{error}");
    assert!(error.contains("build.project"), "{error}");
    assert!(error.contains("check.one"), "{error}");
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn consumer_operation_references_reject_missing_ambiguous_and_nonoperation_values() {
    let (root, mut config) = fixture();
    config["consumer_operations"] = json!({
        "check.one":[{"registry":".chrono-harness/operations.json","pointer":"/build"}],
        "check.two":[{"registry":".chrono-harness/operations.json","pointer":"/build"}],
        "check.other":[{"registry":".chrono-harness/operations.json","pointer":"/other"}]
    });
    let source = root.path().join(".chrono-harness/operations.json");
    for value in [
        json!(null),
        json!([]),
        json!(["build.project", "build.project"]),
        json!({"operation":"build.project"}),
        json!(["build.project", 1]),
    ] {
        fs::write(
            &source,
            serde_json::to_vec(&json!({"build":value})).unwrap(),
        )
        .unwrap();
        let error = plan(root.path(), &config, "check.one").unwrap_err();
        assert!(error.contains("E_CACHE_CONSUMER"), "{value}: {error}");
    }
    fs::write(&source, br#"{"build":"build.project"}"#).unwrap();
    assert!(plan(root.path(), &config, "check.one").is_ok());
    config["consumer_operations"]
        .as_object_mut()
        .unwrap()
        .remove("check.two");
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("check.two")
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn a_consumer_registry_cannot_be_restored_as_a_cache_artifact() {
    let (root, mut config) = fixture();
    config["artifacts"]["target"]["path"] = json!(".chrono-harness/operations.json");
    register_artifacts(root.path(), &config);
    let error = plan(root.path(), &config, "check.one").unwrap_err();
    assert!(
        error.contains("E_CACHE_PATH") && error.contains("consumer registry"),
        "{error}"
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn plan_retains_probe_original_without_creating_release_output() {
    let (root, mut config) = fixture();
    config["inputs"]["compiler"] = json!({"kind":"command","command":{
        "program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"),"args":["echo","actual compiler"],"env":{},
        "timeout_seconds":5,"output_limit_bytes":4096},"inherit":[],"result":"stdout"});
    fs::write(
        root.path().join(".chrono-harness/cache.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let binary = Path::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"));
    let directory = ".chrono-harness/cache/release-results/unit/cache-observations/";
    let plan_path = ".chrono-harness/state/cache/check.one/plan.json".to_owned();
    let run = Command::new(&binary)
        .args([
            "plan",
            "--host-root",
            root.path().to_str().unwrap(),
            "--config",
            ".chrono-harness/cache.json",
            "--consumer",
            "check.one",
            "--plan-output",
            &plan_path,
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let prepared: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        prepared,
        serde_json::from_slice::<Value>(&fs::read(root.path().join(&plan_path)).unwrap()).unwrap()
    );
    let original = prepared["inputs"]["compiler"]["original"]["path"]
        .as_str()
        .unwrap();
    assert!(original.starts_with(".chrono-harness/state/cache-probes/plan-"));
    assert!(
        !root.path().join(directory).exists(),
        "preparation must not create release output"
    );
    let probe: Value =
        serde_json::from_slice(&fs::read(root.path().join(original)).unwrap()).unwrap();
    assert_eq!(probe["exit_code"], 0);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn compatibility_binds_profile_producer_artifacts_owner_and_namespace() {
    let (root, config) = fixture();
    fs::write(
        root.path().join(".chrono-harness/operations.json"),
        br#"{"build":["build.project","changed"],"other":"build.other"}"#,
    )
    .unwrap();
    let first = plan(root.path(), &config, "check.one").unwrap();
    for pointer in [
        "/namespace",
        "/inputs/profile/value/profile",
        "/caches/project/producer",
        "/artifacts/target/path",
    ] {
        let mut changed = config.clone();
        *changed.pointer_mut(pointer).unwrap() = json!("changed");
        register_artifacts(root.path(), &changed);
        let next = plan(root.path(), &changed, "check.one").unwrap();
        assert_ne!(
            first["caches"]["project"]["restore_keys"], next["caches"]["project"]["restore_keys"],
            "{pointer}"
        );
    }
    let mut changed = config.clone();
    changed["caches"]["project"]["owner"] = json!("changed-owner");
    changed["artifacts"]["target"]["owner"] = json!("changed-owner");
    register_artifacts(root.path(), &changed);
    assert_ne!(
        first["caches"],
        plan(root.path(), &changed, "check.one").unwrap()["caches"]
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn absent_inputs_are_explicit_and_empty_files_are_present() {
    let (root, mut config) = fixture();
    fs::remove_file(root.path().join("source")).unwrap();
    assert!(plan(root.path(), &config, "check.one").is_err());
    config["inputs"]["source"]["presence"] = json!("absent");
    let absent = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(
        absent["inputs"]["source"]["observation"]["presence"],
        "absent"
    );
    fs::write(root.path().join("source"), b"").unwrap();
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("expected absent")
    );
    config["inputs"]["source"]["presence"] = json!("present");
    let present = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(present["inputs"]["source"]["observation"]["length"], 0);
    assert_ne!(absent["caches"], present["caches"]);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn exact_keys_still_require_builds_and_artifact_fields_are_closed() {
    let (root, mut config) = fixture();
    config["caches"]["project"]["restore"] = json!("exact");
    let result = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(result["caches"]["project"]["restore_keys"], json!([]));
    assert_eq!(
        result["caches"]["project"]["handling"],
        "registered-build-required"
    );
    for (field, value) in [
        ("kind", json!("compilation")),
        ("kind", json!("dependencies")),
        ("kind", json!("executable-candidate")),
        ("external", json!(false)),
        ("external", json!(true)),
    ] {
        let mut changed = config.clone();
        changed["artifacts"]["target"][field] = value;
        assert!(plan(root.path(), &changed, "check.one").is_err(), "{field}");
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn registration_rejects_missing_ambiguous_and_cross_owner_references() {
    let (root, config) = fixture();
    for (pointer, value) in [
        ("/caches/project/source_inputs", json!(["unknown"])),
        ("/caches/project/source_inputs", json!(["source", "source"])),
        ("/caches/project/source_inputs", json!(["compiler"])),
        ("/caches/project/artifacts", json!(["missing"])),
        ("/caches/project/artifacts", json!(["other-target"])),
        (
            "/caches/project/consumers",
            json!(["check.one", "check.one"]),
        ),
        ("/caches/project/compatibility_inputs", json!([])),
    ] {
        let mut changed = config.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        if pointer == "/caches/project/consumers" {
            changed["consumer_operations"]
                .as_object_mut()
                .unwrap()
                .remove("check.two");
        }
        assert!(
            plan(root.path(), &changed, "check.one")
                .unwrap_err()
                .contains("E_CACHE_CONFIG"),
            "{pointer}"
        );
    }
    let mut unknown = config.clone();
    unknown["infer_dependencies"] = json!(true);
    assert!(serde_json::from_value::<Config>(unknown).is_err());
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn cache_paths_exclude_evidence_metadata_sources_and_overlapping_outputs() {
    let (root, config) = fixture();
    for path in [
        ".git",
        ".git/objects",
        ".chrono-harness",
        ".chrono-harness/state",
        ".chrono-harness/state/check.json",
        "source",
        "out/../source",
        "out/*",
    ] {
        let mut changed = config.clone();
        changed["artifacts"]["target"]["path"] = json!(path);
        register_artifacts(root.path(), &changed);
        assert!(
            plan(root.path(), &changed, "check.one")
                .unwrap_err()
                .contains("E_CACHE_PATH"),
            "{path}"
        );
    }
    let mut changed = config.clone();
    changed["caches"]["project"]["artifacts"] = json!(["target", "overlap"]);
    changed["artifacts"]["overlap"] = json!({"owner":"project","path":"out/project/nested"});
    register_artifacts(root.path(), &changed);
    assert!(
        plan(root.path(), &changed, "check.one")
            .unwrap_err()
            .contains("overlap")
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn cache_artifacts_require_host_paths_and_exact_existing_registration() {
    let (root, mut config) = fixture();
    let catalog = root.path().join(".chrono-harness/artifacts.json");
    for rows in [
        json!([]),
        json!([{"path":"out/project","owner":"project","tracked":true}]),
        json!([{"path":"out/project","owner":"project","tracked":false,"kind":"execution-evidence"}]),
        json!([{"path":"out/project","owner":"wrong-owner","tracked":false}]),
        json!([{"path":"out/project","owner":"project","tracked":false}, {"path":"out/project","owner":"project","tracked":false}]),
    ] {
        fs::write(
            &catalog,
            serde_json::to_vec(&json!({"artifacts":rows})).unwrap(),
        )
        .unwrap();
        assert!(
            plan(root.path(), &config, "check.one")
                .unwrap_err()
                .contains("E_CACHE_ARTIFACT")
        );
    }
    register_artifacts(root.path(), &config);
    let external = tempfile::tempdir().unwrap();
    let external_path = fs::canonicalize(external.path()).unwrap();
    let host_path = fs::canonicalize(root.path()).unwrap();
    for path in [
        external_path.to_str().unwrap(),
        host_path.to_str().unwrap(),
        "/",
        "",
        ".",
        "./out/project",
        "../out/project",
        "out/../project",
    ] {
        config["artifacts"]["target"]["path"] = json!(path);
        assert!(
            plan(root.path(), &config, "check.one")
                .unwrap_err()
                .contains("E_CACHE_PATH"),
            "{path}"
        );
    }
}

#[cfg(unix)]
#[test]
#[cfg(not(verus_keep_ghost))]
fn file_and_cache_aliases_do_not_silently_expand_registered_paths() {
    let (root, config) = fixture();
    fs::rename(root.path().join("source"), root.path().join("real-source")).unwrap();
    std::os::unix::fs::symlink("real-source", root.path().join("source")).unwrap();
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("symlink")
    );
    fs::remove_file(root.path().join("source")).unwrap();
    fs::rename(root.path().join("real-source"), root.path().join("source")).unwrap();
    std::os::unix::fs::symlink(".chrono-harness", root.path().join("out")).unwrap();
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("symlink")
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn cli_observes_only_explicit_child_environment_without_exposing_values() {
    let (root, mut config) = fixture();
    config["inputs"]["profile"] =
        json!({"kind":"environment","name":"CHRONO_CACHE_TEST_PROFILE","presence":"present"});
    let path = root.path().join(".chrono-harness/cache.json");
    fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    let binary = Path::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"));
    let run = |value: Option<&str>| {
        let mut command = Command::new(&binary);
        command.args([
            "plan",
            "--host-root",
            root.path().to_str().unwrap(),
            "--config",
            ".chrono-harness/cache.json",
            "--consumer",
            "check.one",
        ]);
        command.env_remove("CHRONO_CACHE_TEST_PROFILE");
        if let Some(value) = value {
            command.env("CHRONO_CACHE_TEST_PROFILE", value);
        }
        command.output().unwrap()
    };
    let missing = run(None);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("E_CACHE_INPUT"));
    let first = run(Some("first-private-value"));
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(!String::from_utf8_lossy(&first.stdout).contains("first-private-value"));
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["schema"], "chrono-cache-plan/v2");
    assert_eq!(first["consumer"], "check.one");
    assert_eq!(first["execution"], "not-started");
    let next = run(Some("second-private-value"));
    assert!(next.status.success());
    let next: Value = serde_json::from_slice(&next.stdout).unwrap();
    assert_ne!(
        first["caches"]["project"]["restore_keys"],
        next["caches"]["project"]["restore_keys"]
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn restore_policy_is_bound_without_consumer_list_order() {
    let (root, mut config) = fixture();
    let first = plan(root.path(), &config, "check.one").unwrap();
    config["caches"]["project"]["consumers"] = json!(["check.two", "check.one"]);
    assert_eq!(first, plan(root.path(), &config, "check.one").unwrap());
    config["caches"]["project"]["restore"] = json!("exact");
    let exact = plan(root.path(), &config, "check.one").unwrap();
    assert_ne!(
        first["caches"]["project"]["key"],
        exact["caches"]["project"]["key"]
    );
    assert_eq!(exact["caches"]["project"]["restore_keys"], json!([]));
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn cli_rejects_unknown_actions_and_ambiguous_options() {
    for args in [
        vec![],
        vec!["unknown"],
        vec!["plan", "--unknown", "x"],
        vec!["plan", "--host-root", ".", "--host-root", "."],
        vec!["plan", "--consumer"],
    ] {
        let args = args.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert!(chrono_cache::dispatch(&args).is_err());
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn github_outputs_preserve_selected_paths_and_never_publish_a_failed_plan() {
    let (root, mut config) = fixture();
    config["caches"]["other"]["consumers"] = json!(["check.one"]);
    config["consumer_operations"]
        .as_object_mut()
        .unwrap()
        .remove("check.other");
    fs::write(
        root.path().join(".chrono-harness/operations.json"),
        br#"{"build":["build.project","build.other"]}"#,
    )
    .unwrap();
    fs::write(root.path().join("unavailable-other-input"), b"other source").unwrap();
    config["artifacts"]["target"]["path"] = json!("outputs with spaces/target");
    register_artifacts(root.path(), &config);
    fs::write(
        root.path().join(".chrono-harness/cache.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let output = root.path().join("github-output");
    fs::write(&output, "existing=value\n").unwrap();
    let binary = Path::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"));
    let invoke = |consumer: &str| {
        Command::new(&binary)
            .args([
                "plan",
                "--host-root",
                root.path().to_str().unwrap(),
                "--config",
                ".chrono-harness/cache.json",
                "--consumer",
                consumer,
                "--github-output",
                output.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let result = invoke("check.one");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let prepared: Value = serde_json::from_slice(&result.stdout).unwrap();
    let contents = fs::read_to_string(&output).unwrap();
    assert!(contents.starts_with("existing=value\n"));
    let mut values = std::collections::BTreeMap::new();
    let mut lines = contents.lines().skip(1);
    while let Some(header) = lines.next() {
        let (name, delimiter) = header.split_once("<<").unwrap();
        let mut value = Vec::new();
        loop {
            let line = lines.next().unwrap();
            if line == delimiter {
                break;
            }
            value.push(line);
        }
        assert!(values.insert(name, value.join("\n")).is_none());
    }
    assert_eq!(values.len(), 6);
    assert_eq!(values["cache_0_key"], prepared["caches"]["other"]["key"]);
    assert_eq!(values["cache_1_key"], prepared["caches"]["project"]["key"]);
    assert_eq!(values["cache_1_paths"], "outputs with spaces/target");
    assert_eq!(
        values["cache_1_restore"],
        prepared["caches"]["project"]["restore_keys"][0]
    );
    assert_eq!(prepared["execution"], "not-started");
    let failed = invoke("missing");
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("E_CACHE_CONSUMER"));
    assert_eq!(fs::read_to_string(&output).unwrap(), contents);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn rejected_file_probe_reports_its_original_and_lexical_failure() {
    for (mode, arg, expected) in [
        ("echo", "", "expected one absolute file path"),
        ("echo", "relative", "expected one absolute file path"),
        ("echo", "/first\n/second", "expected one absolute file path"),
        ("invalid-encoding", "", "path encoding"),
        ("embedded-nul", "", "expected one absolute file path"),
    ] {
        let (root, mut config) = fixture();
        config["inputs"]["compiler"] = json!({"kind":"command", "command":{
            "program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"), "args":[mode,arg],
            "env":{},"timeout_seconds":2,"output_limit_bytes":4096},"inherit":[],"result":"file-path"});
        let error = plan(root.path(), &config, "check.one").unwrap_err();
        assert!(error.contains(expected), "{error}");
        let original = error
            .split("original ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(
            root.path().join(original.trim_end_matches(';')).is_file(),
            "{error}"
        );
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn registered_probe_binds_actual_selected_compiler_bytes_with_stable_keys() {
    let (root, mut config) = fixture();
    let compiler = root.path().join("compiler");
    config["inputs"]["compiler"] = json!({"kind":"command", "command":{
        "program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"),
        "args":["file",compiler],"env":{},"timeout_seconds":2,"output_limit_bytes":4096},
        "inherit":[],"result":"file-path"});
    let first = plan(root.path(), &config, "check.one").unwrap();
    let repeated = plan(root.path(), &config, "check.one").unwrap();
    let mut stable = first.clone();
    let mut stable_repeated = repeated.clone();
    let mut originals = Vec::new();
    for report in [&mut stable, &mut stable_repeated] {
        let original = report["inputs"]["compiler"]
            .as_object_mut()
            .unwrap()
            .remove("original")
            .unwrap();
        let path = original["path"].as_str().unwrap();
        let bytes = fs::read(root.path().join(path)).unwrap();
        assert!(path.starts_with(".chrono-harness/state/cache-probes/attempt-"));
        assert!(path.ends_with("/compiler/process.json"));
        let process: Value = serde_json::from_slice(&bytes).unwrap();
        let observed = &report["inputs"]["compiler"]["observation"];
        assert_eq!(process["exit_code"], observed["exit_code"]);
        assert_eq!(process["sha256"], observed["executable_sha256"]);
        assert_eq!(process["stdout_sha256"], observed["stdout_sha256"]);
        assert_eq!(process["stderr_sha256"], observed["stderr_sha256"]);
        originals.push(process);
    }
    assert_ne!(originals[0]["child_pid"], originals[1]["child_pid"]);
    assert_eq!(
        stable, stable_repeated,
        "distinct invocation originals must not change observed inputs or cache keys"
    );
    fs::write(&compiler, b"compiler-two").unwrap();
    let next = plan(root.path(), &config, "check.one").unwrap();
    assert_ne!(
        first["caches"]["project"]["restore_keys"],
        next["caches"]["project"]["restore_keys"]
    );
    let observed = &first["inputs"]["compiler"]["observation"];
    assert_eq!(observed["exit_code"], 0);
    assert_eq!(observed["file"]["length"], b"compiler-one".len());
    assert!(observed["executable_sha256"].as_str().unwrap().len() == 64);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn named_probe_uses_only_its_declared_path_and_records_the_bound_executable() {
    let (root, mut config) = fixture();
    let tools = root.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let executable = tools.join("registered-probe");
    fs::hard_link(env!("CARGO_BIN_EXE_chrono-cache-test-probe"), &executable).unwrap();
    config["inputs"]["compiler"] = json!({"kind":"command", "command":{
        "program":"registered-probe", "args":["echo","actual compiler"],
        "env":{"PATH":tools},"timeout_seconds":2,"output_limit_bytes":4096},
        "inherit":[],"result":"stdout"});
    check_named_probe(
        config,
        |config| {
            let result = plan(root.path(), &config, "check.one").map(|prepared| {
                let observed = Path::new(
                    prepared["inputs"]["compiler"]["observation"]["executable"]
                        .as_str()
                        .unwrap(),
                );
                (
                    observed.is_absolute(),
                    fs::canonicalize(observed).unwrap() == fs::canonicalize(&executable).unwrap(),
                )
            });
            (config, result)
        },
        |mut config| {
            config["inputs"]["compiler"]["command"]["env"] = json!({});
            config
        },
        |config| {
            let rejected = plan(root.path(), &config, "check.one").is_err();
            (config, rejected)
        },
    )
    .1
    .unwrap();
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn failed_and_bounded_probes_retain_original_results_and_stop_planning() {
    for (mode, limit, timeout) in [("exit", 4096, 5), ("sleep", 4096, 1), ("flood", 64, 5)] {
        let (root, mut config) = fixture();
        config["inputs"]["compiler"] = json!({"kind":"command","command":{
            "program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"),"args":[mode],"env":{},
            "timeout_seconds":timeout,"output_limit_bytes":limit},"inherit":[],"result":"stdout"});
        let error = plan(root.path(), &config, "check.one").unwrap_err();
        assert!(error.contains("E_CACHE_PROBE"), "{error}");
        let files = fs::read_dir(root.path().join(".chrono-harness/state/cache-probes"))
            .unwrap()
            .map(|v| v.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        let evidence: Value =
            serde_json::from_slice(&fs::read(files[0].join("compiler/process.json")).unwrap())
                .unwrap();
        assert!(error.contains(files[0].file_name().unwrap().to_str().unwrap()));
        if mode == "exit" {
            assert_eq!(evidence["exit_code"], 7, "{evidence}");
            assert_eq!(evidence["stderr"], "original probe failure\n");
        } else {
            let expected = if mode == "sleep" {
                "process timed out"
            } else {
                "process output limit exceeded"
            };
            assert_eq!(evidence["failure"], expected, "{evidence}");
        }
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn registered_source_keys_follow_only_explicit_transitive_filemap_edges() {
    let (root, mut config) = fixture();
    config["inputs"]["source"] = json!({"kind":"registered-files","registry":".chrono-harness/FILEMAP.json","node":"project:product","edges":["compile"]});
    let mut registry = json!({"schema_version":2,"project_edges":[
        {"from":"project:dependency","to":"project:product","kind":"compile"},
        {"from":"project:unrelated","to":"project:product","kind":"test-execution"}],
        "files":[
            {"path":"source","edges":[{"kind":"compile","to":"project:product"}]},
            {"path":"dependency","edges":[{"kind":"compile","to":"project:dependency"}]},
            {"path":"unavailable","edges":[{"kind":"compile","to":"project:unrelated"}]}]});
    fs::write(root.path().join("dependency"), b"first dependency").unwrap();
    let invoke = |registry: &Value| {
        fs::write(
            root.path().join(".chrono-harness/FILEMAP.json"),
            serde_json::to_vec(registry).unwrap(),
        )
        .unwrap();
        fs::write(
            root.path().join(".chrono-harness/cache.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let binary = Path::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"));
        let result = Command::new(binary)
            .args([
                "plan",
                "--host-root",
                root.path().to_str().unwrap(),
                "--config",
                ".chrono-harness/cache.json",
                "--consumer",
                "check.one",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<Value>(&result.stdout).unwrap()
    };
    let first = invoke(&registry);
    assert_eq!(
        first["inputs"]["source"]["observation"]["files"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    registry["files"][2]["cost"] = json!("unrelated membership update");
    assert_eq!(first, invoke(&registry));
    fs::write(root.path().join("dependency"), b"next dependency").unwrap();
    let changed = invoke(&registry);
    assert_ne!(
        first["caches"]["project"]["key"],
        changed["caches"]["project"]["key"]
    );
    assert_eq!(
        first["caches"]["project"]["restore_keys"],
        changed["caches"]["project"]["restore_keys"]
    );
    registry["project_edges"][0]["kind"] = json!("runtime-input");
    let detached = invoke(&registry);
    assert_eq!(
        detached["inputs"]["source"]["observation"]["files"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_ne!(
        changed["caches"]["project"]["key"],
        detached["caches"]["project"]["key"]
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn primary_checkout_transport_refuses_linked_worktrees_before_restoration() {
    let (root, mut config) = fixture();
    config["require_primary_checkout"] = json!(true);
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("E_CACHE_OWNERSHIP")
    );
    // Ownership consumes physical metadata only. Model those inputs directly;
    // no Git executable, repository initialization, commit or worktree is needed.
    fs::create_dir(root.path().join(".git")).unwrap();
    assert!(plan(root.path(), &config, "check.one").is_ok());
    let linked = tempfile::tempdir().unwrap();
    fs::write(linked.path().join(".git"), b"gitdir: fixture-metadata\n").unwrap();
    assert!(
        plan(linked.path(), &config, "check.one")
            .unwrap_err()
            .contains("E_CACHE_OWNERSHIP")
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn json_value_inputs_bind_selected_semantics_and_retain_whole_file_provenance() {
    let (root, mut config) = fixture();
    config["inputs"]["profile"] =
        json!({"kind":"json-value","path":"settings.json","pointer":"/build"});
    let settings = root.path().join("settings.json");
    fs::write(
        &settings,
        br#"{"build":{"flags":["-g"],"incremental":true},"units":["one"]}"#,
    )
    .unwrap();
    let first = plan(root.path(), &config, "check.one").unwrap();
    fs::write(
        &settings,
        br#"{ "units": ["two"], "build": {"incremental":true,"flags":["-g"]} }"#,
    )
    .unwrap();
    let unrelated = plan(root.path(), &config, "check.one").unwrap();
    assert_eq!(first["caches"], unrelated["caches"]);
    assert_eq!(
        first["inputs"]["profile"]["observation"],
        unrelated["inputs"]["profile"]["observation"]
    );
    assert_ne!(
        first["inputs"]["profile"]["source_file"],
        unrelated["inputs"]["profile"]["source_file"]
    );
    fs::write(&settings, br#"{"build":null}"#).unwrap();
    assert_ne!(
        first["caches"],
        plan(root.path(), &config, "check.one").unwrap()["caches"]
    );
    fs::write(&settings, b"{}").unwrap();
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("missing JSON value settings.json#/build")
    );
    fs::write(&settings, b"invalid JSON").unwrap();
    assert!(plan(root.path(), &config, "check.one").is_err());
    fs::write(&settings, br#"{"a/b":{"~c":["selected"]}}"#).unwrap();
    config["inputs"]["profile"]["pointer"] = json!("/a~1b/~0c/0");
    assert!(plan(root.path(), &config, "check.one").is_ok());
    for pointer in ["build", "/a~2b", "/a~"] {
        config["inputs"]["profile"]["pointer"] = json!(pointer);
        assert!(
            plan(root.path(), &config, "check.one")
                .unwrap_err()
                .contains("RFC 6901")
        );
    }
    config["inputs"]["profile"]["pointer"] = json!("");
    assert!(plan(root.path(), &config, "check.one").is_ok());
    fs::create_dir_all(root.path().join("out/project")).unwrap();
    fs::copy(&settings, root.path().join("out/project/settings.json")).unwrap();
    config["inputs"]["profile"]["path"] = json!("out/project/settings.json");
    assert!(
        plan(root.path(), &config, "check.one")
            .unwrap_err()
            .contains("inside a selected cache artifact")
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn registered_producer_mock_preserves_source_refresh_and_compatible_domain() {
    let (root, _) = fixture();
    let action = json!({"operation":"test.cache-tests","tool":"cargo","argv":["test","--locked","--manifest-path","crates/cache-tests/Cargo.toml","--","--test-threads=2"]});
    let registry = json!({"projects":[{"id":"cache-tests","actions":{"execute":action}}]});
    fs::write(
        root.path().join(".chrono-harness/projects.json"),
        serde_json::to_vec(&registry).unwrap(),
    )
    .unwrap();
    let mut config = json!({
        "schema":"chrono-cache/v2", "namespace":"fixture", "artifact_registry":".chrono-harness/artifacts.json",
        "artifacts":{"target":{"owner":"cache-tests","path":"crates/cache-tests/target/"}},
        "inputs":{"producer":{"kind":"json-value","path":".chrono-harness/projects.json","pointer":"/projects/0/actions/execute"}},
        "consumer_operations":{"check.unit_cache":[{"registry":".chrono-harness/projects.json","pointer":"/projects/0/actions/execute/operation"}]},
        "caches":{"check.cache-tests":{"owner":"cache-tests","producer":"test.cache-tests","consumers":["check.unit_cache"],
            "artifacts":["target"],"compatibility_inputs":["producer","cache-tests.manifest","cache-tests.lock","rustc.version"],
            "source_inputs":["cache-tests.sources"],"restore":"compatible","refresh_sources":true}}
    });
    register_artifacts(root.path(), &config);
    fs::create_dir_all(root.path().join(".git")).unwrap();
    config["require_primary_checkout"] = json!(true);
    config["recover_failed_restores"] = json!(true);
    config["recover_unconfirmed_restores"] = json!(true);
    config["caches"]["check.cache-tests"]["refresh_sources"] = json!(true);
    let project = root.path().join("crates/cache-tests");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"cache-compiler-acceptance\"\nversion = \"0.1.0\"\nedition = \"2024\"\n").unwrap();
    fs::write(
        project.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"cache-compiler-acceptance\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(project.join("tests")).unwrap();
    fs::write(
        project.join("tests/cli.rs"),
        "#[test]\nfn cli() { assert!(std::process::Command::new(env!(\"CARGO_BIN_EXE_cache-compiler-acceptance\")).status().unwrap().success()); }\n",
    )
    .unwrap();
    for (id, path) in [
        ("cache-tests.manifest", "crates/cache-tests/Cargo.toml"),
        ("cache-tests.lock", "crates/cache-tests/Cargo.lock"),
        ("cache-tests.sources", "crates/cache-tests/src/main.rs"),
    ] {
        config["inputs"][id] = json!({"kind":"file","path":path,"presence":"present"});
    }
    config["inputs"]["rustc.version"] = json!({"kind":"literal","value":"mock rustc version"});
    let registrations: Value = serde_json::from_slice(
        &fs::read(root.path().join(".chrono-harness/projects.json")).unwrap(),
    )
    .unwrap();
    let owner = registrations["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "cache-tests")
        .unwrap();
    let action = &owner["actions"]["execute"];
    let argv: Vec<String> = serde_json::from_value(action["argv"].clone()).unwrap();
    let build = || {
        assert_eq!(action["tool"], "cargo");
        let result = Command::new(env!("CARGO_BIN_EXE_chrono-cache-test-probe"))
            .arg("cargo")
            .args(&argv)
            .current_dir(root.path())
            .env("CARGO_INCREMENTAL", "1")
            .output()
            .unwrap();
        fs::write(root.path().join("compiler-stdout"), &result.stdout).unwrap();
        fs::write(root.path().join("compiler-stderr"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        fs::read(project.join("target/debug/mock-output")).unwrap()
    };
    fs::write(
        project.join("src/main.rs"),
        "fn main() { println!(\"original\"); }\n",
    )
    .unwrap();
    let before = plan(root.path(), &config, "check.unit_cache").unwrap();
    assert_eq!(build(), b"original\n");
    let incremental = project.join("target/debug/incremental/retained-input");
    fs::write(&incremental, b"retain incremental state").unwrap();
    fs::write(
        project.join("src/main.rs"),
        "fn main() { println!(\"candidate-output\"); }\n",
    )
    .unwrap();
    // Model a restored output newer than changed source. This tests timestamp
    // refresh and the command boundary; it does not execute Cargo's shortcut.
    fs::File::open(project.join("src/main.rs"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
        .unwrap();
    let after = plan(root.path(), &config, "check.unit_cache").unwrap();
    assert_ne!(
        before["caches"]["check.cache-tests"]["key"],
        after["caches"]["check.cache-tests"]["key"]
    );
    assert_eq!(
        before["caches"]["check.cache-tests"]["restore_keys"],
        after["caches"]["check.cache-tests"]["restore_keys"]
    );
    fs::create_dir_all(root.path().join(".chrono-harness/state/restore")).unwrap();
    for (path, value) in [
        (".chrono-harness/cache.json", &config),
        (".chrono-harness/state/restore/plan.json", &after),
    ] {
        fs::write(root.path().join(path), serde_json::to_vec(value).unwrap()).unwrap();
    }
    let steps: serde_json::Map<String, Value> = after["caches"]
        .as_object()
        .unwrap()
        .keys()
        .enumerate()
        .map(|(index, id)| {
            let restore = if id == "check.cache-tests" {
                json!({"outcome":"success","outputs":{"cache-matched-key":before["caches"][id]["key"]}})
            } else {
                json!({"outcome":"skipped"})
            };
            (format!("cache_{index}_restore"), restore)
        })
        .collect();
    let restored = recovery::recover(root.path(), &json!(steps));
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(build(), b"candidate-output\n");
    assert_eq!(fs::read(incremental).unwrap(), b"retain incremental state");
    assert!(
        fs::read_dir(project.join("target/debug/incremental"))
            .unwrap()
            .next()
            .is_some()
    );
    let mut incompatible = registrations;
    incompatible["projects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "cache-tests")
        .unwrap()["actions"]["execute"]["argv"]
        .as_array_mut()
        .unwrap()
        .push(json!("--release"));
    fs::write(
        root.path().join(".chrono-harness/projects.json"),
        serde_json::to_vec(&incompatible).unwrap(),
    )
    .unwrap();
    let incompatible = plan(root.path(), &config, "check.unit_cache").unwrap();
    assert_ne!(
        after["caches"]["check.cache-tests"]["restore_keys"],
        incompatible["caches"]["check.cache-tests"]["restore_keys"]
    );
}

#[cfg(all(unix, not(verus_keep_ghost)))]
#[test]
fn invalid_native_argument_in_an_unadopted_host_retains_its_original_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let host = tempfile::tempdir().unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"))
        .current_dir(host.path())
        .env_clear()
        .arg("plan")
        .arg(std::ffi::OsString::from_vec(vec![201, 255]))
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(run.stdout.is_empty());
    let diag: serde_json::Value = serde_json::from_slice(&run.stderr).unwrap();
    assert_eq!(diag["error"]["code"], "E_USAGE");
    assert_eq!(diag["error"]["context"]["argv_index"], 2);
    assert_eq!(diag["error"]["context"]["argument_index"], 1);
    assert_eq!(
        diag["error"]["context"]["argument_bytes"],
        serde_json::json!([201, 255])
    );
}
