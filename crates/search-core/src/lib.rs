//! Canonical search contract shared by `planning-search-mcp` and the desktop app.
//!
//! Provider adapters map vendor-specific responses into [`SearchResult`], so the
//! MCP tool contract never changes when a provider is added or replaced.

use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    env, fs,
    io::Write,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub const DEFAULT_COUNT: usize = 8;
pub const MAX_COUNT: usize = 10;
pub const MAX_QUERY_CHARS: usize = 128;
pub const MAX_SOURCE_CONTENT: usize = 16_000;
pub const MAX_RESPONSE_BYTES: usize = 60_000;
pub const MCP_VERSION: &str = "1";
pub const MCP_INSTRUCTIONS: &str = "Use search to discover sources and fetch to verify an original URL. A search snippet alone is not verified evidence; only fetched content counts as page access. Record URL, publisher, publication date and gaps; skip login walls and CAPTCHA instead of bypassing them.";

#[derive(Clone, Debug, Serialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub content: String,
    pub published_at: Option<String>,
    pub authority: Option<i32>,
    pub score: Option<f64>,
    pub site_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FetchedPage {
    pub url: String,
    pub title: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderResponse {
    pub provider: String,
    pub sources: Vec<SearchResult>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub query: String,
    pub count: usize,
    pub need_content: bool,
    pub ori_question: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Auth,
    RateLimit,
    Timeout,
    NoResults,
    FetchFailed,
    InvalidParams,
    Transport,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::RateLimit => "rate_limit",
            Self::Timeout => "timeout",
            Self::NoResults => "no_results",
            Self::FetchFailed => "fetch_failed",
            Self::InvalidParams => "invalid_params",
            Self::Transport => "transport",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchError {
    pub code: ErrorCode,
    pub message: String,
}

impl SearchError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

/// One vendor backend. Adding a provider means adding one of these; the MCP
/// tools below stay exactly the same.
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, SearchError>;
    fn fetch(&self, url: &str) -> Result<FetchedPage, SearchError>;
}

pub fn truncate_chars(input: &str, limit: usize) -> String {
    if input.chars().count() <= limit {
        return input.to_owned();
    }
    let mut text: String = input.chars().take(limit).collect();
    text.push_str("\n[content truncated]");
    text
}

fn encoded_len(response: &ProviderResponse) -> usize {
    serde_json::to_string(response).map(|text| text.len()).unwrap_or(0)
}

/// Keep the tool result inside a context budget without changing its shape.
pub fn fit_response(mut response: ProviderResponse, max_bytes: usize) -> ProviderResponse {
    while response.sources.len() > 1 && encoded_len(&response) > max_bytes {
        response.sources.pop();
        if !response.warnings.iter().any(|w| w == "response_truncated") {
            response.warnings.push("response_truncated".to_owned());
        }
    }
    if encoded_len(&response) > max_bytes {
        if let Some(first) = response.sources.first_mut() {
            first.content = truncate_chars(&first.content, 4_000);
            if !response.warnings.iter().any(|w| w == "content_truncated") {
                response.warnings.push("content_truncated".to_owned());
            }
        }
    }
    response
}

fn curl_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

fn private_temp(prefix: &str, contents: &[u8]) -> Result<std::path::PathBuf, SearchError> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = env::temp_dir().join(format!("planning-search-{prefix}-{}-{nonce}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path).map_err(|e| {
        SearchError::new(ErrorCode::Transport, format!("Cannot create private temp file: {e}"))
    })?;
    file.write_all(contents).map_err(|e| {
        SearchError::new(ErrorCode::Transport, format!("Cannot write private temp file: {e}"))
    })?;
    Ok(path)
}

