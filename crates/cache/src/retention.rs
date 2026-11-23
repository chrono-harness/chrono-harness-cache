//! The installed runner owns publication locks, original process streams and retention metadata.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::OsString,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    rc::{Rc, Weak},
};

pub struct Session {
    io: Rc<RefCell<Connection>>,
}
thread_local! {
    static ACTIVE: RefCell<BTreeMap<PathBuf, Weak<RefCell<Connection>>>> = RefCell::new(BTreeMap::new());
}
struct Connection {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.wait();
    }
}

pub fn installed(name: &str) -> OsString {
    if std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
    {
        return name.into();
    }
    std::env::args_os()
        .next()
        .and_then(|p| {
            Path::new(&p)
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .and_then(|d| std::path::absolute(d).ok())
        })
        .map(|d| d.join(name).into_os_string())
        .unwrap_or_else(|| name.into())
}

impl Session {
    /// The retention owner records read capabilities for the lifetime of one operation.
    /// Multiple requests on this host must use the same owner process.
    pub fn scope(root: &Path) -> Result<Option<Self>, String> {
        let policy = crate::no_symlink_parents(root, ".chrono-harness/retention.json")?;
        match std::fs::symlink_metadata(policy) {
            Ok(_) => Self::open(root).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn open(root: &Path) -> Result<Self, String> {
        let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
        if let Some(io) = ACTIVE.with(|active| active.borrow().get(&root).and_then(Weak::upgrade)) {
            return Ok(Self { io });
        }
        let mut child = Command::new(installed("chrono-harness"))
            .arg("retained-artifacts-v1")
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("start retained-artifacts-v1: {e}"))?;
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        let io = Rc::new(RefCell::new(Connection {
            child,
            input,
            output,
        }));
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            active.retain(|_, entry| entry.strong_count() != 0);
            active.insert(root, Rc::downgrade(&io));
        });
        Ok(Self { io })
    }
    pub fn query<T: DeserializeOwned>(&self, query: Value) -> Result<T, String> {
        let mut io = self.io.borrow_mut();
        let input = io.input.as_mut().ok_or("original session closed")?;
        serde_json::to_writer(&mut *input, &query).map_err(|e| e.to_string())?;
        input
            .write_all(b"\n")
            .and_then(|_| input.flush())
            .map_err(|e| e.to_string())?;
        let mut line = Vec::new();
        io.output
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if !line.ends_with(b"\n") {
            return Err("incomplete retained artifact response".into());
        }
        let data: Result<T, String> = crate::decode(&line)?;
        data
    }
    fn fact<T: DeserializeOwned>(&self, fact: Value) -> Result<T, String> {
        self.query(json!({"operation":"inventory","fact":fact}))
    }
    pub fn launch(&self, path: &Path, sha256: &str) -> Result<(), String> {
        self.query(json!({"operation":"validate-launch","original":{"path":path,"sha256":sha256}}))
    }
}

