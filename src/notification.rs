//! Durable completion notifications sent through yooooo-message-hub.
//!
//! Credentials stay outside the workspace: either SCF-style environment
//! variables or the existing yooooo-notifier client config. The workspace only
//! stores the user-enabled state, destination choice and delivery outbox.

use crate::domain::Store;
use crate::report_publish::Publication;
use crate::app::TaskApp;
use gpui_kit::Context;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub(crate) const JOB_WAITING: &str = "Waiting for publication";
pub(crate) const JOB_PENDING: &str = "Pending";
pub(crate) const JOB_SENDING: &str = "Sending";
pub(crate) const JOB_DELIVERED: &str = "Delivered";
pub(crate) const JOB_FAILED: &str = "Failed";
pub(crate) const JOB_NEEDS_REVIEW: &str = "Needs review";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub enabled: bool,
    #[serde(default)]
    pub destination: String,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { enabled: true, destination: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationJob {
    pub id: String,
    pub topic: u64,
    #[serde(default)]
    pub occurrence: Option<u64>,
    pub status: String,
    pub title: String,
    pub summary: String,
    pub fingerprint: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub sent_chunks: u32,
    #[serde(default)]
    pub last_error: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NotificationOutcome {
    pub(crate) status: String,
    pub(crate) sent_chunks: u32,
    pub(crate) error: String,
}

pub(crate) enum NotificationEvent {
    Complete(NotificationOutcome),
}

impl NotificationOutcome {
    fn delivered() -> Self {
        Self { status: JOB_DELIVERED.into(), sent_chunks: 0, error: String::new() }
    }

    fn failed(error: impl Into<String>, sent_chunks: u32) -> Self {
        Self { status: JOB_FAILED.into(), sent_chunks, error: error.into() }
    }
}

#[derive(Debug, Deserialize)]
struct ClientFileConfig {
    endpoint: Option<String>,
    client_id: Option<String>,
    hmac_key: Option<String>,
    default_destination: Option<String>,
}

#[derive(Clone, Debug)]
struct ClientCredentials {
    endpoint: String,
    client_id: String,
    hmac_key: String,
    default_destination: String,
}

fn notification_config_path() -> PathBuf {
    std::env::var_os("YOOOOO_NOTIFY_CONFIG")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/yooooo-notifier/client.json")))
        .unwrap_or_else(|| PathBuf::from(".config/yooooo-notifier/client.json"))
}

fn load_credentials() -> Result<ClientCredentials, String> {
    let file = fs::read_to_string(notification_config_path()).ok()
        .and_then(|raw| serde_json::from_str::<ClientFileConfig>(&raw).ok());
    let file_value = |name: &str| -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.trim().is_empty()).or_else(|| {
            file.as_ref().and_then(|config| match name {
                "YOOOOO_NOTIFY_URL" => config.endpoint.clone(),
                "YOOOOO_NOTIFY_CLIENT_ID" => config.client_id.clone(),
                "YOOOOO_NOTIFY_HMAC_KEY" => config.hmac_key.clone(),
                _ => None,
            })
        })
    };
    let endpoint = file_value("YOOOOO_NOTIFY_URL").unwrap_or_default().trim().trim_end_matches('/').to_string();
    if !endpoint.starts_with("https://") || endpoint.len() <= "https://".len() {
        return Err("Notification endpoint is missing. Set YOOOOO_NOTIFY_URL or configure the shared client file.".into());
    }
    let hmac_key = file_value("YOOOOO_NOTIFY_HMAC_KEY").unwrap_or_default();
    if hmac_key.len() < 32 {
        return Err("Notification HMAC key is missing or too short. Set YOOOOO_NOTIFY_HMAC_KEY or configure the shared client file.".into());
    }
    let client_id = file_value("YOOOOO_NOTIFY_CLIENT_ID").unwrap_or_else(|| "planning-app".into());
    let default_destination = std::env::var("YOOOOO_NOTIFY_DESTINATION").ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| file.as_ref().and_then(|config| config.default_destination.clone()))
        .unwrap_or_default();
    if default_destination.trim().is_empty() {
        return Err("Notification destination is missing. Set it in Settings or configure the shared client file.".into());
    }
    Ok(ClientCredentials { endpoint, client_id, hmac_key, default_destination })
}