fn post_json(endpoint: &str, api_key: &str, body: &str, timeout_secs: u64) -> Result<(u16, String), SearchError> {
    let body_path = private_temp("body", body.as_bytes())?;
    // Production endpoints are HTTPS-only; plain HTTP is allowed only for a
    // local compatible gateway, which needs no protocol pin at all.
    let protocol = if endpoint.starts_with("https://") {
        "proto = \"=https\"\nproto-redir = \"=https\"\n"
    } else {
        ""
    };
    // The key stays inside a 0600 config file, never in argv or a persisted record.
    let config = format!(
        "url = \"{}\"\nrequest = \"POST\"\nsilent\nshow-error\nlocation\nmax-time = {}\n{}header = \"Content-Type: application/json; charset=utf-8\"\nheader = \"Authorization: Bearer {}\"\ndata-binary = \"@{}\"\nwrite-out = \"\\n%{{http_code}}\"\n",
        curl_quote(endpoint),
        timeout_secs,
        protocol,
        curl_quote(api_key),
        curl_quote(&body_path.to_string_lossy()),
    );
    let config_path = private_temp("curl", config.as_bytes())?;
    let started = Command::new("curl")
        .arg("--config")
        .arg(&config_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| SearchError::new(ErrorCode::Transport, format!("Cannot start curl: {e}")));
    let _ = fs::remove_file(&body_path);
    let _ = fs::remove_file(&config_path);
    let output = started?;
    if output.status.code() == Some(28) {
        return Err(SearchError::new(ErrorCode::Timeout, "Search provider timed out."));
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let (raw, code_text) = text.rsplit_once('\n').unwrap_or((&text, "0"));
    let status = code_text.trim().parse::<u16>().unwrap_or(0);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if status == 0 {
        let reason = if stderr.is_empty() { "Search provider did not return a status." } else { &stderr };
        return Err(SearchError::new(ErrorCode::Transport, reason.to_owned()));
    }
    if !(200..300).contains(&status) {
        let code = match status {
            401 | 403 => ErrorCode::Auth,
            429 => ErrorCode::RateLimit,
            400 => ErrorCode::InvalidParams,
            _ => ErrorCode::Transport,
        };
        return Err(SearchError::new(code, format!("Search provider returned HTTP {status}: {stderr}")));
    }
    Ok((status, raw.to_owned()))
}

fn normalize_date(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.len() < 20 || !value.contains('T') {
        return None;
    }
    Some(value.to_owned())
}

fn map_deepseek_result(item: &Value) -> Option<SearchResult> {
    let url = item["url"].as_str()?.trim().to_owned();
    if url.is_empty() {
        return None;
    }
    let content = ["content", "summary", "snippet"]
        .into_iter()
        .filter_map(|key| item[key].as_str())
        .find(|text| !text.trim().is_empty())
        .unwrap_or("");
    Some(SearchResult {
        url,
        title: item["title"].as_str().unwrap_or("").to_owned(),
        content: truncate_chars(content, MAX_SOURCE_CONTENT),
        published_at: normalize_date(item["date"].as_str()),
        authority: item["authority"].as_i64().map(|value| value.clamp(-1, 4) as i32),
        score: item["rerank_score"]
            .as_f64()
            .map(|value| ((value / 3.0).clamp(0.0, 1.0) * 1000.0).round() / 1000.0),
        site_name: item["site_name"].as_str().map(str::to_owned),
    })
}

/// DeepSeek Search & Fetch adapter. Endpoint can be overridden for testing or
/// a compatible gateway through `DEEPSEEK_SEARCH_BASE_URL`.
pub struct DeepSeekProvider {
    api_key: String,
    base_url: String,
}

impl DeepSeekProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        let base_url = env::var("DEEPSEEK_SEARCH_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".to_owned());
        Self { api_key: api_key.into(), base_url }
    }
}

impl Provider for DeepSeekProvider {
    fn name(&self) -> &'static str {
        "deepseek"
    }

    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, SearchError> {
        let length = request.query.chars().count();
        if length == 0 || length > MAX_QUERY_CHARS {
            return Err(SearchError::new(
                ErrorCode::InvalidParams,
                format!("Query must be 1 to {MAX_QUERY_CHARS} characters."),
            ));
        }
        let mut body = json!({
            "query": request.query,
            "count": request.count.clamp(1, MAX_COUNT),
            "need_content": request.need_content,
        });
        if let Some(question) = &request.ori_question {
            body["ori_question"] = json!(question);
        }
        let (_, text) = post_json(
            &format!("{}/v1/tools/search", self.base_url),
            &self.api_key,
            &body.to_string(),
            30,
        )?;
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            SearchError::new(ErrorCode::Transport, format!("Invalid search response: {e}"))
        })?;
        let results = value.get("results").and_then(Value::as_array).ok_or_else(|| {
            SearchError::new(ErrorCode::Transport, "Search response has no results array.")
        })?;
        let mut seen = HashSet::new();
        let mut sources = Vec::new();
        for item in results {
            let Some(result) = map_deepseek_result(item) else { continue };
            if seen.insert(result.url.clone()) {
                sources.push(result);
            }
        }
        if sources.is_empty() {
            return Err(SearchError::new(ErrorCode::NoResults, "Search returned no usable sources."));
        }
        Ok(sources)
    }

    fn fetch(&self, url: &str) -> Result<FetchedPage, SearchError> {
        if url.len() > 8192 || !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(SearchError::new(
                ErrorCode::InvalidParams,
                "Fetch requires an HTTP(S) URL of at most 8192 characters.",
            ));
        }
        let body = json!({ "url": url });
        let (_, text) = post_json(
            &format!("{}/v1/tools/fetch", self.base_url),
            &self.api_key,
            &body.to_string(),
            45,
        )?;
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            SearchError::new(ErrorCode::Transport, format!("Invalid fetch response: {e}"))
        })?;
        let content = value["content"].as_str().unwrap_or("");
        if content.trim().is_empty() {
            return Err(SearchError::new(ErrorCode::FetchFailed, "Fetch returned no content."));
        }
        Ok(FetchedPage {
            url: url.to_owned(),
            title: value["title"].as_str().unwrap_or("").to_owned(),
            content: truncate_chars(content, MAX_SOURCE_CONTENT),
        })
    }
}

