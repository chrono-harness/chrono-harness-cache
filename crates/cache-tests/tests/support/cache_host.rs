use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

pub(crate) struct Host(tempfile::TempDir);

impl Host {
    pub(crate) fn new() -> Self {
        let retained = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.chrono-harness/state/cache-test-failures");
        fs::create_dir_all(&retained).unwrap();
        Self(tempfile::tempdir_in(retained).unwrap())
    }

    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if std::thread::panicking() {
            use std::io::Write;
            self.0.disable_cleanup(true);
            let _ = writeln!(
                std::io::stderr(),
                "Cache test original host: {}",
                self.path().display()
            );
        }
    }
}

pub(crate) fn fixture() -> (Host, Value) {
    let root = Host::new();
    fs::create_dir(root.path().join(".chrono-harness")).unwrap();
    fs::write(
        root.path().join(".chrono-harness/retention.json"),
        include_bytes!("../fixtures/retention.json"),
    )
    .unwrap();
    fs::write(root.path().join("compiler"), b"compiler-one").unwrap();
    fs::write(root.path().join("source"), b"source-one").unwrap();
    let mut config = json!({"schema":"chrono-cache/v2","namespace":"fixture","artifact_registry":".chrono-harness/artifacts.json",
        "inputs":{
            "compiler":{"kind":"file","path":"compiler","presence":"present"},
            "source":{"kind":"file","path":"source","presence":"present"},
            "profile":{"kind":"literal","value":{"profile":"debug","incremental":true}},
            "other":{"kind":"file","path":"unavailable-other-input","presence":"present"}},
        "artifacts":{
            "target":{"owner":"project","path":"out/project"},
            "other-target":{"owner":"other","path":"out/other"}},
        "caches":{
            "project":{"owner":"project","producer":"build.project","consumers":["check.one","check.two"],
                "artifacts":["target"],"compatibility_inputs":["compiler","profile"],"source_inputs":["source"],"restore":"compatible"},
            "other":{"owner":"other","producer":"build.other","consumers":["check.other"],
                "artifacts":["other-target"],"compatibility_inputs":["compiler"],"source_inputs":["other"],"restore":"exact"}}});
    register_consumer_contract(root.path(), &mut config);
    register_artifacts(root.path(), &config);
    (root, config)
}

pub(crate) fn register_artifacts(root: &Path, config: &Value) {
    let rows: Vec<_> = config["artifacts"].as_object().unwrap().values()
        .map(|a| json!({"path":a["path"],"owner":a["owner"],"kind":"fixture-output","tracked":false})).collect();
    fs::write(
        root.join(".chrono-harness/artifacts.json"),
        serde_json::to_vec(&json!({"artifacts":rows})).unwrap(),
    )
    .unwrap();
}

pub(crate) fn register_consumer_contract(root: &Path, config: &mut Value) {
    fs::write(
        root.join(".chrono-harness/operations.json"),
        br#"{"build":["build.project"],"other":"build.other"}"#,
    )
    .unwrap();
    config["consumer_operations"] = json!({
        "check.one":[{"registry":".chrono-harness/operations.json","pointer":"/build"}],
        "check.two":[{"registry":".chrono-harness/operations.json","pointer":"/build"}],
        "check.other":[{"registry":".chrono-harness/operations.json","pointer":"/other"}]
    });
}

pub(crate) fn probe_config(root: &Path, config: &mut Value, args: Value) {
    config["inputs"]["compiler"] = json!({"kind":"command","command":{
    "program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"),"args":args,"env":{},
    "timeout_seconds":5,"output_limit_bytes":4096},"inherit":[],"result":"stdout"});
    fs::write(
        root.join(".chrono-harness/cache.json"),
        serde_json::to_vec(config).unwrap(),
    )
    .unwrap();
}

pub(crate) fn plan_args(root: &Path, output: &str) -> Vec<String> {
    [
        "plan",
        "--host-root",
        root.to_str().unwrap(),
        "--config",
        ".chrono-harness/cache.json",
        "--consumer",
        "check.one",
        "--plan-output",
        output,
    ]
    .map(str::to_owned)
    .to_vec()
}

pub(crate) fn attempt_path(root: &Path, output: &str) -> std::path::PathBuf {
    root.join(format!(
        ".chrono-harness/state/cache-probes/plan-{:x}",
        Sha256::digest(output.as_bytes())
    ))
}

pub(crate) fn invoke(root: &Path, output: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"))
        .args(plan_args(root, output))
        .output()
        .unwrap()
}

pub(crate) fn seal(root: &Path, output: &str, cache: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli"));
    command.args([
        "seal",
        "--host-root",
        root.to_str().unwrap(),
        "--config",
        ".chrono-harness/cache.json",
        "--plan",
        output,
        "--steps-env",
        "CACHE_STEPS",
        "--work",
        "build",
    ]);
    if let Some(cache) = cache {
        command.args(["--cache", cache]);
    }
    command
        .env("CACHE_STEPS", "{\"build\":{\"outcome\":\"success\"}} ")
        .output()
        .unwrap()
}

pub(crate) fn mock_git(root: &Path) {
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(
        root.join(".chrono-harness/config.json"),
        serde_json::to_vec(&json!({
    "schema_version":4,"facts_git":{"tool":"git"},
    "tools":[{"id":"git","program":env!("CARGO_BIN_EXE_chrono-cache-test-probe"),
        "resolution":"PATH-once","version_argv":["--version"],"expected_version":null}],
    "environment":{"inherit":[],"values":{},"inputs":[]},
    "protocol":{"timeout_seconds":5,"stdout_limit_bytes":65536}}))
        .unwrap(),
    )
    .unwrap();
}

pub(crate) fn recover(root: &Path, steps: &Value) -> std::process::Output {
    Command::new(Path::new(env!("CARGO_BIN_EXE_chrono-cache-test-cli")))
        .args([
            "recover",
            "--host-root",
            root.to_str().unwrap(),
            "--config",
            ".chrono-harness/cache.json",
            "--plan",
            ".chrono-harness/state/restore/plan.json",
            "--steps-env",
            "CACHE_RECOVERY_STEPS",
        ])
        .env(
            "CACHE_RECOVERY_STEPS",
            serde_json::to_string(steps).unwrap(),
        )
        .output()
        .unwrap()
}
