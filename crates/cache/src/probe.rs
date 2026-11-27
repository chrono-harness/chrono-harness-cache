#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
use super::confirmed_probe;
#[cfg(not(verus_keep_ghost))]
use crate::verified_planner::confirmed_probe;
#[cfg(not(verus_keep_ghost))]
use vstd::prelude::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub enum ProbeError<E> {
    Launch(E),
    Unknown,
    Failed,
}

/// Record terminality separately from whether this result may supply a cache key.
pub fn admit_probe<E, T>(
    launch: Result<(), E>, exit_code: i32, signal: Option<i32>, joined: bool,
    failed: bool, selected: T,
) -> (out: (bool, Result<T, ProbeError<E>>))
    ensures
        out.0 == (launch.is_ok() && (exit_code >= 0 || signal.is_some()) && joined),
        out.1 == match launch {
            Err(error) => Err(ProbeError::Launch(error)),
            Ok(()) => if !out.0 { Err(ProbeError::Unknown) }
                else if exit_code != 0 || failed { Err(ProbeError::Failed) }
                else { Ok(selected) },
        },
{
    let terminal = confirmed_probe(launch.is_ok(), exit_code, signal, joined);
    let result = match launch {
        Err(error) => Err(ProbeError::Launch(error)),
        Ok(()) => if !terminal { Err(ProbeError::Unknown) }
            else if exit_code != 0 || failed { Err(ProbeError::Failed) }
            else { Ok(selected) },
    };
    (terminal, result)
}
}

#[cfg(not(verus_keep_ghost))]
use crate::retention::CommandSpec;
#[cfg(not(verus_keep_ghost))]
use crate::{file_identity, sha256};
#[cfg(not(verus_keep_ghost))]
use serde::{Deserialize, Serialize};
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::{collections::BTreeMap, fs, path::Path};

#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResultKind {
    Stdout,
    FilePath,
}

