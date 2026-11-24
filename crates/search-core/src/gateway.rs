//! HTTP client for the remote search gateway. All provider-specific web tool
//! logic lives in the gateway; this module only speaks the stable JSON contract.

use crate::{
    ErrorCode, FetchedPage, Provider, ProviderResponse, SearchError, SearchRequest, SearchResult,
    post_json,
};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

pub const URL_ENV: &str = "SEARCH_GATEWAY_URL";
pub const TOKEN_ENV: &str = "SEARCH_GATEWAY_TOKEN";
pub const CREDENTIALS_ENV: &str = "PLANNING_SEARCH_CREDENTIALS";

fn credentials_path() -> PathBuf {
    env::var_os(CREDENTIALS_ENV)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/planning-search/credentials.json")))
        .unwrap_or_else(|| PathBuf::from(".config/planning-search/credentials.json"))
}

fn from_file() -> Option<(String, String)> {
    let raw = fs::read_to_string(credentials_path()).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let url = value.get("gateway_url")?.as_str()?.trim().to_owned();
    let token = value.get("token")?.as_str()?.trim().to_owned();
    (!url.is_empty() && !token.is_empty()).then_some((url, token))
}

pub fn credentials() -> Option<(String, String)> {
    let url = env::var(URL_ENV).ok().filter(|value| !value.trim().is_empty()).map(|value| value.trim().to_owned());
    let token = env::var(TOKEN_ENV).ok().filter(|value| !value.trim().is_empty()).map(|value| value.trim().to_owned());
    match (url, token) {
        (Some(url), Some(token)) => Some((url, token)),
        _ => from_file(),
    }
}

fn error_code(code: &str) -> ErrorCode {
    match code {
        "auth" => ErrorCode::Auth,
        "rate_limit" => ErrorCode::RateLimit,
        "timeout" => ErrorCode::Timeout,
        "invalid_params" => ErrorCode::InvalidParams,
        "no_results" => ErrorCode::NoResults,
        "fetch_failed" => ErrorCode::FetchFailed,
        _ => ErrorCode::Transport,
    }
}

fn check_error(value: &Value) -> Result<(), SearchError> {
    let Some(error) = value.get("error") else { return Ok(()) };
    let code = error.get("code").and_then(Value::as_str).unwrap_or("transport");
    let message = error.get("message").and_then(Value::as_str).unwrap_or("Gateway request failed.");
    Err(SearchError::new(error_code(code), message.to_owned()))
}

fn post(path: &str, body: Value) -> Result<Value, SearchError> {
    let (url, token) = credentials().ok_or_else(|| {
        SearchError::new(ErrorCode::Auth, "Search gateway is not configured.")
    })?;
    // Settings stores the MCP endpoint (…/mcp) for agent clients. The REST
    // calls below need the gateway origin, so strip that suffix here.
    let base = url.trim_end_matches('/').strip_suffix("/mcp").unwrap_or(url.trim_end_matches('/'));
    let endpoint = format!("{base}{path}");
    let (_, text) = post_json(&endpoint, &token, &body.to_string(), 30)?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        SearchError::new(ErrorCode::Transport, format!("Invalid gateway response: {error}"))
    })?;
    check_error(&value)?;
    Ok(value)
}

fn source_from(value: &Value) -> Option<SearchResult> {
    let url = value.get("url")?.as_str()?.trim().to_owned();
    if url.is_empty() { return None; }
    Some(SearchResult {
        url,
        title: value.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
        content: value.get("content").and_then(Value::as_str).unwrap_or("").to_owned(),
        published_at: value.get("published_at").and_then(Value::as_str).map(str::to_owned),
        authority: value.get("authority").and_then(Value::as_i64).map(|value| value as i32),
        score: value.get("score").and_then(Value::as_f64),
        site_name: value.get("site_name").and_then(Value::as_str).map(str::to_owned),
    })
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

pub struct GatewayProvider;

impl GatewayProvider {
    /// Returns the provider ids configured on the gateway. The MCP `providers`
    /// tool uses this instead of local registry names so callers see real ids.
    pub fn provider_ids() -> Result<Vec<String>, SearchError> {
        let value = post("/v1/provider", json!({}))?;
        Ok(value.get("providers").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect())
    }
}

impl Provider for GatewayProvider {
    fn name(&self) -> &'static str {
        "gateway"
    }

    fn search(&self, _request: &SearchRequest) -> Result<Vec<SearchResult>, SearchError> {
        self.search_response(_request).map(|response| response.sources)
    }

    fn fetch(&self, url: &str) -> Result<FetchedPage, SearchError> {
        self.fetch_response(url).map(|(_, page, _)| page)
    }

    fn search_response(&self, request: &SearchRequest) -> Result<ProviderResponse, SearchError> {
        let mut body = json!({
            "provider": "auto",
            "query": request.query,
            "count": request.count,
            "need_content": request.need_content,
        });
        if let Some(question) = &request.ori_question {
            body["ori_question"] = json!(question);
        }
        let value = post("/v1/search", body)?;
        let sources = value.get("sources").and_then(Value::as_array).into_iter().flatten()
            .filter_map(source_from)
            .collect();
        Ok(ProviderResponse {
            provider: value.get("provider").and_then(Value::as_str).unwrap_or("gateway").to_owned(),
            sources,
            warnings: strings(value.get("warnings")),
            answer: value.get("answer").and_then(Value::as_str).filter(|text| !text.trim().is_empty()).map(str::to_owned),
        })
    }

    fn fetch_response(&self, url: &str) -> Result<(String, FetchedPage, Vec<String>), SearchError> {
        let value = post("/v1/fetch", json!({ "provider": "auto", "url": url }))?;
        let page = value.get("page").ok_or_else(|| SearchError::new(ErrorCode::Transport, "Gateway fetch response has no page."))?;
        let page = FetchedPage {
            url: page.get("url").and_then(Value::as_str).unwrap_or(url).to_owned(),
            title: page.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
            content: page.get("content").and_then(Value::as_str).unwrap_or("").to_owned(),
        };
        Ok((
            value.get("provider").and_then(Value::as_str).unwrap_or("gateway").to_owned(),
            page,
            strings(value.get("warnings")),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_errors_map_to_stable_codes() {
        let value: Value = serde_json::from_str(r#"{"error":{"code":"rate_limit","message":"slow down"}}"#).unwrap();
        let error = check_error(&value).unwrap_err();
        assert_eq!(error.code, ErrorCode::RateLimit);
        assert!(error.message.contains("slow down"));
    }

    #[test]
    fn canonical_sources_parse_without_vendor_fields() {
        let value: Value = serde_json::from_str(r#"{
            "provider":"deepseek",
            "sources":[{"url":"https://example.com/a","title":"A","content":"body","published_at":"2026-11-20T00:00:00Z","authority":3,"score":0.8,"site_name":"Example"}],
            "answer":"answer",
            "warnings":["model_generated_content"]
        }"#).unwrap();
        let source = source_from(&value["sources"][0]).unwrap();
        assert_eq!(source.published_at.as_deref(), Some("2026-11-20T00:00:00Z"));
        assert_eq!(source.score, Some(0.8));
        assert_eq!(strings(value.get("warnings")), vec!["model_generated_content".to_owned()]);
    }
}