pub struct Inventory {
    pub session: Session,
    root: PathBuf,
    policies: BTreeMap<String, bool>,
}
impl Inventory {
    pub fn adopted(root: &Path) -> Result<Option<Self>, String> {
        let Some(session) = Session::scope(root)? else {
            return Ok(None);
        };
        let policies: Option<BTreeMap<String, bool>> = session
            .fact(json!({"action":"adoption","producers":["cache-plan","cache-probe-attempt"]}))?;
        Ok(policies.map(|policies| Self {
            session,
            root: root.to_owned(),
            policies,
        }))
    }
    pub fn has_terminal_limits(&self, id: &str) -> bool {
        self.policies.get(id) == Some(&true)
    }
    pub fn rooted_original(
        &self,
        producer: &str,
        role: &str,
    ) -> Result<Option<(String, String, String, Value)>, String> {
        self.session
            .fact(json!({"action":"rooted","producer":producer,"role":role}))
    }
    pub fn enrolled(&self, path: &str) -> Result<bool, String> {
        self.session.fact(json!({"action":"enrolled","path":path}))
    }
    pub fn producer_status(&self, producer: &str, path: &str) -> Result<Value, String> {
        self.session
            .fact(json!({"action":"status","producer":producer,"path":path}))
    }
    pub fn maintain_originals(&self, ids: &[String]) -> Result<Value, String> {
        self.session.fact(json!({"action":"maintain","ids":ids}))
    }
    pub fn resume(&self, producer: &str, path: &str, outcome: Value) -> Result<bool, String> {
        self.session.fact(
            json!({"action":"resume-sealed","producer":producer,"path":path,"outcome":outcome}),
        )
    }
    pub fn published_original(
        &self,
        path: &str,
        digest: &str,
        length: u64,
    ) -> Result<bool, String> {
        self.session
            .fact(json!({"action":"published","path":path,"sha256":digest,"length":length}))
    }
    pub fn published_tree_member(
        &self,
        producer: &str,
        path: &str,
        member: &str,
        digest: &str,
        length: u64,
    ) -> Result<bool, String> {
        self.session.fact(json!({"action":"member","producer":producer,"path":path,"member":member,"sha256":digest,"length":length}))
    }
    pub fn consume(&self, id: &str) -> Result<(), String> {
        self.session.fact(json!({"action":"consume","id":id}))
    }
    pub fn release(&self, role: &str) -> Result<(), String> {
        self.session
            .fact(json!({"action":"release-role","role":role}))
    }
    pub fn dependencies_paths(&self, path: &str, originals: &[String]) -> Result<(), String> {
        self.session
            .query(json!({"operation":"dependencies","path":path,"originals":originals}))
    }
    pub fn plan_rooted(
        self,
        producer: &str,
        path: &str,
        intent: Value,
        role: &str,
    ) -> Result<Publication, String> {
        #[derive(Deserialize)]
        struct Started {
            id: String,
        }
        let reply:Started=self.session.query(json!({"operation":"begin-directory","path":path,"producer":producer,"reference":role,"intent":intent,"rooted":true}))?;
        Ok(Publication {
            dir: self.root.join(path),
            relative: path.into(),
            session: self.session,
            id: reply.id,
        })
    }
}
pub struct Publication {
    dir: PathBuf,
    relative: String,
    pub session: Session,
    id: String,
}
impl Publication {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn create_directory(&self, path: &Path) -> Result<(), String> {
        self.session.query(json!({"operation":"create-directory-member","path":self.relative,"name":path.strip_prefix(&self.dir).map_err(|e|e.to_string())?}))
    }
    pub fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let target: PathBuf = self.session.query(
            json!({"operation":"reserve-directory-file","path":self.relative,
            "name":path.strip_prefix(&self.dir).map_err(|e|e.to_string())?,"length":bytes.len()}),
        )?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())
    }
    pub fn complete_sealed(self, outcome: Value) -> Result<(), String> {
        self.session
            .query(json!({"operation":"seal-directory","path":self.relative,"outcome":outcome}))
    }
    pub fn execute(
        &self,
        member: &str,
        command: &CommandSpec,
        sha256: &str,
    ) -> Result<Value, String> {
        self.session.query(json!({"operation":"execute-retained","path":self.relative,"member":member,"command":command,"expected_sha256":sha256}))
    }
}
pub fn publish_final(
    root: &Path,
    producer: &str,
    path: &str,
    raw: &[u8],
    outcome: Value,
) -> Result<bool, String> {
    let Some(session) = Session::scope(root)? else {
        return Ok(false);
    };
    session.fact(json!({"action":"final-original","producer":producer,"path":path,"text":std::str::from_utf8(raw).map_err(|e|e.to_string())?,"outcome":outcome}))
}
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_seconds: u64,
    pub output_limit_bytes: usize,
}
impl CommandSpec {
    pub fn validate(&self) -> Result<(), String> {
        if self.program.is_empty()
            || self.program.contains('\0')
            || self.args.iter().any(|a| a.contains('\0'))
            || self
                .env
                .iter()
                .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
            || self.timeout_seconds == 0
            || self.output_limit_bytes == 0
            || self.output_limit_bytes > 67_108_864
        {
            return Err("invalid command/bounds".into());
        }
        Ok(())
    }
    pub fn program(&self, root: &Path) -> Result<PathBuf, String> {
        let p = Path::new(&self.program);
        let search = self.env.get("PATH").map(String::as_str).unwrap_or("");
        let found = if p.is_absolute() {
            Some(p.to_owned())
        } else if self.program.contains('/') {
            Some(root.join(p))
        } else {
            std::env::split_paths(search)
                .map(|dir| root.join(dir).join(p))
                .find(|path| path.is_file())
        };
        found
            .filter(|path| path.is_file())
            .ok_or_else(|| format!("executable not found: {}", self.program))
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Original {
    pub path: String,
    pub sha256: String,
}
pub fn read_original(root: &Path, o: &Original) -> Result<Vec<u8>, String> {
    let text: String =
        Session::open(root)?.query(json!({"operation":"read-original","original":o}))?;
    Ok(text.into_bytes())
}
pub fn identity(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(format!("{}:{}:directory", meta.dev(), meta.ino()))
}
pub fn unique_path(store: &Path) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(store.join(format!(
        "attempt-{}-{now}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )))
}
