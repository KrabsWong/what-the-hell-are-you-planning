//! Local OpenCode CLI adapter. The worker owns the child; GPUI only drains typed events.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Event {
    Log(String),
    Text(String),
    Session(String),
    BrowserBlocked(String),
    Finished {
        code: Option<i32>,
        cancelled: bool,
        error: Option<String>,
    },
}
#[derive(Clone, Debug)]
pub struct Request {
    pub executable: PathBuf,
    pub directory: PathBuf,
    pub model: String,
    pub prompt: String,
    pub title: String,
    pub files: Vec<PathBuf>,
    pub session_id: Option<String>,
}
pub struct Running {
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    done: Receiver<()>,
}
impl Running {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        kill_group(self.pid.load(Ordering::SeqCst));
    }
    pub fn stop_and_wait(&self, timeout: Duration) -> (bool, Vec<Event>) {
        self.cancel();
        let deadline = Instant::now() + timeout;
        let mut pending = vec![];
        loop {
            // Prevent a full UI event queue from blocking child cleanup during app exit.
            pending.extend(self.events.try_iter());
            match self.done.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => {
                    pending.extend(self.events.try_iter());
                    return (true, pending);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                kill_group(self.pid.load(Ordering::SeqCst));
                return (false, pending);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn timestamp() -> String {
    // Keep timestamps readable and preserve the originating UTC offset.
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}
pub fn detect_executable() -> String {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("opencode"))
                .collect()
        })
        .unwrap_or_default();
    paths.extend([
        PathBuf::from("/opt/homebrew/bin/opencode"),
        PathBuf::from("/usr/local/bin/opencode"),
    ]);
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".opencode/bin/opencode"));
    }
    paths
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "opencode".into())
}
pub fn valid_model(model: &str) -> bool {
    let Some((provider, name)) = model.split_once('/') else {
        return false;
    };
    !provider.is_empty() && !name.is_empty() && !model.chars().any(char::is_whitespace)
}
/// Persist this ID before launch, so even an exit before the first event can be resumed.
pub fn new_session_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "ses_tm{:x}{:x}{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(unix)]
pub(crate) fn lock_session(request: &Request) -> Result<Option<std::fs::File>, String> {
    use sha2::{Digest, Sha256};
    use std::os::unix::{fs::OpenOptionsExt, io::AsRawFd};
    let Some(session) = &request.session_id else {
        return Ok(None);
    };
    let mut hash = Sha256::new();
    hash.update(request.directory.as_os_str().as_encoded_bytes());
    hash.update([0]);
    hash.update(session.as_bytes());
    let path =
        std::env::temp_dir().join(format!("task-manager-session-{:x}.lock", hash.finalize()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| format!("Cannot lock OpenCode session: {e}"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("This OpenCode session is still running, possibly from an earlier app instance. Wait for it to finish before resuming.".into());
    }
    Ok(Some(file))
}

pub fn start(mut request: Request) -> Result<Running, String> {
    if !request.directory.is_absolute() || !request.directory.is_dir() {
        return Err("Choose an existing absolute project folder before running.".into());
    }
    request.directory = request
        .directory
        .canonicalize()
        .map_err(|e| format!("Cannot resolve project folder: {e}"))?;
    if !valid_model(&request.model) {
        return Err("Choose an OpenCode model in provider/model format.".into());
    }
    #[cfg(unix)]
    let session_lock = lock_session(&request)?;
    let (tx, rx) = mpsc::sync_channel(128);
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let pid = Arc::new(AtomicU32::new(0));
    let worker_pid = pid.clone();
    let (done_tx, done) = mpsc::channel();
    thread::Builder::new()
        .name("opencode-run".into())
        .spawn(move || {
            run_worker(
                request,
                worker_cancel,
                worker_pid,
                tx,
                #[cfg(unix)]
                session_lock,
            );
            let _ = done_tx.send(());
        })
        .map_err(|e| format!("Cannot start worker: {e}"))?;
    Ok(Running {
        events: rx,
        cancel,
        pid,
        done,
    })
}
fn command(executable: &Path) -> Command {
    let mut cmd = Command::new(executable);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}
fn kill_group(pid: u32) {
    #[cfg(unix)]
    if pid > 0 {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}
fn terminate(child: &mut Child) {
    kill_group(child.id());
    let _ = child.kill();
}
fn finish_reader<T>(reader: thread::JoinHandle<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !reader.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if reader.is_finished() {
        reader.join().ok()
    } else {
        None
    }
}
#[cfg(unix)]
fn inherit_session_lock(cmd: &mut Command, file: &std::fs::File) {
    use std::os::unix::{io::AsRawFd, process::CommandExt};
    let fd = file.as_raw_fd();
    // The CLI keeps the lease after a force-quit. A new app cannot resume the same
    // session while the orphan is still working. Normal group cleanup releases it.
    unsafe {
        cmd.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn run_worker(
    request: Request,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    tx: SyncSender<Event>,
    #[cfg(unix)] session_lock: Option<std::fs::File>,
) {
    if cancel.load(Ordering::SeqCst) {
        let _ = tx.send(Event::Finished {
            code: None,
            cancelled: true,
            error: None,
        });
        return;
    }
    let mut cmd = command(&request.executable);
    cmd.current_dir(&request.directory)
        .env("PWD", &request.directory)
        .args([
            "--standalone",
            "run",
            "--format",
            "json",
            "--model",
            &request.model,
            "--agent",
            "build",
            "--title",
            &request.title,
        ]);
    if let Some(session) = &request.session_id {
        cmd.arg("--session").arg(session);
    }
    #[cfg(unix)]
    if let Some(file) = &session_lock {
        inherit_session_lock(&mut cmd, file);
    }
    for file in &request.files {
        cmd.arg("--file").arg(file);
    }
    cmd.arg("--").arg(&request.prompt);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = tx.send(Event::Finished {
                code: None,
                cancelled: false,
                error: Some(format!("Cannot start OpenCode: {e}")),
            });
            return;
        }
    };
    pid.store(child.id(), Ordering::SeqCst);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_tx = tx.clone();
    let err_tx = tx.clone();
    let out_cancel = cancel.clone();
    let out_pid = pid.clone();
    let out = thread::spawn(move || read_events(stdout, false, out_tx, out_cancel, out_pid));
    let err_cancel = cancel.clone();
    let err_pid = pid.clone();
    let err = thread::spawn(move || read_events(stderr, true, err_tx, err_cancel, err_pid));
    let (status, cancelled) = loop {
        if cancel.load(Ordering::SeqCst) {
            terminate(&mut child);
            break (child.wait(), true);
        }
        match child.try_wait() {
            Ok(Some(status)) => break (Ok(status), false),
            Ok(None) => thread::sleep(Duration::from_millis(40)),
            Err(e) => {
                terminate(&mut child);
                let _ = child.wait();
                break (Err(e), false);
            }
        }
    };
    // A tool can leave descendants holding pipes after the CLI exits. Clean this owned group
    // and bound stream teardown so neither normal completion nor cancellation can hang the UI.
    kill_group(child.id());
    let streams_closed = finish_reader(out).is_some() & finish_reader(err).is_some();
    pid.store(0, Ordering::SeqCst);
    let (code, mut error) = match status {
        Ok(status) => (status.code(), None),
        Err(e) => (None, Some(e.to_string())),
    };
    if !streams_closed {
        error = Some("OpenCode output pipes did not close; stream cleanup timed out.".into());
    }
    let _ = tx.send(Event::Finished {
        code,
        cancelled,
        error,
    });
}
fn read_events(
    input: impl Read,
    stderr: bool,
    tx: SyncSender<Event>,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
) {
    let mut reader = BufReader::new(input);
    loop {
        // Bound a single diagnostic/event, including output produced by tools.
        let mut bytes = Vec::new();
        match reader
            .by_ref()
            .take(1024 * 1024)
            .read_until(b'\n', &mut bytes)
        {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&bytes);
                if stderr {
                    if !line.trim().is_empty()
                        && tx
                            .send(Event::Log(format!("[stderr] {}", clip(line.trim(), 8000))))
                            .is_err()
                    {
                        break;
                    }
                } else {
                    let events = parse_event(&line);
                    if events.iter().any(|e| matches!(e, Event::BrowserBlocked(_))) {
                        // Stop before waiting for the UI to drain its event queue.
                        cancel.store(true, Ordering::SeqCst);
                        kill_group(pid.load(Ordering::SeqCst));
                    }
                    for event in events {
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Event::Log(format!("[stream error] {e}")));
                break;
            }
        }
    }
}
pub fn parse_event(line: &str) -> Vec<Event> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return vec![Event::Log(clip(line, 8000))];
    };
    let mut events = vec![];
    if let Some(session) = value.get("sessionID").and_then(Value::as_str) {
        events.push(Event::Session(session.into()));
    }
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("event");
    let part = &value["part"];
    if matches!(kind, "tool_use" | "tool")
        && matches!(part["tool"].as_str(), Some("shell" | "bash"))
    {
        let command = part["state"]["input"]["command"].as_str().unwrap_or("");
        let args: Vec<_> = command.split_whitespace().collect();
        let ego_execution = args.windows(2).any(|args| {
            let executable = args[0].trim_matches(['\'', '"']);
            (executable == "ego-browser" || executable.ends_with("/ego-browser"))
                && args[1].trim_matches(['\'', '"']) == "nodejs"
        });
        let output = part["state"]["output"]
            .as_str()
            .or_else(|| part["state"]["error"].as_str())
            .unwrap_or("");
        // A browser-owned permission prompt is an explicit handoff, not a retryable fetch error.
        if ego_execution
            && output.trim_start().starts_with("Error: ")
            && output.contains(": A browser permission prompt")
            && output.contains("The user now controls this task space")
        {
            events.push(Event::BrowserBlocked(
                "ego(lite) needs your attention. Handle its browser permission prompt, then confirm before resuming the same research Space.".into(),
            ));
        }
    }
    let log = match kind {
        "text" => part
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "reasoning" => format!(
            "[reasoning] {}",
            part.get("text").and_then(Value::as_str).unwrap_or("")
        ),
        "step_start" => "[step] started".into(),
        "step_finish" => format!(
            "[step] {}",
            part.get("reason")
                .and_then(Value::as_str)
                .unwrap_or("finished")
        ),
        "tool_use" | "tool" => format!(
            "[tool: {}] {}\nInput: {}\n{}",
            part.get("tool")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            part["state"]["status"].as_str().unwrap_or(""),
            part["state"].get("input").unwrap_or(&Value::Null),
            part["state"]
                .get("output")
                .and_then(Value::as_str)
                .or_else(|| part["state"].get("error").and_then(Value::as_str))
                .unwrap_or("")
        ),
        "error" => format!("[error] {}", value.get("error").unwrap_or(&value)),
        _ => format!("[{kind}] {line}"),
    };
    if !log.is_empty() {
        events.push(Event::Log(clip(&log, 8000)));
    }
    if kind == "text"
        && let Some(text) = value["part"]["text"].as_str()
    {
        events.push(Event::Text(clip(text, 128_000)));
    }
    events
}
pub fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() > limit {
        format!(
            "{}\n… [truncated]",
            text.chars().take(limit).collect::<String>()
        )
    } else {
        text.into()
    }
}

