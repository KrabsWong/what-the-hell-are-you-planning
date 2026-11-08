//! Heuristic creation-time validation for automation instructions.
//!
//! This is deliberately a small, pure text check. It is **not** a sandbox, a
//! permission boundary, or proof that an approved agent cannot perform an
//! external action. Approved steps still run through the shared research agent
//! pipeline with that agent's normal permissions. Keep user-facing wording and
//! documentation aligned with this limitation.
//!
//! The check is intentionally conservative and incomplete. Wording variants,
//! negation scope, quoted descriptions, examples and non-English languages are
//! handled heuristically. Passing this validation is not permission to perform
//! an external action.

/// Maximum size accepted for one instruction field, in bytes.
pub const MAX_INSTRUCTION_BYTES: usize = 12_000;

/// User-editable instruction fields with distinct, actionable error labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstructionField {
    Objective,
    ExpectedResult,
    Instructions,
}

impl InstructionField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Objective => "Objective",
            Self::ExpectedResult => "Expected result",
            Self::Instructions => "Step instructions",
        }
    }

    fn empty_message(self) -> String {
        match self {
            Self::Objective => {
                "Describe what the automation should do each period (1–12,000 bytes).".into()
            }
            Self::ExpectedResult => "Describe the expected result (1–12,000 bytes).".into(),
            Self::Instructions => "Add the step instructions (1–12,000 bytes).".into(),
        }
    }

    fn too_long_message(self) -> String {
        match self {
            Self::Objective => {
                "The objective is too long; use at most 12,000 bytes.".into()
            }
            Self::ExpectedResult => {
                "The expected result is too long; use at most 12,000 bytes.".into()
            }
            Self::Instructions => {
                "The step instructions are too long; use at most 12,000 bytes.".into()
            }
        }
    }
}

/// A likely request for an action the product does not intend to automate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rejection {
    pub action: &'static str,
}

impl Rejection {
    pub fn message(self) -> String {
        format!(
            "This task appears to ask for {}. Automation creation accepts information-collection and local-report work only; remove that action to continue. This is input validation, not a read-only sandbox.",
            self.action
        )
    }
}

struct Rule {
    action: &'static str,
    phrases: &'static [&'static str],
}

/// Obvious external-action wording. Prefer specific phrases over bare words:
/// ordinary research topics can contain "publish", "order", "buy" or "sell".
const RULES: &[Rule] = &[
    Rule {
        action: "sending email",
        phrases: &[
            "send email",
            "send an email",
            "send emails",
            "sending email",
            "email the report",
            "email the results",
            "email the answer",
            "email the summary",
            "email it to",
            "email the team",
            "email the company",
            "send by email",
            "send it by email",
            "send the report by email",
            "send the results by email",
            "by email",
            "发送邮件",
            "发邮件",
            "邮件发送",
            "邮件发给",
        ],
    },
    Rule {
        action: "sending messages",
        phrases: &[
            "send message",
            "send a message",
            "send messages",
            "sending messages",
            "message the team",
            "post to slack",
            "post in slack",
            "发送消息",
            "发消息",
            "通知团队",
        ],
    },
    Rule {
        action: "placing orders or trading",
        phrases: &[
            "place order",
            "place an order",
            "place a trade",
            "place trades",
            "buy shares",
            "sell shares",
            "buy stock",
            "sell stock",
            "transfer funds",
            "wire funds",
            "make a payment",
            "pay the invoice",
            "下单",
            "买入",
            "卖出",
            "转账",
            "自动交易",
        ],
    },
    Rule {
        action: "publishing or deploying",
        phrases: &[
            "publish the report",
            "publish the new report",
            "publish a report",
            "publish a new report",
            "publish report",
            "publish it",
            "publish to",
            "post publicly",
            "deploy the",
            "deploy to",
            "自动发布",
            "发布报告",
            "发布到",
            "上线部署",
        ],
    },
    Rule {
        action: "pushing to Git",
        phrases: &[
            "git push",
            "push to github",
            "push to gitlab",
            "提交到仓库",
            "推送到仓库",
        ],
    },
    Rule {
        action: "deleting files",
        phrases: &[
            "delete file",
            "delete the file",
            "delete files",
            "delete the files",
            "remove files",
            "remove the directory",
            "rm -rf",
            "删除文件",
            "删除目录",
            "删掉文件",
        ],
    },
    Rule {
        action: "running scripts or shell commands",
        phrases: &[
            "run script",
            "run a script",
            "run the script",
            "execute script",
            "execute a script",
            "shell command",
            "run a shell",
            "执行脚本",
            "运行脚本",
            "执行命令",
        ],
    },
];

