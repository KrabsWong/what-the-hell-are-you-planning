//! Presentation-only summaries. Keep the original log for technical details.

pub(crate) fn user_message(line: &str) -> &'static str {
    if line.starts_with("[blocked]") {
        return "The research browser needs your attention. Open it to complete any sign-in or confirmation, then retry this step.";
    }
    if line.starts_with("[interrupted]") {
        return "This attempt stopped before it finished. Check the previous session, then retry this step.";
    }
    if line.starts_with("[workspace]") {
        return "The task's working folder could not be prepared. Check that its location is available and writable, then retry.";
    }

    let text = line.to_ascii_lowercase();
    let contains = |terms: &[&str]| terms.iter().any(|term| text.contains(term));
    let payload = line
        .strip_prefix("[error]")
        .or_else(|| line.strip_prefix("[stream error]"))
        .unwrap_or(line)
        .trim();
    let status = serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|value| status_code(&value));

    // Quota errors may also use HTTP 429; explain the quota before rate limits.
    if contains(&[
        "provider.free-limit",
        "freeusagelimiterror",
        "free usage limit",
    ]) {
        "This model's free usage allowance has been used up. Choose another model in Settings, or connect a provider with available credit, then retry this step."
    } else if contains(&[
        "insufficient_quota",
        "insufficient credit",
        "insufficient balance",
        "quota exceeded",
        "billing limit",
    ]) {
        "The model provider's usage allowance or credit has run out. Check your provider account or choose another model in Settings, then retry."
    } else if status == Some(401)
        || contains(&[
            "invalid api key",
            "invalid_api_key",
            "authenticationerror",
            "unauthorized",
        ])
    {
        "The model provider could not verify your credentials. Check your provider connection and API key, then retry this step."
    } else if status == Some(403) || contains(&["permissiondenied", "access denied", "forbidden"]) {
        "Your provider account does not have access to this model. Check its permissions or choose another model in Settings, then retry."
    } else if status == Some(429) || contains(&["rate limit", "rate_limit", "too many requests"]) {
        "The model is receiving too many requests. Wait a little before retrying, or choose another model in Settings."
    } else if contains(&[
        "context_length_exceeded",
        "context window",
        "maximum context",
        "too many tokens",
        "prompt is too long",
    ]) {
        "This task contains more information than the model can handle at once. Reduce its instructions or materials, or choose a model with a larger context window, then retry."
    } else if contains(&["timeout", "timed out"]) || status == Some(408) || status == Some(504) {
        "The model took too long to respond. Retry this step in a moment; if it keeps happening, try another model."
    } else if contains(&[
        "connection refused",
        "connection reset",
        "network",
        "dns",
        "failed to fetch",
        "error sending request",
    ]) {
        "Could not connect to the model service. Check your internet connection and provider connection, then retry this step."
    } else if status.is_some_and(|code| (500..600).contains(&code))
        || contains(&["overloaded", "service unavailable"])
    {
        "The model service is temporarily unavailable. Try again later or choose another model in Settings."
    } else {
        "This step could not finish. Retry it; if the problem continues, check your model settings. Last attempt details contains technical information for troubleshooting."
    }
}

fn status_code(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Object(fields) => ["status", "statusCode", "status_code"]
            .iter()
            .filter_map(|key| fields.get(*key))
            .find_map(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
            .or_else(|| fields.values().find_map(status_code)),
        serde_json::Value::String(text) => serde_json::from_str(text)
            .ok()
            .and_then(|nested| status_code(&nested)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::user_message;

    #[test]
    fn free_allowance_takes_priority_over_rate_limit() {
        let error = r#"[error] {"type":"provider.free-limit","message":"Free usage limit reached","status":429,"response":{"body":"{\"error\":{\"type\":\"FreeUsageLimitError\"}}"},"retryAfterMs":30432000}"#;
        assert!(user_message(error).contains("free usage allowance"));
        assert!(user_message(error).contains("Settings"));
    }

    #[test]
    fn structured_and_plain_errors_offer_relevant_recovery() {
        for (error, expected) in [
            (r#"[error] {"status":429}"#, "too many requests"),
            (r#"[error] {"error":{"statusCode":"401"}}"#, "credentials"),
            (
                r#"[error] {"response":{"body":"{\"status\":503}"}}"#,
                "temporarily unavailable",
            ),
            ("[error] insufficient_quota", "credit has run out"),
            ("[error] Forbidden", "does not have access"),
            ("[stream error] connection reset", "internet connection"),
            ("[error] request timed out", "too long to respond"),
            ("[error] context_length_exceeded", "more information"),
            ("[blocked] User confirmation required", "research browser"),
            ("[interrupted] Application closed", "previous session"),
            (
                "[workspace] Permission denied: /private/path",
                "working folder",
            ),
        ] {
            assert!(user_message(error).contains(expected), "{error}");
        }
    }

    #[test]
    fn unknown_or_malformed_errors_never_expose_raw_details() {
        for error in [
            "[error] {broken json secret-token",
            "[error] internal failure at /private/path",
            "[error] {\"request_id\":429}",
        ] {
            let message = user_message(error);
            assert!(message.starts_with("This step could not finish."));
            assert!(!message.contains("secret-token"));
            assert!(!message.contains("/private/path"));
        }
    }
}
