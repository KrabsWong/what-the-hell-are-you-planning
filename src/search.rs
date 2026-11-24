//! Search-first collection prompts and the standalone MCP runtime probe.
//!
//! The browser bridge remains available as a legacy fallback, but a configured
//! `planning-search-mcp` makes search + fetch the default collection path.

use std::{env, fs, io::Write, path::{Path, PathBuf}, process::Command};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Environment variables win so a server deployment needs no UI setup.
pub const API_KEY_ENV: &str = "DEEPSEEK_API_KEY";
pub const CREDENTIALS_PATH_ENV: &str = "PLANNING_SEARCH_CREDENTIALS";

pub fn configured(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty() && Path::new(path).is_file()
}

/// Search and fetch are the baseline. ego(lite) is an opt-in advanced capability
/// for pages that need a signed-in session, interaction or human handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionRuntime {
    Search,
    Browser,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionPlan {
    pub runtime: CollectionRuntime,
    pub browser_enabled: bool,
}

pub fn collection_plan(search_path: &str, ego_path: &str, ego_lite_enabled: bool) -> CollectionPlan {
    let browser_enabled = ego_lite_enabled && crate::ego::executable(ego_path).is_ok();
    let runtime = if configured(search_path) {
        CollectionRuntime::Search
    } else if browser_enabled {
        CollectionRuntime::Browser
    } else {
        CollectionRuntime::Unavailable
    };
    CollectionPlan { runtime, browser_enabled }
}

fn credentials_path() -> PathBuf {
    env::var_os(CREDENTIALS_PATH_ENV)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| {
            PathBuf::from(home).join(".config/planning-search/credentials.json")
        }))
        .unwrap_or_else(|| PathBuf::from(".config/planning-search/credentials.json"))
}

fn read_key(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let key = value.get("api_key")?.as_str()?.trim().to_owned();
    (!key.is_empty()).then_some(key)
}

fn write_key(path: &Path, key: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("Cannot create credentials folder: {error}"))?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| format!("Cannot open credentials file: {error}"))?;
    let body = serde_json::json!({ "api_key": key.trim() }).to_string();
    file.write_all(body.as_bytes()).map_err(|error| format!("Cannot write credentials file: {error}"))
}

/// The key shown in Settings is stored outside the workspace, never in saved research data.
pub fn api_key() -> Option<String> {
    env::var(API_KEY_ENV).ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .or_else(|| read_key(&credentials_path()))
}

pub fn save_api_key(key: &str) -> Result<(), String> {
    let path = credentials_path();
    if key.trim().is_empty() {
        if path.exists() {
            fs::remove_file(&path).map_err(|error| format!("Cannot remove credentials file: {error}"))?;
        }
        return Ok(());
    }
    write_key(&path, key.trim())
}

pub fn credentials_location() -> String {
    credentials_path().to_string_lossy().into_owned()
}

pub fn probe(path: &str) -> Result<String, String> {
    if !configured(path) {
        return Err("planning-search-mcp is not installed at this path. Install the search MCP binary and select it in Settings.".into());
    }
    let mut command = Command::new(path.trim());
    command.arg("--status");
    if let Some(key) = api_key() {
        command.env(API_KEY_ENV, key);
    }
    let output = command.output()
        .map_err(|error| format!("Cannot start planning-search-mcp: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if output.status.success() {
        return Ok(format!("Search ready · {stdout}"));
    }
    let reason = if stderr.is_empty() { stdout } else { stderr };
    Err(format!("Search runtime needs attention: {reason}"))
}

pub fn instructions(agent: &str) -> String {
    let transport = if agent == "Codex" {
        "Use the planning_search MCP server: call its search tool with a query to discover sources, then its fetch tool with an original URL to verify the page. The tools return structured sources with URLs, titles, fetched Markdown content, publication dates and authority."
    } else {
        "Use your configured search tools to discover sources, then fetch the original URL to verify the page. If a planning_search MCP server is available, prefer its search and fetch tools; they return structured sources with URLs, titles, fetched Markdown content, publication dates and authority."
    };
    format!(
        "\nSEARCH-FIRST COLLECTION\n{transport}\nTreat every search result as discovery only. Before recording a finding, fetch its original URL and take the headline, publisher, publication date/time/timezone (or explicitly unknown), event date, retrieval time, excerpt and relevance from the fetched page. Keep source titles, URLs and quotation marks unchanged. Keep original quotations separate from your explanation. A snippet alone is not a verified source; do not claim page access unless you actually fetched it.\nSOURCE FAILURE POLICY: A login wall, CAPTCHA/device verification, paywall, HTTP error, timeout or unavailable content is a skipped source, not a workflow blocker. Record the URL and reason, stop accessing that source, and seek independent accessible sources. Do not solve or bypass an access restriction. Do not hand off a whole research goal because one website is unavailable. Bound retries and report remaining evidence gaps rather than stopping. Browser escalation is only for a source that cannot be verified any other way and requires explicit user confirmation; never use another browser, background fetching or a new Space to bypass a handoff.\nIndependent evidence matters more than URL count. Deduplicate syndications, flag unverifiable dates, distinguish stated intent from actual behavior, and return partial results with explicit gaps instead of fabricated news.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runtime_is_actionable() {
        let error = probe("/no/such/planning-search-mcp").unwrap_err();
        assert!(error.contains("planning-search-mcp"));
    }

    #[test]
    fn instructions_keep_the_source_failure_policy() {
        let text = instructions("OpenCode");
        assert!(text.contains("SEARCH-FIRST COLLECTION"));
        assert!(text.contains("snippet alone is not a verified source"));
        assert!(text.contains("explicit user confirmation"));
        assert!(!text.contains("ego"));
    }

    #[test]
    fn codex_instructions_name_the_mcp_tools() {
        let text = instructions("Codex");
        assert!(text.contains("planning_search MCP"));
        assert!(text.contains("fetch tool"));
    }

    #[test]
    fn browser_capability_requires_an_explicit_opt_in() {
        let path = std::env::temp_dir().join(format!(
            "planning-search-runtime-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::write(&path, "fake").unwrap();
        let path = path.to_string_lossy().into_owned();
        assert_eq!(
            collection_plan(&path, &path, false),
            CollectionPlan { runtime: CollectionRuntime::Search, browser_enabled: false }
        );
        let advanced = collection_plan(&path, &path, true);
        assert_eq!(advanced.runtime, CollectionRuntime::Search);
        assert!(advanced.browser_enabled);
        assert_eq!(
            collection_plan("/no/such/mcp", &path, false),
            CollectionPlan { runtime: CollectionRuntime::Unavailable, browser_enabled: false }
        );
        assert_eq!(
            collection_plan("/no/such/mcp", &path, true).runtime,
            CollectionRuntime::Browser
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn credentials_round_trip_outside_the_workspace() {
        let path = std::env::temp_dir().join(format!(
            "planning-search-key-test-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        write_key(&path, "sk-test-key").unwrap();
        assert_eq!(read_key(&path).as_deref(), Some("sk-test-key"));
        let _ = fs::remove_file(&path);
    }
}
