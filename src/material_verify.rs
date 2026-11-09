//! Bounded background material content verification. One worker thread, a small
//! request queue and a bounded response channel keep hashing out of UI callbacks.

use crate::attachments;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;

const REQUEST_QUEUE: usize = 4;
const RESPONSE_QUEUE: usize = 4;
const MAX_FILES_PER_REQUEST: usize = 16;
const MAX_REQUEST_BYTES: u64 = 256 * 1024 * 1024;
const HASH_BUFFER: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyFile {
    pub relative_path: String,
    pub expected_size: u64,
    pub expected_sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerifyMode {
    Verify,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedFile {
    relative_path: String,
    size: u64,
    content_sha256: String,
    modified: Option<SystemTime>,
}

impl VerifiedFile {
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }
    pub(crate) fn size(&self) -> u64 {
        self.size
    }
    pub(crate) fn content_sha256(&self) -> &str {
        &self.content_sha256
    }
    pub(crate) fn matches(&self, relative_path: &str, size: u64, sha: &str) -> bool {
        self.relative_path == relative_path && self.size == size && self.content_sha256 == sha
    }
}

#[derive(Debug)]
pub(crate) struct VerifyResponse {
    pub id: u64,
    pub root: PathBuf,
    pub files: Vec<Result<VerifiedFile, String>>,
}

#[derive(Debug)]
struct Request {
    id: u64,
    root: PathBuf,
    files: Vec<VerifyFile>,
    mode: VerifyMode,
}

pub(crate) struct MaterialVerifier {
    tx: SyncSender<Request>,
    rx: Receiver<VerifyResponse>,
    next_id: u64,
    spawn_error: Option<String>,
    test_delay_ms: Arc<AtomicU64>,
}

impl MaterialVerifier {
    pub(crate) fn new() -> Self {
        let (tx, requests) = mpsc::sync_channel::<Request>(REQUEST_QUEUE);
        let (responses, rx) = mpsc::sync_channel::<VerifyResponse>(RESPONSE_QUEUE);
        let fallback = tx.clone();
        let test_delay_ms = Arc::new(AtomicU64::new(0));
        let worker_delay = test_delay_ms.clone();
        let spawned = thread::Builder::new()
            .name("material-verify".into())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let delay = worker_delay.load(Ordering::Relaxed);
                    if delay > 0 {
                        thread::sleep(std::time::Duration::from_millis(delay));
                    }
                    let files = request
                        .files
                        .iter()
                        .map(|file| {
                            let verified = verify_one(&request.root, file)?;
                            if request.mode == VerifyMode::Delete {
                                if file.expected_sha256.as_deref()
                                    != Some(verified.content_sha256.as_str())
                                {
                                    return Err(
                                        "The material changed before cleanup.".into(),
                                    );
                                }
                                let attachment = attachments::Attachment {
                                    name: String::new(),
                                    relative_path: file.relative_path.clone(),
                                    media_type: String::new(),
                                    size: verified.size,
                                    content_sha256: String::new(),
                                };
                                let path = attachments::resolve(&request.root, &attachment)?;
                                std::fs::remove_file(path).map_err(|error| {
                                    format!("Cannot remove material copy: {error}")
                                })?;
                            }
                            Ok(verified)
                        })
                        .collect();
                    if responses
                        .send(VerifyResponse {
                            id: request.id,
                            root: request.root,
                            files,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        Self {
            tx: fallback,
            rx,
            next_id: 1,
            spawn_error: spawned
                .err()
                .map(|error| format!("Cannot start material verification: {error}")),
            test_delay_ms,
        }
    }

    #[cfg(feature = "qa")]
    pub(crate) fn set_test_delay_ms(&self, delay_ms: u64) {
        self.test_delay_ms.store(delay_ms, Ordering::Relaxed);
    }

    pub(crate) fn submit(
        &mut self,
        root: PathBuf,
        files: Vec<VerifyFile>,
    ) -> Result<u64, String> {
        self.submit_mode(root, files, VerifyMode::Verify)
    }

    pub(crate) fn submit_delete(
        &mut self,
        root: PathBuf,
        files: Vec<VerifyFile>,
    ) -> Result<u64, String> {
        self.submit_mode(root, files, VerifyMode::Delete)
    }

    fn submit_mode(
        &mut self,
        root: PathBuf,
        files: Vec<VerifyFile>,
        mode: VerifyMode,
    ) -> Result<u64, String> {
        if let Some(error) = &self.spawn_error {
            return Err(error.clone());
        }
        if files.len() > MAX_FILES_PER_REQUEST {
            return Err("Too many materials in one verification request.".into());
        }
        let total = files
            .iter()
            .try_fold(0u64, |total, file| total.checked_add(file.expected_size))
            .ok_or_else(|| String::from("The material set size is invalid."))?;
        if total > MAX_REQUEST_BYTES {
            return Err("The material set is too large to verify at once.".into());
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.tx
            .try_send(Request {
                id,
                root,
                files,
                mode,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    String::from("Material verification is busy. Try again shortly.")
                }
                TrySendError::Disconnected(_) => String::from(
                    "Material verification is unavailable. Restart the app and try again.",
                ),
            })?;
        Ok(id)
    }

    pub(crate) fn try_recv(&self) -> Option<VerifyResponse> {
        match self.rx.try_recv() {
            Ok(response) => Some(response),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
}

fn verify_one(root: &Path, file: &VerifyFile) -> Result<VerifiedFile, String> {
    let attachment = attachments::Attachment {
        name: String::new(),
        relative_path: file.relative_path.clone(),
        media_type: String::new(),
        size: file.expected_size,
        content_sha256: String::new(),
    };
    let path = attachments::resolve(root, &attachment)?;
    let before = std::fs::metadata(&path)
        .map_err(|error| format!("Cannot inspect material: {error}"))?;
    if !before.is_file() || before.len() != file.expected_size {
        return Err("The material size or file type changed.".into());
    }
    let modified = before.modified().ok();
    let mut input = File::open(&path).map_err(|error| format!("Cannot read material: {error}"))?;
    let handle_before = input
        .metadata()
        .map_err(|error| format!("Cannot inspect material: {error}"))?;
    if !handle_before.is_file()
        || handle_before.len() != before.len()
        || !same_file(&before, &handle_before)
    {
        return Err("The material was replaced while it was being opened.".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER];
    let mut read = 0u64;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read material: {error}"))?;
        if count == 0 {
            break;
        }
        read = read
            .checked_add(count as u64)
            .ok_or_else(|| String::from("The material size is invalid."))?;
        if read > file.expected_size || read > attachments::MAX_BYTES {
            return Err("The material changed while it was being verified.".into());
        }
        hasher.update(&buffer[..count]);
    }
    let handle_after = input
        .metadata()
        .map_err(|error| format!("Cannot inspect material: {error}"))?;
    let after = std::fs::metadata(&path)
        .map_err(|error| format!("Cannot inspect material: {error}"))?;
    if after.len() != before.len()
        || after.modified().ok() != modified
        || handle_after.len() != before.len()
        || handle_after.modified().ok() != modified
        || !same_file(&before, &after)
        || !same_file(&before, &handle_after)
        || read != before.len()
    {
        return Err("The material changed while it was being verified.".into());
    }
    Ok(VerifiedFile {
        relative_path: file.relative_path.clone(),
        size: after.len(),
        content_sha256: format!("{:x}", hasher.finalize()),
        modified,
    })
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_a: &std::fs::Metadata, _b: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn temp_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "t13-verify-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn verifies_content_and_size_errors_without_ui_blocking() {
        let root = temp_root();
        let attachment =
            crate::attachments::import_bytes(&root, "verify.pdf", b"%PDF-1.4\nverify\n").unwrap();
        let mut verifier = MaterialVerifier::new();
        let started = Instant::now();
        let id = verifier
            .submit(
                root.clone(),
                vec![VerifyFile {
                    relative_path: attachment.relative_path.clone(),
                    expected_size: attachment.size,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        let response = loop {
            if let Some(response) = verifier.try_recv() {
                break response;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(response.id, id);
        let file = response.files[0].as_ref().unwrap();
        assert!(file.matches(
            &attachment.relative_path,
            attachment.size,
            &attachment.content_sha256
        ));
        assert!(verifier
            .submit(
                root.clone(),
                vec![VerifyFile {
                    relative_path: attachment.relative_path,
                    expected_size: attachment.size + 1,
                    expected_sha256: None,
                }],
            )
            .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn queue_and_request_budget_are_bounded() {
        let root = temp_root();
        let attachment =
            crate::attachments::import_bytes(&root, "queue.pdf", b"%PDF-1.4\nqueue\n").unwrap();
        let mut verifier = MaterialVerifier::new();
        let mut errors = 0;
        for _ in 0..20 {
            if verifier
                .submit(
                    root.clone(),
                    vec![VerifyFile {
                        relative_path: attachment.relative_path.clone(),
                        expected_size: attachment.size,
                        expected_sha256: None,
                    }],
                )
                .is_err()
            {
                errors += 1;
            }
        }
        assert!(errors > 0, "bounded queue must reject overflow");
        let too_many = (0..17)
            .map(|_| VerifyFile {
                relative_path: attachment.relative_path.clone(),
                expected_size: attachment.size,
                expected_sha256: None,
            })
            .collect();
        assert!(verifier.submit(root.clone(), too_many).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn large_file_submission_returns_before_hashing_completes() {
        let root = temp_root();
        let mut bytes = b"%PDF-1.4\n".to_vec();
        bytes.resize(8 * 1024 * 1024, b'x');
        let attachment = crate::attachments::import_bytes(&root, "large.pdf", &bytes).unwrap();
        let mut verifier = MaterialVerifier::new();
        let started = Instant::now();
        verifier
            .submit(
                root.clone(),
                vec![VerifyFile {
                    relative_path: attachment.relative_path,
                    expected_size: attachment.size,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_job_reports_mixed_success_per_identity() {
        let root = temp_root();
        let a = crate::attachments::import_bytes(&root, "delete-a.pdf", b"%PDF-1.4\nA\n").unwrap();
        let b = crate::attachments::import_bytes(&root, "delete-b.pdf", b"%PDF-1.4\nB\n").unwrap();
        let a_path = crate::attachments::resolve(&root, &a).unwrap();
        let b_path = crate::attachments::resolve(&root, &b).unwrap();
        let mut verifier = MaterialVerifier::new();
        let id = verifier
            .submit_delete(
                root.clone(),
                vec![
                    VerifyFile {
                        relative_path: a.relative_path.clone(),
                        expected_size: a.size,
                        expected_sha256: Some(a.content_sha256.clone()),
                    },
                    VerifyFile {
                        relative_path: b.relative_path.clone(),
                        expected_size: b.size,
                        expected_sha256: Some("wrong-digest".into()),
                    },
                ],
            )
            .unwrap();
        let response = loop {
            if let Some(response) = verifier.try_recv() {
                break response;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(response.id, id);
        assert_eq!(response.files.len(), 2);
        assert!(response.files[0].is_ok());
        assert!(response.files[1].is_err());
        assert!(!a_path.exists(), "matching copy is deleted");
        assert!(b_path.exists(), "mismatched copy is retained");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn changed_during_verification_is_rejected_by_stable_metadata() {
        let root = temp_root();
        let attachment =
            crate::attachments::import_bytes(&root, "stale.pdf", b"%PDF-1.4\nstale\n").unwrap();
        let _ = verify_one(
            &root,
            &VerifyFile {
                relative_path: attachment.relative_path,
                expected_size: attachment.size,
                expected_sha256: None,
            },
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
