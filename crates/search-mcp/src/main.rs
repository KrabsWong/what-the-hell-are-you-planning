//! Stdio MCP server exposing a stable, two-tool search facade.
//!
//! The tool names and schemas are fixed. New backends are added inside
//! `search-core` adapters, so no agent configuration or prompt has to change.

use search_core::{
    DEFAULT_COUNT, ErrorCode, MCP_INSTRUCTIONS, MCP_VERSION, MAX_QUERY_CHARS, MAX_RESPONSE_BYTES,
    ProviderResponse, Registry, SearchError, SearchRequest, fit_response,
};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

fn text_result(value: &str, is_error: bool) -> Value {
    json!({ "isError": is_error, "content": [{ "type": "text", "text": value }] })
}

fn tool_error(error: SearchError) -> Value {
    text_result(&format!("{}: {}", error.code.as_str(), error.message), true)
}

fn respond(request: &Value, registry: &Registry) -> Option<Value> {
    let id = request.get("id")?;
    let result = match request["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "planning-search", "version": MCP_VERSION },
            "instructions": MCP_INSTRUCTIONS,
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": [
            {
                "name": "search",
                "description": "Search the web and return structured sources with original URLs and fetched page content.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": { "type": "string", "description": "Search query, 1 to 128 characters. Supports site:, after: and before: operators." },
                        "count": { "type": "integer", "description": "Number of sources; capped by the server." },
                        "need_content": { "type": "boolean", "description": "Include fetched page content. Defaults to true." },
                        "ori_question": { "type": "string", "description": "Optional original research question." }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "fetch",
                "description": "Have the model read one original URL and return a usable plaintext summary with citations generated from that page. Not a verbatim page capture.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": { "url": { "type": "string", "description": "HTTP(S) URL, at most 8192 characters." } },
                    "required": ["url"]
                }
            },
            {
                "name": "providers",
                "description": "List configured search providers. Returns provider ids only; model choices stay server-side.",
                "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
            }
        ] }),
        "tools/call" => {
            let params = &request["params"];
            let arguments = &params["arguments"];
            match params["name"].as_str().unwrap_or("") {
                "search" => {
                    let query = arguments["query"].as_str().unwrap_or("").trim().to_owned();
                    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
                        tool_error(SearchError::new(
                            ErrorCode::InvalidParams,
                            format!("Query must be 1 to {MAX_QUERY_CHARS} characters."),
                        ))
                    } else {
                        let request = SearchRequest {
                            query,
                            count: arguments["count"].as_u64().unwrap_or(DEFAULT_COUNT as u64) as usize,
                            need_content: arguments["need_content"].as_bool().unwrap_or(true),
                            ori_question: arguments["ori_question"].as_str().map(str::to_owned),
                        };
                        match registry.search_auto(&request) {
                            Ok(response) => {
                                let response: ProviderResponse = fit_response(response, MAX_RESPONSE_BYTES);
                                match serde_json::to_string(&response) {
                                    Ok(text) => text_result(&text, false),
                                    Err(error) => tool_error(SearchError::new(
                                        ErrorCode::Transport,
                                        format!("Cannot encode search result: {error}"),
                                    )),
                                }
                            }
                            Err(error) => tool_error(error),
                        }
                    }
                }
                "fetch" => {
                    let url = arguments["url"].as_str().unwrap_or("").trim().to_owned();
                    if url.is_empty() {
                        tool_error(SearchError::new(ErrorCode::InvalidParams, "Provide a URL to fetch."))
                    } else {
                        match registry.fetch_auto(&url) {
                            Ok((provider, page, warnings)) => {
                                let response = json!({
                                    "provider": provider,
                                    "page": page,
                                    "warnings": warnings,
                                });
                                match serde_json::to_string(&response) {
                                    Ok(text) => text_result(&text, false),
                                    Err(error) => tool_error(SearchError::new(
                                        ErrorCode::Transport,
                                        format!("Cannot encode fetch result: {error}"),
                                    )),
                                }
                            }
                            Err(error) => tool_error(error),
                        }
                    }
                }
                "providers" => {
                    let providers: Vec<Value> = registry.names().into_iter()
                        .map(|id| json!({ "id": id }))
                        .collect();
                    match serde_json::to_string(&json!({ "providers": providers })) {
                        Ok(text) => text_result(&text, false),
                        Err(error) => tool_error(SearchError::new(
                            ErrorCode::Transport,
                            format!("Cannot encode provider list: {error}"),
                        )),
                    }
                }
                _ => tool_error(SearchError::new(ErrorCode::InvalidParams, "Unknown search tool.")),
            }
        }
        _ => return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "Method not found" } })),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn serve(registry: &Registry) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let request: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
        let Some(response) = respond(&request, registry) else { continue };
        serde_json::to_writer(&mut stdout, &response).map_err(|error| error.to_string())?;
        writeln!(stdout).map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn status() -> i32 {
    let registry = Registry::from_env();
    let names = registry.names();
    if names.is_empty() {
        println!("Not configured: set DEEPSEEK_API_KEY.");
        return 1;
    }
    println!("Configured · provider(s): {}", names.join(", "));
    0
}

