//! A stdio MCP bridge: only the configured ego runtime runs with desktop access.
//! Codex's shell commands retain their workspace sandbox.
use serde_json::{Value, json};
use std::{io::{BufRead, Write}, path::Path};

pub fn serve(path: &Path) -> Result<(), String> {
    crate::ego::executable(&path.to_string_lossy())?;
    let directory = std::env::current_dir().map_err(|e| e.to_string())?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let request: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if let Some(response) = respond(&request, |args| crate::opencode::capture_diagnostics(path, &directory, args)) {
            serde_json::to_writer(&mut stdout, &response).map_err(|e| e.to_string())?;
            writeln!(stdout).map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn respond(request: &Value, mut execute: impl FnMut(&[&str]) -> Result<String, String>) -> Option<Value> {
    let id = request.get("id")?;
    let result = match request["method"].as_str().unwrap_or("") {
        "initialize" => json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"planning-ego","version":"1"}}),
        "ping" => json!({}),
        "tools/list" => json!({"tools":[
            {"name":"guide","description":"Read the complete installed ego-browser guide before using run. Browser calls use this MCP bridge, not shell commands.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
            {"name":"run","description":"Run a short ego-browser Node.js script using the installed guide's APIs. Reuse the assigned research Space. Respect user control and browser permission handoffs. Maximum 30 seconds per call; split long work into rounds. Do not use shell commands to access ego-browser.","inputSchema":{"type":"object","properties":{"script":{"type":"string"}},"required":["script"],"additionalProperties":false}}
        ]}),
        "tools/call" => {
            let params = &request["params"];
            let output = match params["name"].as_str() {
                Some("guide") => execute(&["skill"]),
                Some("run") => match params["arguments"]["script"].as_str() {
                    Some(script) if !script.is_empty() && script.len() <= 256_000 => execute(&["nodejs", "-e", script]),
                    _ => Err("Provide a nonempty script of at most 256,000 bytes.".into()),
                },
                _ => Err("Unknown browser tool.".into()),
            };
            let is_error = output.is_err();
            json!({"isError":is_error,"content":[{"type":"text","text":output.unwrap_or_else(|e|e)}]})
        }
        _ => return Some(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scripts_are_passed_as_one_argument_and_errors_are_tool_errors() {
        let script = "console.log('$HOME; `echo nope`')";
        let request = json!({"id":7,"method":"tools/call","params":{"name":"run","arguments":{"script":script}}});
        let result = respond(&request, |args| {
            assert_eq!(args, &["nodejs", "-e", script]);
            Err("Browser needs user control".into())
        }).unwrap();
        assert_eq!(result["id"], 7);
        assert_eq!(result["result"]["isError"], true);
        assert!(respond(&json!({"method":"notifications/initialized"}), |_| panic!()).is_none());
        let invalid = json!({"id":8,"method":"tools/call","params":{"name":"run","arguments":{}}});
        assert_eq!(respond(&invalid, |_| panic!()).unwrap()["result"]["isError"], true);
    }
}