const NEGATION_MARKERS: &[&str] = &[
    "do not",
    "don't",
    "don’t",
    "does not",
    "doesn't",
    "doesn’t",
    "never",
    "no need",
    "without",
    "avoid",
    "not",
    "cannot",
    "can't",
    "can’t",
    "instead of",
    "无需",
    "不用",
    "不要",
    "不得",
    "禁止",
    "别",
    "勿",
    "不需要",
];

const DESCRIPTION_MARKERS: &[&str] = &[
    "for example",
    "e.g.",
    "such as",
    "example:",
    "examples:",
    "the phrase",
    "the words",
    "the text",
    "the string",
    "the term",
    "the label",
    "the button",
    "the menu",
    "the field",
    "the option",
    "the title",
    "the heading",
    "the status",
    "the error",
    "the log",
    "mention of",
    "reference to",
    "例如",
    "比如",
    "示例",
    "举例",
    "字样",
    "这句话",
    "这段文字",
    "这个词",
    "这个标签",
    "这个按钮",
];

/// Markers after a closing quote that make the quoted text descriptive rather
/// than an instruction, for example `"send email" is an example`.
const DESCRIPTION_AFTER_MARKERS: &[&str] = &[
    "is an example",
    "is a phrase",
    "is the phrase",
    "is a term",
    "is an action",
    "is a label",
    "is a button",
    "is listed",
    "is prohibited",
    "is forbidden",
    "is not allowed",
    "is rejected",
    "is unsupported",
    "refers to",
    "describes",
    "means",
    "appears in",
    "指的是",
    "表示",
    "是示例",
    "属于",
    "被禁止",
];

/// Clause breaks for negation and example scope. Commas and "or" stay inside
/// the clause so lists such as "do not send email, trade, or delete files"
/// remain negated. Contrast and sequence markers do break the scope so a later
/// positive request is not waived.
const CLAUSE_MARKERS: &[&str] = &[
    " but ",
    " however ",
    " then ",
    " and then ",
    " yet ",
    "然而",
    "但是",
    "然后",
    "随后",
    "接着",
];

/// Return the first likely external-action request in `text`.
///
/// Quoted descriptions and nearby example markers are skipped where reasonably
/// distinguishable. This is a heuristic only; see the module documentation.
pub fn rejection(text: &str) -> Option<Rejection> {
    let lower = text.to_lowercase();
    let quotes = quoted_spans(&lower);
    let mut best: Option<(usize, &'static str)> = None;

    for rule in RULES {
        for phrase in rule.phrases {
            for (start, _) in lower.match_indices(phrase) {
                let end = start + phrase.len();
                if !has_ascii_boundaries(&lower, start, end, phrase) {
                    continue;
                }
                let span = span_containing(start, &quotes);
                let context_end = span.map_or(start, |(span_start, _)| span_start);
                let window = &lower[clause_start(&lower, context_end)..context_end];
                if contains_any_token(window, NEGATION_MARKERS) {
                    continue;
                }
                if contains_any_token(window, DESCRIPTION_MARKERS) {
                    continue;
                }
                if let Some((_, span_end)) = span {
                    let after = &lower[span_end..sentence_end(&lower, span_end)];
                    if contains_any_token(after, DESCRIPTION_AFTER_MARKERS) {
                        continue;
                    }
                    // A quote alone is not descriptive: `Please "publish the
                    // report"` remains a request. Without a descriptive marker,
                    // treat the quoted action as a candidate.
                }
                if best.is_none_or(|(position, _)| start < position) {
                    best = Some((start, rule.action));
                }
                break;
            }
        }
    }

    best.map(|(_, action)| Rejection { action })
}

/// Validate the required automation objective.
pub fn validate_objective(text: &str) -> Result<(), String> {
    validate_field(InstructionField::Objective, text, true)
}

/// Validate the expected result. `required` is false while the user is still
/// editing a Quick task, where an empty answer is allowed and a default is
/// derived later.
pub fn validate_expected_result(text: &str, required: bool) -> Result<(), String> {
    validate_field(InstructionField::ExpectedResult, text, required)
}

/// Validate arbitrary generated or inherited instruction text. The text must be
/// non-empty and within the same byte limit as the user-editable fields.
pub fn validate_instruction_text(text: &str) -> Result<(), String> {
    validate_field(InstructionField::Instructions, text, true)
}

/// Validate both user-editable automation instruction fields in form order.
pub fn validate_automation_fields(
    objective: &str,
    expected: &str,
    expected_required: bool,
) -> Result<(), String> {
    validate_objective(objective)?;
    validate_expected_result(expected, expected_required)
}

fn validate_field(field: InstructionField, text: &str, required: bool) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return if required {
            Err(field.empty_message())
        } else {
            Ok(())
        };
    }
    if text.len() > MAX_INSTRUCTION_BYTES {
        return Err(field.too_long_message());
    }
    if let Some(rejection) = rejection(text) {
        return Err(format!("{}: {}", field.label(), rejection.message()));
    }
    Ok(())
}

