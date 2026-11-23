//! Immediate last-close retirement requires its own Cargo test process.
#[path = "support/cache_host.rs"]
mod cache_host;

use cache_host::{attempt_path, fixture, invoke, mock_git, probe_config, recover, seal};
use serde_json::{Value, json};
use std::fs;

#[test]
fn recovery_and_seal_keep_probe_custody_until_final_consumer_releases() {
    let (root, mut config) = fixture();
    mock_git(root.path());
    config["require_primary_checkout"] = json!(true);
    config["recover_failed_restores"] = json!(true);
    config["recover_unconfirmed_restores"] = json!(true);
    config["caches"]["project"]["verify_contents"] = json!(true);
    let count = root.path().join("invocations");
    probe_config(root.path(), &mut config, json!(["count", count]));
    let output = ".chrono-harness/state/restore/plan.json";
    let run = invoke(root.path(), output);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let plan: Value = serde_json::from_slice(&run.stdout).unwrap();
    let attempt = attempt_path(root.path(), output);
    let original = plan["inputs"]["compiler"]["original"]["path"]
        .as_str()
        .unwrap();
    let original_bytes = fs::read(root.path().join(original)).unwrap();
    let recovered = recover(
        root.path(),
        &json!({"cache_0_restore":{"outcome":"success","outputs":{"cache-matched-key":plan["caches"]["project"]["key"]}}}),
    );
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_eq!(fs::read(&count).unwrap(), b"x");
    let inventory = cache_host::mock_inventory::Inventory::adopted(root.path())
        .unwrap()
        .unwrap();
    inventory.maintain_at(u64::MAX).unwrap();
    assert!(
        attempt.is_dir(),
        "the seal consumer still needs the original attempt"
    );
    fs::create_dir_all(root.path().join("out/project")).unwrap();
    fs::write(
        root.path().join("out/project/object"),
        b"actual mock build bytes",
    )
    .unwrap();
    let partial = seal(root.path(), output, Some("project"));
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    inventory.maintain_at(u64::MAX).unwrap();
    assert!(
        attempt.is_dir(),
        "partial seal keeps the plan's whole consumer"
    );
    let completed = seal(root.path(), output, None);
    assert!(
        completed.status.success(),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    assert_eq!(fs::read(&count).unwrap(), b"x");
    assert_eq!(
        fs::read(root.path().join(original)).unwrap(),
        original_bytes
    );
    let mut originals = Vec::new();
    for (producer, path) in [
        (
            "cache-probe-attempt",
            attempt.strip_prefix(root.path()).unwrap().to_str().unwrap(),
        ),
        ("cache-plan", output),
    ] {
        let status = inventory.producer_status(producer, Some(path)).unwrap();
        assert_eq!(status["records"].as_array().unwrap().len(), 1, "{status}");
        originals.push(status["records"][0]["id"].as_str().unwrap().to_owned());
    }
    inventory.maintain_originals_at(&originals, 0).unwrap();
    assert!(attempt.is_dir() && root.path().join(output).is_file());
    let mut rounds = Vec::new();
    // Two retained outputs, eleven declared physical nodes, and up to four
    // phase/edge transitions per output. The original 25 ms round budget stays
    // unchanged, including rounds that only advance bounded metadata work.
    for _ in 0..(11 + 4 * 2) {
        if originals.is_empty() {
            break;
        }
        let round = inventory
            .maintain_originals_at(&originals, u64::MAX)
            .unwrap();
        for error in round["accounting_errors"].as_array().unwrap() {
            // Nonblocking maintenance may honestly preserve a busy local
            // component. Corruption is not accepted, and the final physical
            // retirement assertions still require all busy work to resolve.
            assert!(
                error["reason"]
                    .as_str()
                    .unwrap()
                    .contains("metadata is busy"),
                "{round}"
            );
            assert!(
                !round["protected"].as_array().unwrap().is_empty(),
                "{round}"
            );
        }
        originals.retain(|id| {
            !round["disposed"]
                .as_array()
                .unwrap()
                .iter()
                .any(|retired| retired["id"].as_str() == Some(id))
        });
        rounds.push(round);
    }
    fs::write(
        root.path().join("maintenance.json"),
        serde_json::to_vec(&rounds).unwrap(),
    )
    .unwrap();
    assert!(
        !attempt.exists(),
        "confirmed terminal originals retire after consumer release: {rounds:?}"
    );
    assert!(
        !root.path().join(output).exists(),
        "the released plan retires through its own finite policy"
    );
    // Reusing an entirely retired address creates a different attempt. A stale
    // plan must not qualify that later generation, even with equal semantic keys.
    let next = invoke(root.path(), output);
    assert!(
        next.status.success(),
        "{}",
        String::from_utf8_lossy(&next.stderr)
    );
    let next_plan: Value = serde_json::from_slice(&next.stdout).unwrap();
    assert_eq!(plan["caches"], next_plan["caches"]);
    assert_ne!(
        plan["inputs"]["compiler"]["original"]["attempt_id"],
        next_plan["inputs"]["compiler"]["original"]["attempt_id"]
    );
    fs::write(root.path().join(output), serde_json::to_vec(&plan).unwrap()).unwrap();
    let stale = seal(root.path(), output, None);
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr)
            .contains("original digest or attempt identity differs"),
        "{}",
        String::from_utf8_lossy(&stale.stderr)
    );
    assert_eq!(fs::read(&count).unwrap(), b"xx");
}
