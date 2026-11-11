//! Human-facing timestamps; persisted values retain their original precision and zone.
pub fn timestamp(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .or_else(|_| chrono::DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S %:z"))
        .map(|date| date.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| value.to_string())
}

/// Format only the app-generated metadata line, preserving the request prose.
pub fn original_request(text: &str, request: Option<&crate::domain::ResearchRequest>) -> String {
    let Some(request) = request else { return text.to_string(); };
    let suffix = format!("\n\nOriginal request (submitted {}):\n{}", request.submitted_at, request.input);
    let Some(prefix) = text.strip_suffix(&suffix) else { return text.to_string(); };
    format!("{prefix}\n\nOriginal request (submitted {}):\n{}", timestamp(&request.submitted_at), request.input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_request_prose_is_never_rewritten() {
        let source = "Original request (submitted 2026-11-11T20:54:31.320122+08:00):\nQuoted user text";
        assert_eq!(original_request(source, None), source);
    }

    #[test]
    fn only_verified_generated_suffix_is_formatted() {
        let mut store = crate::domain::Store::empty();
        let prose = "Original request (submitted 2026-11-11T20:54:31.320122+08:00):\nQuoted user text";
        store.create_research_request(1, None, prose).unwrap();
        let request = &store.research_requests()[0];
        let source = format!("Brief\n\nOriginal request (submitted {}):\n{}", request.submitted_at, request.input);
        assert_eq!(original_request(&source, Some(request)), format!("Brief\n\nOriginal request (submitted {}):\n{}", timestamp(&request.submitted_at), prose));
        assert_eq!(original_request(prose, Some(request)), prose);
    }

    #[test]
    fn equivalent_storage_formats_display_the_same_local_second() {
        let rfc = "2026-11-11T20:54:31.320122+08:00";
        let expected = chrono::DateTime::parse_from_rfc3339(rfc).unwrap()
            .with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string();
        assert_eq!(timestamp(rfc), expected);
        assert_eq!(timestamp("2026-11-11 12:54:31 +00:00"), expected);
        assert_eq!(timestamp("Time not recorded"), "Time not recorded");
        assert_eq!(timestamp(""), "");
    }
}
