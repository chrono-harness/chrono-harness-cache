//! The cache test drives a deterministic custodian and an explicitly selected Rust probe.
mod mock_inventory;
use mock_inventory::{Inventory, hash, member_bytes};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};

type Reply = Result<Value, String>;
fn text<'a>(q: &'a Value, key: &str) -> Result<&'a str, String> {
    q[key]
        .as_str()
        .ok_or_else(|| format!("missing fixture field {key}"))
}
fn length(q: &Value, key: &str) -> Result<u64, String> {
    q[key]
        .as_u64()
        .ok_or_else(|| format!("missing fixture integer {key}"))
}
fn fact(i: &Inventory, q: &Value) -> Reply {
    let path = q["path"].as_str().unwrap_or("");
    let producer = q["producer"].as_str().unwrap_or("");
    let policy: Value = serde_json::from_slice(
        &fs::read(i.root.join(".chrono-harness/retention.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    match text(q, "action")? {
        "adoption" => Ok(
            json!({"cache-probe-attempt":policy["terminal_limits"]["cache-probe-attempt"].is_object(),"cache-plan":policy["terminal_limits"]["cache-plan"].is_object()}),
        ),
        "rooted" => Ok(json!(i.rooted_original(producer, text(q, "role")?)?)),
        "enrolled" => i.update(|r| Ok(json!(r.contains_key(path)))),
        "status" => i.producer_status(producer, Some(path)),
        "maintain" => i.maintain_originals(&[]),
        "resume-sealed" => {
            i.seal(path, &q["outcome"])?;
            Ok(json!(true))
        }
        "published" => i.update(|rows| {
            Ok(json!(
                rows.get(path).is_some_and(|r| r["sha256"] == q["sha256"])
                    && member_bytes(&i.root, path, text(q, "sha256")?, length(q, "length")?)
            ))
        }),
        "member" => Ok(json!(i.published_tree_member(
            producer,
            path,
            text(q, "member")?,
            text(q, "sha256")?,
            length(q, "length")?
        )?)),
        "consume" => i.update(|r| {
            if r.values().any(|v| v["id"] == q["id"]) {
                Ok(Value::Null)
            } else {
                Err("missing mock reader".into())
            }
        }),
        "release-role" => {
            i.reference_paths(text(q, "role")?, &[])?;
            Ok(Value::Null)
        }
        "final-original" => {
            if !policy["producers"][producer]
                .as_array()
                .is_some_and(|paths| {
                    paths
                        .iter()
                        .any(|p| p.as_str().is_some_and(|prefix| path.starts_with(prefix)))
                })
            {
                return Ok(json!(false));
            }
            let data = text(q, "text")?.as_bytes();
            let actual = i.root.join(path);
            if actual.exists() && fs::read(&actual).map_err(|e| e.to_string())? != data {
                return Err("addressed original bytes mismatch".into());
            }
            fs::write(&actual, data).map_err(|e| e.to_string())?;
            i.update(|r|{r.entry(path.to_owned()).or_insert_with(||json!({"id":hash(path.as_bytes()),"producer":producer,"phase":"released","sha256":hash(data)}));Ok(json!(true))})
        }
        other => Err(format!("unknown fixture fact {other}")),
    }
}
fn execute(root: &Path, q: &Value) -> Reply {
    use std::os::unix::process::ExitStatusExt;
    let command = &q["command"];
    let program = text(command, "program")?;
    let args = command["args"]
        .as_array()
        .ok_or("missing fixture args")?
        .iter()
        .map(|x| x.as_str().ok_or("fixture arg"))
        .collect::<Result<Vec<_>, _>>()?;
    let env = command["env"]
        .as_object()
        .ok_or("missing fixture environment")?;
    let dir = root.join(text(q, "path")?).join(text(q, "member")?);
    let stdout = dir.join("stdout");
    let stderr = dir.join("stderr");
    let mut child = Command::new(program)
        .args(&args)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v.as_str().unwrap())))
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(fs::File::create(&stdout).map_err(|e| e.to_string())?)
        .stderr(fs::File::create(&stderr).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| e.to_string())?;
    let child_pid = child.id();
    let launcher_pid = std::process::id();
    let launch = json!({"schema":"chrono-process-launch/v1","launcher_pid":launcher_pid,"child_pid":child_pid,"executable":program,"sha256":q["expected_sha256"]});
    let staged = dir.join("launch.json.writing");
    fs::write(&staged, serde_json::to_vec(&launch).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(staged, dir.join("launch.json")).map_err(|e| e.to_string())?;
    // These two programmed replies exercise the declared limit and timeout contract.
    let sleep = args.first() == Some(&"sleep");
    if sleep {
        child.kill().map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let mut stdout = fs::read(stdout).map_err(|e| e.to_string())?;
    let stderr = fs::read(stderr).map_err(|e| e.to_string())?;
    let limit = length(command, "output_limit_bytes")? as usize;
    let failure = if sleep {
        Some("process timed out")
    } else if stdout.len() > limit {
        stdout.truncate(limit);
        Some("process output limit exceeded")
    } else {
        None
    };
    let mut argv = vec![program];
    argv.extend(args);
    Ok(
        json!({"argv":argv,"cwd":root,"environment":env,"executable":program,"sha256":q["expected_sha256"],"exit_code":status.code().unwrap_or(-1),"exit_signal":status.signal(),"owned_work_joined":true,"failure":failure,
              "stdout":String::from_utf8_lossy(&stdout),"stderr":String::from_utf8_lossy(&stderr),"stdout_sha256":hash(&stdout),"stderr_sha256":hash(&stderr),"stdout_bytes":stdout,"stderr_bytes":stderr,"child_pid":child_pid,"launcher_pid":launcher_pid}),
    )
}
fn answer(i: &Inventory, q: Value) -> Reply {
    let path = q["path"].as_str().unwrap_or("");
    match text(&q, "operation")? {
        "inventory" => fact(i, &q["fact"]),
        "begin-directory" => {
            fs::create_dir(i.root.join(path)).map_err(|e| e.to_string())?;
            let id = format!("mock-{}-{}", std::process::id(), hash(path.as_bytes()));
            i.update(|r|{r.insert(path.into(),json!({"id":id,"producer":q["producer"],"role":q["reference"],"phase":"publishing","outcome":q["intent"]}));Ok(json!({"adopted":true,"id":id}))})
        }
        "create-directory-member" => {
            fs::create_dir(i.root.join(path).join(text(&q, "name")?)).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "reserve-directory-file" => Ok(json!(i.root.join(path).join(text(&q, "name")?))),
        "execute-retained" => execute(&i.root, &q),
        "validate-launch" => {
            let receipt = i.root.join(text(&q["original"], "path")?);
            let raw = fs::read(&receipt).map_err(|e| e.to_string())?;
            if hash(&raw) != q["original"]["sha256"] {
                return Err("changed process original".into());
            }
            let process: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
            let launch: Value = serde_json::from_slice(
                &fs::read(receipt.with_file_name("launch.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if process["child_pid"] != launch["child_pid"]
                || process["launcher_pid"] != launch["launcher_pid"]
            {
                return Err("receipt differs from launch".into());
            }
            Ok(Value::Null)
        }
        "seal-directory" => {
            i.seal(path, &q["outcome"])?;
            Ok(Value::Null)
        }
        "dependencies" => i.update(|r| {
            let dependencies = q["originals"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| {
                    let member = Path::new(p.as_str()?);
                    r.iter()
                        .find(|(path, _)| member.starts_with(path.as_str()))
                        .map(|(_, row)| row["id"].clone())
                })
                .collect::<Vec<_>>();
            if let Some(row) = r.get_mut(path) {
                row["dependencies"] = json!(dependencies);
            }
            Ok(Value::Null)
        }),
        "read-original" => {
            let reference = &q["original"];
            let raw = fs::read(i.root.join(text(reference, "path")?)).map_err(|e| e.to_string())?;
            if hash(&raw) != reference["sha256"] {
                return Err("original digest differs".into());
            }
            String::from_utf8(raw)
                .map(Value::String)
                .map_err(|e| e.to_string())
        }
        other => Err(format!("unhandled fixture request {other}")),
    }
}
fn diagnostics(root: &Path) {
    let policy: Value =
        serde_json::from_slice(&fs::read(root.join(".chrono-harness/logging.json")).unwrap())
            .unwrap();
    let dir = root.join(policy["directory"].as_str().unwrap());
    fs::create_dir_all(&dir).unwrap();
    let mut log = fs::File::create(dir.join(format!("mock-{}.jsonl", std::process::id()))).unwrap();
    println!("{{\"ready\":true}}");
    io::stdout().flush().unwrap();
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        writeln!(log, "{line}").unwrap();
        let q: Value = serde_json::from_str(&line).unwrap();
        if let Some(status) = q["exit_code"].as_u64() {
            std::process::exit(status as i32);
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "cli-events-v1") {
        diagnostics(&PathBuf::from(&args[2]));
        return;
    }
    assert_eq!(args.get(1).unwrap(), "retained-artifacts-v1");
    let i = Inventory::adopted(&std::env::current_dir().unwrap())
        .unwrap()
        .unwrap();
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let result = answer(&i, serde_json::from_str(&line.unwrap()).unwrap());
        if serde_json::to_writer(&mut output, &result).is_err()
            || writeln!(output).is_err()
            || output.flush().is_err()
        {
            break;
        }
    }
}