fn has_ascii_boundaries(text: &str, start: usize, end: usize, phrase: &str) -> bool {
    let first = phrase.chars().next().expect("non-empty phrase");
    let last = phrase.chars().next_back().expect("non-empty phrase");
    let before_ok = !first.is_ascii_alphanumeric()
        || text[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !character.is_ascii_alphanumeric());
    let after_ok = !last.is_ascii_alphanumeric()
        || text[end..]
            .chars()
            .next()
            .is_none_or(|character| !character.is_ascii_alphanumeric());
    before_ok && after_ok
}

fn clause_start(text: &str, match_start: usize) -> usize {
    let before = &text[..match_start];
    let mut start = 0;
    for (index, character) in before.char_indices() {
        if matches!(character, '.' | '!' | '?' | ';' | '\n' | '。' | '！' | '？' | '；') {
            start = index + character.len_utf8();
            continue;
        }
        let rest = &before[index..];
        for marker in CLAUSE_MARKERS {
            if rest.starts_with(marker) {
                start = index + marker.len();
                break;
            }
        }
    }
    start
}

fn contains_any_token(text: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|token| contains_token(text, token))
}

fn contains_token(text: &str, token: &str) -> bool {
    let mut offset = 0;
    while let Some(relative) = text[offset..].find(token) {
        let start = offset + relative;
        let end = start + token.len();
        let before_ok = !token
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
            || text[..start]
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_ascii_alphanumeric());
        let after_ok = !token
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_alphanumeric())
            || text[end..]
                .chars()
                .next()
                .is_none_or(|character| !character.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        offset = end;
        if offset >= text.len() {
            break;
        }
    }
    false
}

/// Byte ranges of paired quotes used for descriptions such as
/// `Analyze the phrase "send email"`.
fn quoted_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open: Option<(char, usize)> = None;
    for (index, character) in text.char_indices() {
        if let Some((opening, start)) = open {
            if Some(character) == closing_quote(opening) {
                spans.push((start, index + character.len_utf8()));
                open = None;
            }
            continue;
        }
        if is_opening_quote(character) {
            open = Some((character, index));
        }
    }
    spans
}

fn is_opening_quote(character: char) -> bool {
    matches!(character, '"' | '“' | '「' | '『' | '`')
}

fn closing_quote(opening: char) -> Option<char> {
    match opening {
        '"' => Some('"'),
        '“' => Some('”'),
        '「' => Some('」'),
        '『' => Some('』'),
        '`' => Some('`'),
        _ => None,
    }
}

fn span_containing(index: usize, spans: &[(usize, usize)]) -> Option<(usize, usize)> {
    spans
        .iter()
        .find(|(start, end)| index >= *start && index < *end)
        .copied()
}