pub fn discover(executable: &Path, directory: &Path) -> Result<(String, Vec<String>), String> {
    let version = capture(executable, directory, &["--version"])?;
    let raw = capture(executable, directory, &["models", "--standalone"])?;
    let mut models: Vec<_> = raw
        .lines()
        .map(str::trim)
        .filter(|line| valid_model(line))
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err("OpenCode returned no models. Configure a provider with opencode auth login, then refresh.".into());
    }
    Ok((version.trim().into(), models))
}
pub(crate) fn capture(
    executable: &Path,
    directory: &Path,
    args: &[&str],
) -> Result<String, String> {
    // OpenCode can exit before flushing large piped exports. A regular file
    // makes stdout writes synchronous; unlink it immediately to keep transcripts private.
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!("task-manager-capture-{}", new_session_id()));
    let mut out = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Cannot capture OpenCode output: {e}"))?;
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    let mut child = command(executable)
        .current_dir(directory)
        .env("PWD", directory)
        .args(args)
        .stdout(Stdio::from(out.try_clone().map_err(|e| e.to_string())?))
        .spawn()
        .map_err(|e| format!("Cannot start OpenCode: {e}"))?;
    let mut err = child.stderr.take().unwrap();
    let stderr = thread::spawn(move || {
        let mut text = String::new();
        let _ = Read::by_ref(&mut err)
            .take(256 * 1024)
            .read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                terminate(&mut child);
                let _ = child.wait();
                break Err("Local command timed out after 30 seconds.".to_string());
            }
            Err(e) => {
                terminate(&mut child);
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    kill_group(child.id());
    let err = finish_reader(stderr).ok_or("Local command diagnostics did not close.")?;
    let status = result?;
    if !status.success() {
        return Err(format!("Local command failed: {}", clip(err.trim(), 1000)));
    }
    const MAX_OUTPUT: u64 = 32 * 1024 * 1024;
    if out.metadata().map_err(|e| e.to_string())?.len() > MAX_OUTPUT {
        return Err("Local command output exceeds the 32 MiB capture limit.".into());
    }
    out.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut text = String::new();
    out.take(MAX_OUTPUT + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("Cannot read OpenCode output: {e}"))?;
    if text.len() as u64 > MAX_OUTPUT {
        return Err("Local command output exceeds the 32 MiB capture limit.".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_preserves_large_unicode_json_using_file_stdout() {
        let output = capture(
            Path::new("/bin/sh"),
            &std::env::temp_dir(),
            &["-c", r#"test -f /dev/stdout || exit 9; printf '{"text":"'; awk 'BEGIN { for (i=0; i<100000; i++) printf "研究结果" }'; printf '"}'"#],
        ).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["text"].as_str().unwrap(), "研究结果".repeat(100000));
        assert!(
            capture(
                Path::new("/bin/sh"),
                &std::env::temp_dir(),
                &["-c", "printf 'export failed' >&2; exit 3"]
            )
            .unwrap_err()
            .contains("export failed")
        );
    }

    #[test]
    fn browser_permission_handoff_is_detected_only_in_tool_results() {
        let message = "Error: task.listTabs: A browser permission prompt for location access has appeared.\nThe user now controls this task space. Wait for the user to handle the prompt.";
        let tool = serde_json::json!({"type":"tool_use","part":{"tool":"shell","state":{"status":"completed","input":{"command":"cd /tmp && /Users/test/.local/bin/ego-browser nodejs < /tmp/task.mjs 2>&1 | head -60"},"output":message}}});
        assert!(
            parse_event(&tool.to_string())
                .iter()
                .any(|e| matches!(e, Event::BrowserBlocked(_)))
        );
        let text = serde_json::json!({"type":"text","part":{"text":message}});
        assert!(
            !parse_event(&text.to_string())
                .iter()
                .any(|e| matches!(e, Event::BrowserBlocked(_)))
        );
        let guide = serde_json::json!({"type":"tool_use","part":{"tool":"shell","state":{"input":{"command":message},"output":"Read the guide and stop when a permission prompt appears."}}});
        assert!(
            !parse_event(&guide.to_string())
                .iter()
                .any(|e| matches!(e, Event::BrowserBlocked(_)))
        );
        for (name, command) in [("read", ""), ("shell", "cat /tmp/old-browser-error.log")] {
            let historical = serde_json::json!({"type":"tool_use","part":{"tool":name,"state":{"input":{"command":command},"output":message}}});
            assert!(
                !parse_event(&historical.to_string())
                    .iter()
                    .any(|e| matches!(e, Event::BrowserBlocked(_)))
            );
        }
    }

    #[cfg(unix)]
    fn fake_script(body: &str) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "task-manager-runner-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opencode");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        (dir, path)
    }
    #[cfg(unix)]
    #[test]
    fn explicit_sessions_resume_without_fork_and_exclude_duplicate_workers() {
        let (dir, executable) =
            fake_script("printf '%s\\n' \"$@\" > args.txt\necho ready\nsleep 20");
        let session = new_session_id();
        let request = Request {
            executable,
            directory: dir.clone(),
            model: "test/model".into(),
            prompt: "resume from persisted context".into(),
            title: "Test".into(),
            files: vec![],
            session_id: Some(session.clone()),
        };
        let first = start(request.clone()).unwrap();
        assert!(matches!(
            first.events.recv_timeout(Duration::from_secs(3)).unwrap(),
            Event::Log(text) if text == "ready"
        ));
        let args = std::fs::read_to_string(dir.join("args.txt")).unwrap();
        assert!(args.contains(&format!("--session\n{session}\n")));
        assert!(!args.contains("--continue"));
        assert!(!args.contains("--fork"));
        assert!(
            start(request.clone())
                .err()
                .unwrap()
                .contains("still running")
        );
        let mut independent = request.clone();
        independent.session_id = Some(new_session_id());
        let second = start(independent).unwrap();
        assert!(matches!(
            second.events.recv_timeout(Duration::from_secs(3)).unwrap(),
            Event::Log(text) if text == "ready"
        ));
        assert!(first.stop_and_wait(Duration::from_secs(3)).0);
        let resumed = start(request).unwrap();
        assert!(resumed.stop_and_wait(Duration::from_secs(3)).0);
        assert!(second.stop_and_wait(Duration::from_secs(3)).0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn child_retains_session_lock_when_parent_lease_is_dropped() {
        let (dir, executable) = fake_script("sleep 20 &\necho ready\nwait");
        let request = Request {
            executable: executable.clone(),
            directory: dir.canonicalize().unwrap(),
            model: "test/model".into(),
            prompt: "test".into(),
            title: "test".into(),
            files: vec![],
            session_id: Some(new_session_id()),
        };
        let lease = lock_session(&request).unwrap().unwrap();
        let mut cmd = command(&executable);
        inherit_session_lock(&mut cmd, &lease);
        let mut child = cmd.spawn().unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        drop(lease);
        assert!(lock_session(&request).is_err());
        terminate(&mut child);
        child.wait().unwrap();
        // Descendants inherit the same lease; process group termination closes them too.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if lock_session(&request).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "session lease remained after process cleanup"
            );
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn generated_session_ids_are_unique_and_have_opencode_prefix() {
        let a = new_session_id();
        let b = new_session_id();
        assert!(a.starts_with("ses_"));
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        assert_ne!(a, b);
    }

    #[cfg(unix)]
    #[test]
    fn streams_before_exit_and_cancels_its_process_group() {
        let (dir, executable) = fake_script(
            "printf '%s\\n' '{\"type\":\"text\",\"part\":{\"text\":\"live output\"}}'\nsleep 20",
        );
        let running = start(Request {
            executable,
            directory: dir.clone(),
            model: "test/model".into(),
            prompt: "$(must-not-run)".into(),
            title: "Test".into(),
            files: vec![],
            session_id: None,
        })
        .unwrap();
        let event = running.events.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(matches!(event,Event::Log(text)if text=="live output"));
        running.cancel();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Event::Finished { cancelled, .. } = running
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                assert!(cancelled);
                break;
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn reports_failed_process_exit() {
        let (dir, executable) = fake_script("echo 'provider unavailable' >&2\nexit 7");
        let running = start(Request {
            executable,
            directory: dir.clone(),
            model: "test/model".into(),
            prompt: "Test".into(),
            title: "Test".into(),
            files: vec![],
            session_id: None,
        })
        .unwrap();
        loop {
            if let Event::Finished {
                code, cancelled, ..
            } = running.events.recv_timeout(Duration::from_secs(3)).unwrap()
            {
                assert_eq!(code, Some(7));
                assert!(!cancelled);
                break;
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn inherited_output_pipes_do_not_keep_a_run_alive() {
        let (dir, executable) = fake_script(
            "test \"$PWD\" = \"$(pwd -P)\" || exit 9\nsleep 20 &\necho 'finished parent'\nexit 0",
        );
        let running = start(Request {
            executable,
            directory: dir.clone(),
            model: "test/model".into(),
            prompt: "test".into(),
            title: "test".into(),
            files: vec![],
            session_id: None,
        })
        .unwrap();
        loop {
            if let Event::Finished { code, .. } =
                running.events.recv_timeout(Duration::from_secs(3)).unwrap()
            {
                assert_eq!(code, Some(0));
                break;
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn shutdown_waits_for_cleanup_even_when_log_queue_is_full() {
        let (dir, executable) = fake_script("while :; do echo busy; done");
        let running = start(Request {
            executable,
            directory: dir.clone(),
            model: "test/model".into(),
            prompt: "test".into(),
            title: "test".into(),
            files: vec![],
            session_id: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(running.stop_and_wait(Duration::from_secs(3)).0);
        assert_eq!(running.pid.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn parses_native_events_and_keeps_unknown_output_visible() {
        let events = parse_event(r#"{"type":"text","sessionID":"ses_123","part":{"text":"你好"}}"#);
        assert!(matches!(&events[0],Event::Session(id)if id=="ses_123"));
        assert!(matches!(&events[1],Event::Log(text)if text=="你好"));
        assert!(
            matches!(&parse_event("plain diagnostic")[0],Event::Log(text)if text=="plain diagnostic")
        );
        assert!(
            matches!(&parse_event(r#"{"type":"error","error":{"message":"bad model"}}"#)[0],Event::Log(text)if text.contains("bad model"))
        );
    }
    #[test]
    fn models_require_an_explicit_provider() {
        assert!(!valid_model("Choose a model"));
        assert!(!valid_model("/model"));
        assert!(valid_model("provider/model#high"));
    }
    #[test]
    fn truncation_preserves_unicode() {
        assert_eq!(clip("测试完成", 2), "测试\n… [truncated]");
    }
}

/// Stop only the associated session, including work surviving the app process.
pub(crate) fn interrupt_session(executable: &Path, run: &crate::domain::AgentRun) -> Result<(), String> {
    let session = run.session_id.as_ref().ok_or("Run has no session")?;
    let response = capture(executable, Path::new(&run.directory),
        &["api", "session.interrupt", "--param", &format!("sessionID={session}"), "--param", "resume=false"])?;
    let value: serde_json::Value = serde_json::from_str(&response).map_err(|e| e.to_string())?;
    let data = value.get("data").unwrap_or(&value);
    let interrupted = data.get("interrupted").and_then(|v| v.as_bool())
        .ok_or("OpenCode did not acknowledge interruption")?;
    if !interrupted {
        // An idle server does not prove a detached CLI process has stopped.
        let request = Request {
            executable: executable.into(),
            directory: Path::new(&run.directory).canonicalize().map_err(|e| e.to_string())?,
            model: String::new(), prompt: String::new(), title: String::new(),
            files: vec![], session_id: Some(session.clone()),
        };
        let _lease = lock_session(&request)?;
    }
    Ok(())
}