#[cfg(not(verus_keep_ghost))]
pub fn observe(
    root: &Path,
    command: &CommandSpec,
    inherit: &[String],
    result: &ResultKind,
    attempt: &super::attempt::Attempt,
    input_id: &str,
) -> Result<(Value, Value), String> {
    let directory = attempt.directory.join(input_id);
    attempt.publication.create_directory(&directory)?;
    fs::File::open(&attempt.directory)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    let original = format!("{}/{input_id}/process.json", attempt.relative);
    let intent = json!({"schema":"chrono-cache-probe-intent/v1","attempt":attempt.relative,
        "input":input_id,"command":command,"inherit":inherit,"result_kind":result,
        "stdin_sha256":sha256(&[]),"task_identity":null,"terminal_result":null,
        "launch":"launch.json","stdout":"stdout","stderr":"stderr",
        "wait":"original process engine native join; absent terminal result stays unknown"});
    attempt.record(&directory.join("intent.json"), &intent)?;
    let mut invoked = false;
    let evaluated = (|| {
        let mut command = command.clone();
        for name in inherit {
            match std::env::var(name) {
                Ok(value) => {
                    command.env.insert(name.clone(), value);
                }
                Err(std::env::VarError::NotPresent) => {}
                Err(e) => return Err(format!("environment {name}: {e}")),
            }
        }
        let executable = command.program(root)?;
        let (executable_sha256, _) = file_identity(&executable)?;
        command.program = executable
            .to_str()
            .ok_or("non-UTF8 executable path")?
            .to_owned();
        attempt.record(
            &directory.join("invocation.json"),
            &json!({"command":command,"executable_sha256":executable_sha256}),
        )?;
        invoked = true;
        attempt.unknown.set(true);
        attempt
            .publication
            .execute(input_id, &command, &executable_sha256)
    })();
    let process = match evaluated {
        Ok(process) => process,
        Err(error) => {
            let record = json!({"error":error,"engine_invoked":invoked,"returned_task_identity":null,
                "launch_original":if invoked {json!({"path":"launch.json","availability":"not-assessed"})} else {Value::Null},
                "terminal_result":null,"completion":if invoked {"unknown"} else {"not-dispatched"}});
            attempt
                .record(&directory.join("error.json"), &record)
                .map_err(|publication| {
                    format!(
                        "E_CACHE_PROBE: {error}; error publication: {publication}; attempt {}",
                        attempt.relative
                    )
                })?;
            return Err(format!(
                "E_CACHE_PROBE: {error}; original {}/{}",
                attempt.relative,
                format!("{input_id}/error.json")
            ));
        }
    };
    // Publish the actual receipt before evaluating launch, exit or file-path semantics.
    #[derive(Deserialize)]
    struct Process {
        exit_code: i32,
        exit_signal: Option<i32>,
        owned_work_joined: bool,
        failure: Option<String>,
        environment: BTreeMap<String, String>,
        executable: std::path::PathBuf,
        sha256: String,
        argv: Vec<String>,
        stdout_sha256: String,
        stdout_bytes: Vec<u8>,
        stderr_sha256: String,
        stderr_bytes: Vec<u8>,
    }
    let process_value = process;
    let process: Process =
        serde_json::from_value(process_value.clone()).map_err(|e| e.to_string())?;
    attempt.record(&directory.join("process.json"), &process_value).map_err(|e|
        format!("E_CACHE_PROBE: original receipt publication: {e}; attempt {}; observed exit {} signal {:?}; original {original}", attempt.publication.id(), process.exit_code, process.exit_signal))?;
    let (original_digest, _) = file_identity(&directory.join("process.json"))?;
    let launch = attempt
        .publication
        .session
        .launch(Path::new(&original), &original_digest);
    let (terminal, admitted) = admit_probe(
        launch,
        process.exit_code,
        process.exit_signal,
        process.owned_work_joined,
        process.failure.is_some(),
        result,
    );
    attempt.unknown.set(!terminal);
    let result = admitted.map_err(|error| match error {
        ProbeError::Launch(error) => format!("E_CACHE_PROBE: {error}; original {original}"),
        ProbeError::Unknown => {
            format!("E_CACHE_PROBE: original process terminality unknown; original {original}")
        }
        ProbeError::Failed => format!(
            "E_CACHE_PROBE: exit {}, failure {:?}; original {original}",
            process.exit_code, process.failure
        ),
    })?;
    let environment: BTreeMap<_, _> = process
        .environment
        .iter()
        .map(|(name, value)| {
            (
                name,
                json!({"sha256":sha256(value.as_bytes()),"length":value.len()}),
            )
        })
        .collect();
    let mut observed = json!({"executable":process.executable,"executable_sha256":process.sha256,
        "argv":process.argv,"environment":environment,"exit_code":process.exit_code,
        "stdout_sha256":process.stdout_sha256,"stdout_length":process.stdout_bytes.len(),
        "stderr_sha256":process.stderr_sha256,"stderr_length":process.stderr_bytes.len()});
    if matches!(result, ResultKind::FilePath) {
        let value = std::str::from_utf8(&process.stdout_bytes)
            .map_err(|e| format!("E_CACHE_PROBE: path encoding: {e}; original {original}"))?;
        let end = super::verified_planner::probe_path_end(value.as_bytes()).ok_or_else(|| {
            format!("E_CACHE_PROBE: expected one absolute file path; original {original}")
        })?;
        let actual = fs::canonicalize(&value[..end])
            .map_err(|e| format!("E_CACHE_PROBE: {e}; original {original}"))?;
        let (digest, length) = file_identity(&actual)
            .map_err(|e| format!("E_CACHE_PROBE: {e}; original {original}"))?;
        observed["file"] = json!({"path":actual,"sha256":digest,"length":length});
    }
    Ok((
        observed,
        json!({"path":original,"sha256":original_digest,"attempt_id":attempt.publication.id()}),
    ))
}
