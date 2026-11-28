//! Display-language state and translation helpers.
//!
//! English source strings are translation keys. User-authored values, source
//! quotations, URLs, identifiers and protocol values are never rewritten: the
//! catalog only contains application-owned prose.

use crate::i18n_catalog::SIMPLIFIED_CHINESE;
use crate::i18n_catalog_audit_automation::AUTOMATION_AUDIT_TRANSLATIONS;
use crate::i18n_catalog_audit_core::CORE_AUDIT_TRANSLATIONS;
use crate::i18n_catalog_audit_domain::DOMAIN_AUDIT_TRANSLATIONS;
use crate::i18n_catalog_automation::AUTOMATION_TRANSLATIONS;
use crate::i18n_catalog_core::CORE_TRANSLATIONS;
use crate::i18n_catalog_domain::DOMAIN_TRANSLATIONS;
use crate::i18n_catalog_export::EXPORT_TRANSLATIONS;
use gpui_kit::SharedString;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InterfaceLanguage {
    #[default]
    English,
    SimplifiedChinese,
}

impl InterfaceLanguage {
    pub(crate) const ALL: [Self; 2] = [Self::English, Self::SimplifiedChinese];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::English => 0,
            Self::SimplifiedChinese => 1,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::SimplifiedChinese,
            _ => Self::English,
        }
    }
}

static ACTIVE_LANGUAGE: AtomicU8 = AtomicU8::new(0);

pub(crate) fn set_active(language: InterfaceLanguage) {
    ACTIVE_LANGUAGE.store(language.code(), Ordering::Release);
}

pub(crate) fn active() -> InterfaceLanguage {
    InterfaceLanguage::from_code(ACTIVE_LANGUAGE.load(Ordering::Acquire))
}

fn translations() -> &'static HashMap<&'static str, &'static str> {
    static TRANSLATIONS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TRANSLATIONS.get_or_init(|| {
        let mut map = HashMap::new();
        for (key, value) in AUTOMATION_TRANSLATIONS
            .iter()
            .chain(AUTOMATION_AUDIT_TRANSLATIONS)
            .chain(CORE_TRANSLATIONS)
            .chain(CORE_AUDIT_TRANSLATIONS)
            .chain(DOMAIN_TRANSLATIONS)
            .chain(DOMAIN_AUDIT_TRANSLATIONS)
            .chain(EXPORT_TRANSLATIONS)
            .chain(SIMPLIFIED_CHINESE)
        {
            map.insert(*key, *value);
        }
        map
    })
}

struct Template {
    key: &'static str,
    key_literals: Vec<&'static str>,
    value_literals: Vec<&'static str>,
    specs: Vec<&'static str>,
}

fn templates() -> &'static Vec<Template> {
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        let mut result: Vec<Template> = translations()
            .iter()
            .filter_map(|(key, value)| {
                let (key_literals, specs) = template_parts(key)?;
                let (value_literals, value_specs) = template_parts(value)?;
                if specs.is_empty() || specs != value_specs {
                    return None;
                }
                if key_literals.iter().all(|part| part.is_empty()) {
                    return None;
                }
                Some(Template {
                    key,
                    key_literals,
                    value_literals,
                    specs,
                })
            })
            .collect();
        result.sort_by(|left, right| {
            let left_len: usize = left.key_literals.iter().map(|part| part.len()).sum();
            let right_len: usize = right.key_literals.iter().map(|part| part.len()).sum();
            right_len
                .cmp(&left_len)
                .then_with(|| left.key.cmp(right.key))
        });
        result
    })
}

/// Translate an application-owned string for the active display language.
/// Unknown values pass through unchanged so user content is preserved.
pub(crate) fn tr(text: impl Into<SharedString>) -> SharedString {
    let text = text.into();
    if active() == InterfaceLanguage::English {
        return text;
    }
    if let Some(value) = translations().get(text.as_ref()) {
        return SharedString::from(*value);
    }
    match_translation(text.as_ref()).unwrap_or(text)
}

pub(crate) fn tr_string(text: impl AsRef<str>) -> String {
    tr(text.as_ref().to_owned()).to_string()
}

/// Match a catalog template against a rendered message and substitute its
/// captures. This lets dynamic error text be localized at the shared render
/// helpers without forcing every formatter call site to know the locale.
fn match_translation(text: &str) -> Option<SharedString> {
    templates()
        .iter()
        .find_map(|template| substitute(template, text))
}