fn signature(key: &str, client: &str, timestamp: i64, nonce: &str, body: &[u8]) -> String {
    let digest = sha2::Sha256::digest(body);
    let body_hash = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let canonical = format!("v1\n{client}\n{timestamp}\n{nonce}\n{body_hash}");
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(canonical.as_bytes());
    mac.finalize().into_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn random_nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|_| "Could not generate a secure notification nonce.".to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn config_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_private_temp(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("planning-notify-{}-{name}", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)
        .map_err(|error| format!("Could not create a private notification file: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    file.write_all(bytes).map_err(|error| format!("Could not write a notification file: {error}"))?;
    file.flush().map_err(|error| format!("Could not flush a notification file: {error}"))?;
    Ok(path)
}

pub(crate) fn send(job: &NotificationJob, config: &NotificationConfig) -> Result<NotificationOutcome, String> {
    let credentials = load_credentials()?;
    let destination = if config.destination.trim().is_empty() {
        credentials.default_destination.clone()
    } else {
        config.destination.trim().to_string()
    };
    let body = notification_payload(job, &destination)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is before the Unix epoch.".to_string())?
        .as_secs() as i64;
    let nonce = random_nonce()?;
    let signed = signature(&credentials.hmac_key, &credentials.client_id, timestamp, &nonce, &body);
    let body_path = write_private_temp("body.json", &body)?;
    let curl_config = format!(
        "url = \"{}\"\nrequest = \"POST\"\nsilent\nshow-error\nlocation\nmax-time = 50\nproto = \"=https\"\nproto-redir = \"=https\"\nwrite-out = \"\\n%{{http_code}}\"\nheader = \"Content-Type: application/json; charset=utf-8\"\nheader = \"User-Agent: planning-notifier/1.0.0\"\nheader = \"X-Notify-Client: {}\"\nheader = \"X-Notify-Timestamp: {}\"\nheader = \"X-Notify-Nonce: {}\"\nheader = \"X-Notify-Signature: v1={}\"\ndata-binary = \"@{}\"\n",
        config_quote(&credentials.endpoint),
        config_quote(&credentials.client_id),
        timestamp,
        nonce,
        signed,
        config_quote(&body_path.to_string_lossy()),
    );
    let config_path = write_private_temp("curl.config", curl_config.as_bytes())?;
    let output = Command::new("curl")
        .arg("--config").arg(&config_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not start the notification client: {error}"))?;
    let _ = fs::remove_file(&body_path);
    let _ = fs::remove_file(&config_path);
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let (raw_body, status_text) = text.rsplit_once('\n').unwrap_or((&text, "0"));
    let status = status_text.trim().parse::<u16>().unwrap_or(0);
    let parsed: serde_json::Value = serde_json::from_str(raw_body.trim()).unwrap_or(serde_json::Value::Null);
    let ok = parsed.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let delivered = parsed.get("status").and_then(serde_json::Value::as_str) == Some("delivered");
    let sent_chunks = parsed.get("sent_chunks").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
    if (200..300).contains(&status) && ok && delivered {
        return Ok(NotificationOutcome::delivered());
    }
    let reason = parsed.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown_error");
    let description = parsed.get("description").and_then(serde_json::Value::as_str).unwrap_or("");
    let detail = if description.is_empty() { reason.to_string() } else { format!("{reason}: {description}") };
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let suffix = if stderr.is_empty() { String::new() } else { format!(" ({stderr})") };
    Ok(NotificationOutcome::failed(
        format!("Notification service returned HTTP {status}: {detail}{suffix}"),
        sent_chunks,
    ))
}

pub(crate) fn publication_summary(markdown: &str) -> String {
    let mut lines = Vec::new();
    let mut chars = 0usize;
    for line in markdown.lines() {
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') || text.starts_with("<!--") {
            continue;
        }
        lines.push(text.to_string());
        chars += text.chars().count();
        if chars >= 900 {
            break;
        }
    }
    let mut summary = lines.join("\n");
    if summary.chars().count() > 1100 {
        summary = summary.chars().take(1097).collect::<String>() + "...";
    }
    summary
}

fn table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !(trimmed.starts_with('|') && trimmed.ends_with('|')) { return None; }
    Some(trimmed[1..trimmed.len() - 1].split('|')
        .map(|cell| cell.trim().replace("\\|", "|"))
        .collect())
}

fn is_table_separator(cells: &[String]) -> bool {
    !cells.is_empty() && cells.iter().all(|cell| {
        let value = cell.trim().trim_matches(':').trim();
        !value.is_empty() && value.chars().all(|character| character == '-')
    })
}

/// Telegram Rich Markdown renders links but not pipe tables. Convert report
/// tables to compact bullet lines before sending, leaving all other Markdown
/// text unchanged.
pub(crate) fn telegram_message_body(summary: &str, url: &str) -> String {
    let lines: Vec<&str> = summary.lines().collect();
    let mut output = String::new();
    let mut index = 0usize;
    while index < lines.len() {
        if table_cells(lines[index]).is_some() {
            let mut rows = Vec::new();
            while index < lines.len() {
                let Some(cells) = table_cells(lines[index]) else { break; };
                rows.push(cells);
                index += 1;
            }
            let headers = rows.first().cloned().unwrap_or_default();
            for cells in rows.iter().skip(1) {
                if is_table_separator(cells) { continue; }
                let mut parts = Vec::new();
                for (position, cell) in cells.iter().enumerate() {
                    let value = cell.trim();
                    if value.is_empty() || value == "—" { continue; }
                    if position == 0 {
                        parts.push(value.to_string());
                    } else if let Some(header) = headers.get(position) {
                        if header.trim().is_empty() { parts.push(value.to_string()); }
                        else { parts.push(format!("{} {}", header.trim(), value)); }
                    } else {
                        parts.push(value.to_string());
                    }
                }
                if !parts.is_empty() {
                    output.push_str("- ");
                    output.push_str(&parts.join("；"));
                    output.push('\n');
                }
            }
            if !rows.is_empty() { output.push('\n'); }
            continue;
        }
        let line = lines[index].trim_end();
        if !line.trim().is_empty() {
            output.push_str(line);
            output.push_str("\n\n");
        }
        index += 1;
    }
    let body = output.trim().to_string();
    if url.trim().is_empty() {
        body
    } else {
        format!("{body}\n\nOnline report: {}", url.trim())
    }
}

fn notification_payload(job: &NotificationJob, destination: &str) -> Result<Vec<u8>, String> {
    let body = telegram_message_body(&job.summary, &job.url);
    let title = format!("✅ **{}**", job.title.trim());
    serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "id": job.id,
        "provider": "client_payload",
        "destination": destination,
        "params": { "title": title, "body": body }
    }))
    .map_err(|error| format!("Could not encode notification: {error}"))
}