/// Ordered backends. `auto` (the default) retries transport-class failures on
/// the next provider, so the model never has to choose or handle failover.
pub struct Registry {
    providers: Vec<Box<dyn Provider>>,
    preferred: Option<String>,
}

impl Registry {
    pub fn empty() -> Self {
        Self { providers: Vec::new(), preferred: None }
    }

    pub fn from_env() -> Self {
        let mut registry = Self::empty();
        if let Ok(key) = env::var("DEEPSEEK_API_KEY") {
            if !key.trim().is_empty() {
                registry.add(Box::new(DeepSeekProvider::new(key)));
            }
        }
        registry.preferred = env::var("SEARCH_PROVIDER")
            .ok()
            .filter(|value| !value.trim().is_empty() && value != "auto")
            .map(|value| value.trim().to_owned());
        registry
    }

    pub fn add(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    pub fn names(&self) -> Vec<String> {
        self.providers.iter().map(|provider| provider.name().to_owned()).collect()
    }

    fn ordered(&self) -> Vec<&dyn Provider> {
        let mut providers: Vec<&dyn Provider> = self.providers.iter().map(|p| p.as_ref()).collect();
        if let Some(preferred) = &self.preferred {
            providers.sort_by_key(|provider| if provider.name() == preferred { 0 } else { 1 });
        }
        providers
    }

    pub fn search_auto(&self, request: &SearchRequest) -> Result<ProviderResponse, SearchError> {
        let mut last = None;
        for provider in self.ordered() {
            match provider.search(request) {
                Ok(sources) => {
                    return Ok(ProviderResponse {
                        provider: provider.name().to_owned(),
                        sources,
                        warnings: Vec::new(),
                    });
                }
                Err(error)
                    if matches!(
                        error.code,
                        ErrorCode::RateLimit | ErrorCode::Timeout | ErrorCode::Transport
                    ) =>
                {
                    last = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last.unwrap_or_else(|| {
            SearchError::new(ErrorCode::Auth, "No search provider configured. Set DEEPSEEK_API_KEY.")
        }))
    }

    pub fn fetch_auto(&self, url: &str) -> Result<(String, FetchedPage), SearchError> {
        let mut last = None;
        for provider in self.ordered() {
            match provider.fetch(url) {
                Ok(page) => return Ok((provider.name().to_owned(), page)),
                Err(error)
                    if matches!(
                        error.code,
                        ErrorCode::RateLimit
                            | ErrorCode::Timeout
                            | ErrorCode::Transport
                            | ErrorCode::FetchFailed
                    ) =>
                {
                    last = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last.unwrap_or_else(|| {
            SearchError::new(ErrorCode::Auth, "No search provider configured. Set DEEPSEEK_API_KEY.")
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_fields_are_normalized_into_the_canonical_shape() {
        let item = json!({
            "url": "https://example.com/a",
            "title": "A",
            "content": "body",
            "date": "2026-11-20T00:00:00Z",
            "authority": 9,
            "rerank_score": 2.4,
            "site_name": "Example"
        });
        let result = map_deepseek_result(&item).unwrap();
        assert_eq!(result.published_at.as_deref(), Some("2026-11-20T00:00:00Z"));
        assert_eq!(result.authority, Some(4));
        assert!((0.0..=1.0).contains(&result.score.unwrap()));
        assert_eq!(result.site_name.as_deref(), Some("Example"));
    }

    #[test]
    fn truncation_is_bounded_and_marked() {
        let text = truncate_chars(&"x".repeat(100), 10);
        assert!(text.chars().count() < 40);
        assert!(text.contains("[content truncated]"));
    }

    #[test]
    fn registry_reports_missing_configuration() {
        let error = Registry::empty()
            .search_auto(&SearchRequest {
                query: "test".into(),
                count: 1,
                need_content: false,
                ori_question: None,
            })
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Auth);
    }

    #[test]
    fn invalid_query_is_rejected_before_any_request() {
        let provider = DeepSeekProvider::new("unused");
        let error = provider
            .search(&SearchRequest {
                query: "x".repeat(MAX_QUERY_CHARS + 1),
                count: 1,
                need_content: false,
                ori_question: None,
            })
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidParams);
    }
}
