//! Keep the caller's PID, typed argument bytes and business events in one diagnostics session.
use crate::{retention::installed, verified_planner::cli as decision};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
};

struct Sink {
    child: Child,
    writer: Option<ChildStdin>,
}
impl Drop for Sink {
    fn drop(&mut self) {
        self.writer.take();
        let _ = self.child.wait();
    }
}
impl Sink {
    fn open(root: &Path, operation: &str) -> io::Result<Self> {
        let mut child = Command::new(installed("chrono-diagnostics"))
            .arg("cli-events-v1")
            .arg(root)
            .arg("chrono-cache")
            .arg(operation)
            .arg(std::process::id().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let writer = child.stdin.take();
        let reader = child.stdout.take().unwrap();
        let sink = Self { child, writer };
        let mut ready = String::new();
        BufReader::new(reader).read_line(&mut ready)?;
        if !serde_json::from_str::<Value>(&ready).is_ok_and(|r| r == json!({"ready":true})) {
            return Err(io::Error::other("diagnostics did not open its CLI log"));
        }
        Ok(sink)
    }
    fn event(&mut self, message: Value) -> io::Result<()> {
        let input = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostics closed"))?;
        serde_json::to_writer(&mut *input, &message)?;
        input.write_all(b"\n")?;
        input.flush()
    }
    fn finish(mut self, exit: u8) -> io::Result<u8> {
        self.event(json!({"exit_code":exit}))?;
        self.writer.take();
        decision::diagnostics_exit(exit, self.child.wait()?.code())
            .map_err(|status| io::Error::other(format!("unexpected diagnostics status {status:?}")))
    }
}

fn drive(args: &[OsString], sink: &mut Option<Sink>) -> io::Result<u8> {
    let mut decoded = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        match decision::argument(index, arg.to_str(), arg.as_encoded_bytes()) {
            Ok(Some(value)) => decoded.push(value.to_owned()),
            Ok(None) => {}
            Err((argv_index, argument_index, bytes)) => {
                if let Some(log) = sink.as_mut() {
                    log.event(json!({"invalid_argument":{"argv_index":argv_index,"bytes":bytes}}))?;
                } else {
                    let error = json!({"schema":"chrono-error/v1","error":{"type":"chrono_cache::cli","code":"E_USAGE","message":"arguments must be valid UTF-8.","context":{"argv_index":argv_index,"argument_index":argument_index,"argument_bytes":bytes},"source":null}});
                    eprintln!("{error}");
                }
                return Ok(2);
            }
        }
    }
    let run = || {
        decision::output(
            (),
            crate::dispatch(&decoded),
            |(), output| print!("{output}"),
            |(), error| eprintln!("{error}"),
        )
        .1
    };
    let Some(writer) = sink.as_mut() else {
        return decision::finish_business(run(), |_| Ok(()));
    };
    let state = Arc::new(Mutex::new((
        writer.writer.take().unwrap(),
        Ok::<(), io::Error>(()),
    )));
    let output = state.clone();
    let sender = tracing_tunnel::TracingEventSender::new(move |event| {
        let mut output = output.lock().unwrap();
        let (input, result) = &mut *output;
        if result.is_ok() {
            *result = (|| -> io::Result<()> {
                serde_json::to_writer(&mut *input, &json!({"event":event}))?;
                input.write_all(b"\n")?;
                input.flush()
            })();
        }
    });
    let result = tracing::subscriber::with_default(sender, run);
    let (input, published) = Arc::try_unwrap(state)
        .map_err(|_| io::Error::other("diagnostics sender still live"))?
        .into_inner()
        .unwrap();
    writer.writer = Some(input);
    published?;
    decision::finish_business(result, |error| writer.event(json!({"diagnostic":error})))
}
fn host(args: &[OsString]) -> io::Result<PathBuf> {
    args.get(1..)
        .unwrap_or_default()
        .windows(2)
        .find(|pair| pair[0] == "--host-root")
        .map(|pair| Ok(PathBuf::from(&pair[1])))
        .unwrap_or_else(std::env::current_dir)
}
fn operation(args: &[OsString]) -> &str {
    args.get(1).and_then(|arg| arg.to_str()).unwrap_or("")
}
pub fn main() -> u8 {
    let args: Vec<_> = std::env::args_os().collect();
    let run = || -> io::Result<u8> {
        let root = host(&args)?;
        let adopted = match std::fs::symlink_metadata(root.join(".chrono-harness/logging.json")) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        let mut sink = adopted
            .then(|| Sink::open(&root, operation(&args)))
            .transpose()?;
        let code = drive(&args, &mut sink)?;
        match sink {
            Some(sink) => sink.finish(code),
            None => Ok(code),
        }
    };
    run().unwrap_or_else(|e| {
        eprintln!("chrono-cache diagnostics: {e}");
        2
    })
}