pub(crate) fn direct_fingerprint(occurrence: u64, final_run: u64, summary: &str) -> String {
    let input = format!("{occurrence}:{final_run}:{summary}");
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

impl Store {
    pub fn notification_config(&self) -> &NotificationConfig {
        self.notification_config_value.get()
    }

    pub fn notification_jobs(&self) -> &[NotificationJob] {
        &self.notification_jobs
    }

    pub fn notification_job_mut(&mut self, id: &str) -> Option<&mut NotificationJob> {
        let index = self.notification_jobs.iter().position(|job| job.id == id)?;
        self.notification_jobs.index_mut(index)
    }

    pub fn replace_notification_jobs(&mut self, jobs: Vec<NotificationJob>) {
        self.notification_jobs.replace_tracked(jobs);
    }

    pub(crate) fn enqueue_notification_job(
        &mut self,
        topic: u64,
        occurrence: Option<u64>,
        title: String,
        summary: String,
        fingerprint: String,
        now: i64,
    ) {
        if !self.notification_config().enabled || summary.trim().is_empty() { return; }
        let id = format!("topic-{}-{}", topic, &fingerprint[..fingerprint.len().min(16)]);
        if self.notification_jobs.iter().any(|job| job.id == id) { return; }
        self.notification_jobs.retain_tracked(|job| {
            job.topic != topic
                || job.fingerprint == fingerprint
                || !matches!(job.status.as_str(), JOB_WAITING | JOB_PENDING)
        });
        let cleaned_title = title.replace(['\n', '\r'], " ").trim().to_string();
        if cleaned_title.is_empty() { return; }
        self.notification_jobs.push_tracked(NotificationJob {
            id,
            topic,
            occurrence,
            status: JOB_WAITING.into(),
            title: cleaned_title,
            summary,
            fingerprint,
            url: String::new(),
            destination: self.notification_config().destination.trim().to_string(),
            attempts: 0,
            sent_chunks: 0,
            last_error: String::new(),
            created_at: now,
            updated_at: now,
        });
    }

    pub(crate) fn enqueue_direct_notification_job(
        &mut self,
        occurrence: u64,
        final_run: u64,
        title: String,
        summary: String,
        fingerprint: String,
        now: i64,
    ) {
        if !self.notification_config().enabled || summary.trim().is_empty() { return; }
        let id = format!("automation-{}-{}", occurrence, &fingerprint[..fingerprint.len().min(16)]);
        if self.notification_jobs.iter().any(|job| job.id == id) { return; }
        self.notification_jobs.retain_tracked(|job| {
            job.occurrence != Some(occurrence)
                || job.topic != 0
                || job.fingerprint == fingerprint
                || !matches!(job.status.as_str(), JOB_WAITING | JOB_PENDING)
        });
        let cleaned_title = title.replace(['\n', '\r'], " ").trim().to_string();
        if cleaned_title.is_empty() { return; }
        self.notification_jobs.push_tracked(NotificationJob {
            id,
            topic: 0,
            occurrence: Some(occurrence),
            status: JOB_PENDING.into(),
            title: cleaned_title,
            summary,
            fingerprint,
            url: String::new(),
            destination: self.notification_config().destination.trim().to_string(),
            attempts: 0,
            sent_chunks: 0,
            last_error: String::new(),
            created_at: now,
            updated_at: now,
        });
    }

    pub(crate) fn activate_notification_jobs(&mut self, publication: &Publication, now: i64) {
        if publication.status != "Online" { return; }
        for index in 0..self.notification_jobs.len() {
            let matches = {
                let job = &self.notification_jobs[index];
                job.topic == publication.topic
                    && job.fingerprint == publication.fingerprint
                    && job.status == JOB_WAITING
            };
            if matches {
                let job = self.notification_jobs.index_mut(index).expect("notification job index");
                job.status = JOB_PENDING.into();
                job.url = publication.url();
                job.last_error.clear();
                job.updated_at = now;
            }
        }
    }

    pub(crate) fn recover_notification_jobs(&mut self, now: i64) {
        for index in 0..self.notification_jobs.len() {
            if self.notification_jobs[index].status != JOB_SENDING { continue; }
            let job = self.notification_jobs.index_mut(index).expect("notification job index");
            job.status = JOB_NEEDS_REVIEW.into();
            job.last_error = "The app stopped before delivery was confirmed. Review and send this notification again.".into();
            job.updated_at = now;
        }
    }
}

impl TaskApp {
    pub(crate) fn recover_notifications(&mut self) {
        self.store.recover_notification_jobs(chrono::Utc::now().timestamp());
    }

    pub(crate) fn retry_failed_notifications(&mut self, cx: &mut Context<Self>) -> usize {
        let now = chrono::Utc::now().timestamp();
        let mut retried = 0usize;
        for index in 0..self.store.notification_jobs().len() {
            if !matches!(self.store.notification_jobs()[index].status.as_str(), JOB_FAILED | JOB_NEEDS_REVIEW) { continue; }
            let job = self.store.notification_jobs.index_mut(index).expect("notification job index");
            job.status = JOB_PENDING.into();
            job.last_error.clear();
            job.updated_at = now;
            retried += 1;
        }
        if retried > 0 { self.start_pump(cx); }
        retried
    }

    pub(crate) fn poll_notifications(&mut self, cx: &mut Context<Self>) {
        let event = self.notification_job.as_ref().and_then(|(id, rx)| match rx.try_recv() {
            Ok(event) => Some((id.clone(), event)),
            Err(mpsc::TryRecvError::Disconnected) => Some((id.clone(), NotificationEvent::Complete(
                NotificationOutcome::failed("Notification worker stopped.", 0),
            ))),
            Err(mpsc::TryRecvError::Empty) => None,
        });
        if let Some((id, NotificationEvent::Complete(outcome))) = event {
            self.notification_job = None;
            if let Some(job) = self.store.notification_job_mut(&id) {
                job.attempts = job.attempts.saturating_add(1);
                job.updated_at = chrono::Utc::now().timestamp();
                job.sent_chunks = outcome.sent_chunks;
                if outcome.status == JOB_DELIVERED {
                    job.status = JOB_DELIVERED.into();
                    job.last_error.clear();
                    self.toast = format!("Notification delivered · {}", job.title);
                } else {
                    let retryable = outcome.sent_chunks == 0
                        && outcome.error.contains("HTTP 503")
                        && job.attempts < 3;
                    job.status = if retryable { JOB_PENDING } else if outcome.sent_chunks > 0 { JOB_NEEDS_REVIEW } else { JOB_FAILED }.into();
                    job.last_error = outcome.error;
                    self.toast = format!("Notification not delivered · {}", job.status);
                }
            }
            self.persist_if_changed(true);
            cx.notify();
        }
        if self.notification_job.is_some()
            || !self.store.notification_config().enabled
            || self.live_can_accept_auxiliary().is_err()
        {
            return;
        }
        let Some(job) = self.store.notification_jobs().iter()
            .find(|job| job.status == JOB_PENDING)
            .cloned()
        else { return; };
        let id = job.id.clone();
        let config = self.store.notification_config().clone();
        {
            let Some(active) = self.store.notification_job_mut(&id) else { return; };
            active.status = JOB_SENDING.into();
            active.updated_at = chrono::Utc::now().timestamp();
        }
        self.persist_if_changed(true);
        if !self.storage_error.is_empty() {
            if let Some(active) = self.store.notification_job_mut(&id) {
                active.status = JOB_PENDING.into();
            }
            cx.notify();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.notification_job = Some((id, rx));
        std::thread::spawn(move || {
            let outcome = send(&job, &config).unwrap_or_else(|error| NotificationOutcome::failed(error, 0));
            let _ = tx.send(NotificationEvent::Complete(outcome));
        });
        self.start_pump(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_canonical_hmac_contract() {
        let body = b"{\"schema_version\":1}";
        assert_eq!(
            signature("0123456789abcdef0123456789abcdef", "planning-app", 1_700_000_000, "abcdef0123456789", body),
            "8c19e0bb036cb2c54843035886d5fd35cf473bc19858c54b2a9d15466fe5571d"
        );
    }

    #[test]
    fn summary_keeps_answer_lines_and_skips_headings() {
        let markdown = "# Topic\n\n## Goal\nDirect answer.\n\nSecond line.\n\n### Sources\n\n- Source <https://example.com>";
        assert_eq!(publication_summary(markdown), "Direct answer.\nSecond line.\n- Source <https://example.com>");
    }

    #[test]
    fn telegram_body_flattens_tables_and_keeps_urls() {
        let summary = "Answer first.\n| 标号 | 深圳 | 杭州 |\n|---|---|---|\n| 89号 | 7.79 | — |\n| 92号 | 8.39 | 8.34 |";
        let body = telegram_message_body(summary, "https://report.example.com/abc/");
        assert!(body.contains("Answer first."));
        assert!(body.contains("- 89号；深圳 7.79"));
        assert!(body.contains("\n- 89号；深圳 7.79\n- "));
        assert!(!body.contains('|'));
        assert!(body.contains("Online report: https://report.example.com/abc/"));
    }

    #[test]
    fn payload_does_not_repeat_title_in_body() {
        let job = NotificationJob {
            id: "job".into(), topic: 0, occurrence: Some(1), status: JOB_PENDING.into(),
            title: "Automation complete: Fuel".into(), summary: "Short answer".into(),
            fingerprint: "f".repeat(64), url: String::new(), destination: String::new(),
            attempts: 0, sent_chunks: 0, last_error: String::new(), created_at: 1, updated_at: 1,
        };
        let payload = notification_payload(&job, "telegram_default").unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let params = value.get("params").unwrap();
        assert_eq!(params.get("title").unwrap().as_str(), Some("✅ **Automation complete: Fuel**"));
        assert_eq!(params.get("body").unwrap().as_str(), Some("Short answer"));
    }

    #[test]
    fn jobs_dedupe_and_activate_only_after_online() {
        let mut store = Store::empty();
        store.enqueue_notification_job(7, Some(3), "Report".into(), "Answer".into(), "a".repeat(64), 100);
        store.enqueue_notification_job(7, Some(3), "Report".into(), "Answer".into(), "a".repeat(64), 100);
        assert_eq!(store.notification_jobs().len(), 1);
        let mut publication = Publication {
            id: "p".into(), topic: 7, fingerprint: "b".repeat(64),
            config: crate::report_publish::PublishingConfig::default(),
            title: "Report".into(), markdown: String::new(),
            status: "Online".into(), commit: Some("c".into()), message: String::new(),
        };
        store.activate_notification_jobs(&publication, 101);
        assert_eq!(store.notification_jobs()[0].status, JOB_WAITING);
        publication.fingerprint = "a".repeat(64);
        store.activate_notification_jobs(&publication, 102);
        assert_eq!(store.notification_jobs()[0].status, JOB_PENDING);
        assert_eq!(store.notification_jobs()[0].url, "https://report.krabs.wang/p/");
    }

    #[test]
    fn direct_automation_jobs_are_pending_without_publication() {
        let mut store = Store::empty();
        let fingerprint = direct_fingerprint(12, 34, "Short answer");
        store.enqueue_direct_notification_job(
            12, 34, "Automation complete: Daily check".into(), "Short answer".into(),
            fingerprint.clone(), 100,
        );
        store.enqueue_direct_notification_job(
            12, 34, "Automation complete: Daily check".into(), "Short answer".into(),
            fingerprint, 100,
        );
        assert_eq!(store.notification_jobs().len(), 1);
        let job = &store.notification_jobs()[0];
        assert_eq!(job.topic, 0);
        assert_eq!(job.occurrence, Some(12));
        assert_eq!(job.status, JOB_PENDING);
        assert!(job.url.is_empty());
    }
}