fn sentence_end(text: &str, from: usize) -> usize {
    for (offset, character) in text[from..].char_indices() {
        if matches!(
            character,
            '.' | '!' | '?' | ';' | '\n' | '。' | '！' | '？' | '；'
        ) {
            return from + offset;
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_rejects_obvious_external_actions() {
        for text in [
            "Send email to the team with the result",
            "Place a trade after the report",
            "Delete the files after collection",
            "Run a shell command to inspect the market",
            "发送邮件给团队",
        ] {
            let error = validate_objective(text).expect_err(text);
            assert!(error.starts_with("Objective: "), "{error}");
            assert!(error.contains("input validation"), "{error}");
            assert!(!error.contains("cannot perform"), "{error}");
        }
    }

    #[test]
    fn expected_result_is_validated_even_when_optional() {
        let error = validate_automation_fields("Check public prices", "Send email the answer", true)
            .expect_err("expected-only request");
        assert!(error.starts_with("Expected result: "), "{error}");

        let error = validate_expected_result("Send email the answer", false)
            .expect_err("optional fields still reject real requests");
        assert!(error.starts_with("Expected result: "), "{error}");

        assert!(validate_expected_result("", false).is_ok());
        assert!(validate_expected_result("   ", false).is_ok());
        assert!(validate_expected_result("", true).is_err());
    }

    #[test]
    fn empty_and_length_rules_are_field_specific() {
        assert!(validate_objective("").expect_err("empty objective").starts_with("Describe"));
        assert!(validate_expected_result("", true)
            .expect_err("empty required expected result")
            .starts_with("Describe the expected result"));
        assert!(validate_instruction_text("")
            .expect_err("empty instructions")
            .starts_with("Add the step instructions"));

        assert!(validate_objective(&"x".repeat(MAX_INSTRUCTION_BYTES + 1)).is_err());
        assert!(validate_objective(&"x".repeat(MAX_INSTRUCTION_BYTES)).is_ok());
        assert!(validate_expected_result(
            &"x".repeat(MAX_INSTRUCTION_BYTES + 1),
            false
        )
        .is_err());
    }

    #[test]
    fn wording_variants_are_covered_but_not_exhaustive() {
        for text in [
            "Email the results to the team",
            "Send the report by email",
            "Post to Slack when finished",
            "Buy shares before the announcement",
            "Transfer funds to the account",
            "Publish the report to the public site",
            "Push to GitHub after the report",
            "Remove the directory after the run",
            "Execute a script to collect the data",
            "买股票并自动交易",
        ] {
            assert!(rejection(text).is_some(), "{text}");
        }
    }

    #[test]
    fn ordinary_research_vocabulary_is_not_treated_as_an_action() {
        for text in [
            "Analyze the publishing industry and its order backlog.",
            "Compare buy-side analyst expectations for email marketing revenue.",
            "Review the company's deployment pipeline and shell company risks.",
            "Summarize public pages about payment processing and trading venues.",
        ] {
            assert_eq!(rejection(text), None, "{text}");
        }
    }

    #[test]
    fn english_and_chinese_negations_are_accepted() {
        for text in [
            "Do not send email; summarize public pages instead.",
            "Never publish, trade, or delete files.",
            "No need to send messages or email the team.",
            "Avoid placing an order or transferring funds.",
            "不要发送邮件，只汇总公开信息。",
            "禁止发布报告或删除文件。",
            "无需通知团队，也不执行脚本。",
        ] {
            assert_eq!(rejection(text), None, "{text}");
        }
    }

    #[test]
    fn mixed_negative_and_positive_clauses_are_rejected() {
        for text in [
            "Do not send email, but publish the report to Slack.",
            "Do not send email, then publish the report.",
            "Do not send email and then publish the report.",
            "Do not send email; however, delete the files.",
            "不要发送邮件，但是发布报告。",
            "不要发送邮件，然后删除文件。",
        ] {
            assert!(rejection(text).is_some(), "{text}");
        }
    }

    #[test]
    fn later_positive_request_is_not_waived_by_an_earlier_negation() {
        let text = "Do not send email about the old report. Publish the new report.";
        assert_eq!(rejection(text).map(|value| value.action), Some("publishing or deploying"));
    }

    #[test]
    fn quoted_descriptions_and_explicit_examples_are_skipped() {
        for text in [
            "Analyze the phrase \"send email\" in support tickets.",
            "The words 'publish the report' describe an action we reject.",
            "For example, send email is the kind of request this product rejects.",
            "Review the text “删除文件” in the policy document.",
            "Summarize mentions such as send message in the feedback.",
            "\"send email\" is an example of a rejected action.",
            "\"publish the report\" is not allowed.",
        ] {
            assert_eq!(rejection(text), None, "{text}");
        }

        // The action phrase itself must remain outside the quoted target.
        assert!(rejection("Send email to \"the research team\"").is_some());
    }

    #[test]
    fn quoted_imperatives_are_still_requests() {
        for text in [
            "Execute this instruction: \"send email to the team\".",
            "Please \"publish the report\".",
            "Do this: \"delete the files\" after the run.",
        ] {
            assert!(rejection(text).is_some(), "{text}");
        }
    }

    #[test]
    fn apostrophes_do_not_open_a_quote_span() {
        // A contraction must not swallow a later positive clause.
        for text in [
            "Don't send email; publish the report.",
            "Don't stop at the email source; publish the report anyway.",
            "Don't \"send email\"; publish the report.",
        ] {
            assert_eq!(
                rejection(text).map(|value| value.action),
                Some("publishing or deploying"),
                "{text}"
            );
        }
        assert_eq!(rejection("Don’t send email; summarize public pages."), None);
    }

    #[test]
    fn negation_matching_respects_word_boundaries() {
        // "not" inside "notable" must not negate a later action.
        assert!(rejection("A notable public report; send email the summary.").is_some());
    }

    #[test]
    fn unicode_processing_is_panic_free_and_byte_bounded() {
        // 4,000 three-byte characters are exactly 12,000 bytes.
        let allowed = "研".repeat(4_000);
        assert_eq!(allowed.len(), MAX_INSTRUCTION_BYTES);
        assert!(validate_objective(&allowed).is_ok());

        let too_long = "研".repeat(4_001);
        assert!(too_long.len() > MAX_INSTRUCTION_BYTES);
        assert!(validate_objective(&too_long).is_err());

        // Lowercasing can expand a character; offsets must remain on character
        // boundaries when the clause window is sliced.
        assert!(rejection("İstanbul research; send email the result").is_some());
        assert!(rejection("公开信息，然后发送邮件给团队").is_some());
    }

    #[test]
    fn arbitrary_instruction_text_uses_the_shared_rules() {
        assert!(validate_instruction_text("Publish the report after analysis").is_err());
        assert!(validate_instruction_text("Summarize public filings with sources").is_ok());
    }
}