fn substitute(template: &Template, text: &str) -> Option<SharedString> {
    if !text.starts_with(template.key_literals[0]) {
        return None;
    }
    let mut cursor = template.key_literals[0].len();
    let mut captures = Vec::with_capacity(template.specs.len());
    for literal in &template.key_literals[1..] {
        let end = if literal.is_empty() {
            text.len()
        } else {
            cursor + text[cursor..].find(literal)?
        };
        if end < cursor {
            return None;
        }
        captures.push(&text[cursor..end]);
        cursor = end + literal.len();
    }
    if cursor != text.len() {
        return None;
    }
    let mut output = template.value_literals[0].to_string();
    for (capture, literal) in captures.iter().zip(&template.value_literals[1..]) {
        output.push_str(capture);
        output.push_str(literal);
    }
    Some(SharedString::from(output))
}

fn template_parts(template: &str) -> Option<(Vec<&str>, Vec<&str>)> {
    let mut literals = Vec::new();
    let mut specs = Vec::new();
    let mut cursor = 0;
    while cursor < template.len() {
        match template[cursor..].find('{') {
            Some(offset) => {
                let open = cursor + offset;
                literals.push(&template[cursor..open]);
                let close = open + template[open..].find('}')?;
                specs.push(&template[open + 1..close]);
                cursor = close + 1;
            }
            None => {
                literals.push(&template[cursor..]);
                break;
            }
        }
    }
    if literals.len() == specs.len() {
        literals.push("");
    }
    (literals.len() == specs.len() + 1).then_some((literals, specs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LANGUAGE_LOCK: Mutex<()> = Mutex::new(());

    fn lock_language() -> std::sync::MutexGuard<'static, ()> {
        LANGUAGE_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    #[test]
    fn english_is_the_default_and_unknown_text_passes_through() {
        let _guard = lock_language();
        set_active(InterfaceLanguage::English);
        assert_eq!(tr("Create research"), "Create research");
        assert_eq!(tr("User authored title"), "User authored title");
    }

    #[test]
    fn simplified_chinese_translates_known_text() {
        let _guard = lock_language();
        set_active(InterfaceLanguage::SimplifiedChinese);
        assert_eq!(tr("Create research"), "创建研究");
        assert_eq!(tr("29 sources · 0 materials"), "29 个来源 · 0 份资料");
        assert_eq!(tr("Your research goals"), "你的研究目标");
        assert_eq!(
            tr(
                "The research browser needs your attention. Switch to it and handle its prompt; return to the task when you are ready to continue."
            ),
            "研究浏览器需要你处理。请切换到该浏览器并处理提示，完成后返回任务继续。"
        );
        assert_eq!(
            tr(
                "The research browser needs your attention. Switch to it to complete any sign-in or confirmation, then retry this step."
            ),
            "研究浏览器需要你处理。请切换到该浏览器完成登录或确认，然后重试此步骤。"
        );
        assert_eq!(tr("Log storage"), "日志存储");
        assert_eq!(tr("Log retention: 30 days"), "日志保留：30 天");
        assert_eq!(
            tr("No old detailed logs to clear."),
            "没有可清除的旧详细日志。"
        );
        assert_eq!(tr("User authored title"), "User authored title");
        set_active(InterfaceLanguage::English);
    }

    #[test]
    fn removed_browser_launch_copy_has_no_translation_entries() {
        for key in [
            "Open browser",
            "Open ego(lite)",
            "Opens the browser app. Select the research Space in ego(lite).",
            "Open the Space menu in ego(lite) to inspect your research task",
            "Open ego(lite) and use its Space menu to inspect your research task",
            "Open the research browser to handle its prompt. Return to the task when you are ready to continue.",
            "The research browser needs your attention. Open it to complete any sign-in or confirmation, then retry this step.",
            "Install ego(lite) and finish its first launch, then try again.",
        ] {
            assert!(translations().get(key).is_none(), "stale key: {key}");
        }
    }

    #[test]
    fn removed_automation_helper_copy_has_no_translation_entries() {
        for key in [
            "Example: Check today's fuel prices in Shenzhen, Guangdong and answer with the current price and source. Left empty, the name and expected answer are derived from this instruction.",
            "Use automations for information checks and local reports. The selected agent keeps its normal permissions; review results before acting.",
        ] {
            assert!(translations().get(key).is_none(), "stale key: {key}");
        }
    }
}