fn help() {
    println!("planning-search-mcp");
    println!("  (no arguments)  serve MCP over stdio");
    println!("  --status        check configured providers, exit 1 when none");
    println!("  --version       print the tool contract version");
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.iter().any(|arg| arg == "--help") {
        help();
        return;
    }
    if arguments.iter().any(|arg| arg == "--version") {
        println!("planning-search-mcp {MCP_VERSION}");
        return;
    }
    if arguments.iter().any(|arg| arg == "--status") {
        std::process::exit(status());
    }
    let registry = Registry::from_env();
    if let Err(error) = serve(&registry) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use search_core::{FetchedPage, Provider, SearchResult};

    struct FakeProvider;

    impl Provider for FakeProvider {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, SearchError> {
            Ok(vec![SearchResult {
                url: format!("https://example.com/?q={}", request.query),
                title: "Example".into(),
                content: "Fetched body".into(),
                published_at: Some("2026-11-20T00:00:00Z".into()),
                authority: Some(3),
                score: Some(0.8),
                site_name: Some("Example".into()),
            }])
        }
        fn fetch(&self, url: &str) -> Result<FetchedPage, SearchError> {
            Ok(FetchedPage { url: url.into(), title: "Example".into(), content: "Fetched body".into() })
        }
    }

    fn registry() -> Registry {
        let mut registry = Registry::empty();
        registry.add(Box::new(FakeProvider));
        registry
    }

    #[test]
    fn initialize_carries_policy_without_a_guide_tool() {
        let response = respond(&json!({"id": 1, "method": "initialize"}), &registry()).unwrap();
        assert_eq!(response["result"]["serverInfo"]["name"], "planning-search");
        assert!(response["result"]["instructions"].as_str().unwrap().contains("not verified evidence"));
    }

    #[test]
    fn tool_list_stays_small() {
        let response = respond(&json!({"id": 2, "method": "tools/list"}), &registry()).unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0]["name"], "search");
        assert_eq!(tools[1]["name"], "fetch");
        assert_eq!(tools[2]["name"], "providers");
        assert_eq!(tools[0]["inputSchema"]["additionalProperties"], false);
    }

    #[test]
    fn providers_tool_returns_ids_only() {
        let request = json!({
            "id": 6,
            "method": "tools/call",
            "params": { "name": "providers", "arguments": {} }
        });
        let response = respond(&request, &registry()).unwrap();
        assert_eq!(response["result"]["isError"], false);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let body: Value = serde_json::from_str(text).unwrap();
        assert_eq!(body["providers"][0]["id"], "fake");
        assert!(body["providers"][0].get("models").is_none());
        assert!(body["providers"][0].get("capabilities").is_none());
    }

    #[test]
    fn search_returns_the_canonical_envelope() {
        let request = json!({
            "id": 3,
            "method": "tools/call",
            "params": { "name": "search", "arguments": { "query": "planning" } }
        });
        let response = respond(&request, &registry()).unwrap();
        assert_eq!(response["result"]["isError"], false);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let body: Value = serde_json::from_str(text).unwrap();
        assert_eq!(body["provider"], "fake");
        assert_eq!(body["sources"][0]["published_at"], "2026-11-20T00:00:00Z");
        assert_eq!(body["sources"][0]["score"], 0.8);
    }

    #[test]
    fn validation_and_unknown_tools_are_tool_errors() {
        let invalid = json!({
            "id": 4,
            "method": "tools/call",
            "params": { "name": "search", "arguments": { "query": "" } }
        });
        let response = respond(&invalid, &registry()).unwrap();
        assert_eq!(response["result"]["isError"], true);
        let unknown = json!({
            "id": 5,
            "method": "tools/call",
            "params": { "name": "legacy", "arguments": {} }
        });
        assert_eq!(respond(&unknown, &registry()).unwrap()["result"]["isError"], true);
        assert!(respond(&json!({"method": "notifications/initialized"}), &registry()).is_none());
    }
}
