//! Deterministic public report presentation. Markdown never enables raw HTML or unsafe URLs.

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// Only generated Markdown HTML enters this function; attribute values are already encoded.
// Local attachment links must not expose paths or resolve against another public report.
fn public_urls(html: &str) -> String {
    let mut result = html.to_string();
    for attribute in ["href=\"", "src=\""] {
        let mut offset = 0;
        while let Some(start) = result[offset..].find(attribute) {
            let start = offset + start + attribute.len();
            let Some(length) = result[start..].find('"') else {
                break;
            };
            let end = start + length;
            let url = &result[start..end];
            if !(url.starts_with("https://")
                || url.starts_with("http://")
                || (attribute == "href=\"" && url.starts_with('#')))
            {
                result.replace_range(start..end, "");
                offset = start + 1;
            } else {
                offset = end + 1;
            }
        }
    }
    result
}

fn heading_text(html: &str) -> String {
    let mut inside = false;
    html.chars()
        .filter(|c| {
            if *c == '<' {
                inside = true;
                return false;
            }
            if *c == '>' {
                inside = false;
                return false;
            }
            !inside
        })
        .collect()
}

pub fn render(title: &str, markdown: &str) -> String {
    // GFM enables tables, task lists and strikethrough. Dangerous HTML/protocols stay disabled.
    let html = markdown::to_html_with_options(markdown, &markdown::Options::gfm())
        .unwrap_or_else(|_| format!("<pre>{}</pre>", escape(markdown)));
    let html = public_urls(&html);
    // The page owns its title; avoid repeating an identical leading Markdown title.
    let html = if let Some(rest) = html.strip_prefix("<h1>") {
        if let Some(end) = rest.find("</h1>") {
            if heading_text(&rest[..end]) == escape(title) {
                rest[end + 5..].trim_start().to_string()
            } else {
                html
            }
        } else {
            html
        }
    } else {
        html
    };
    let mut body = String::new();
    let mut toc = String::new();
    let mut remaining = html.as_str();
    let mut count = 0;
    while let Some((position, level)) = (1..=3)
        .filter_map(|level| remaining.find(&format!("<h{level}>")).map(|p| (p, level)))
        .min_by_key(|(p, _)| *p)
    {
        let open_len = 4;
        let content_start = position + open_len;
        let closing = format!("</h{level}>");
        let Some(end) = remaining[content_start..].find(&closing) else {
            break;
        };
        let content = &remaining[content_start..content_start + end];
        count += 1;
        let id = format!("section-{count}");
        body.push_str(&remaining[..position]);
        body.push_str(&format!("<h{level} id=\"{id}\">{content}</h{level}>"));
        toc.push_str(&format!(
            "<a class=\"toc-depth-{level}\" href=\"#{id}\">{}</a>",
            heading_text(content)
        ));
        remaining = &remaining[content_start + end + closing.len()..];
    }
    body.push_str(remaining);
    let body = body.replace("<table>", "<div class=\"table-wrap\" tabindex=\"0\" role=\"region\" aria-label=\"报告数据表\"><table>")
        .replace("</table>", "</table></div>");
    let title = escape(title);
    let picker = include_str!("../assets/report/theme-picker.html");
    let mobile_picker = picker.replacen(
        "class=\"theme-picker\"",
        "class=\"theme-picker mobile-picker\"",
        1,
    );
    format!(
        r##"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="robots" content="noindex,nofollow,noarchive"><meta name="referrer" content="no-referrer">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; font-src https://gw.alipayobjects.com; img-src https: http:; base-uri 'none'; form-action 'none'">
<title>{title} · Report Something</title><style>{fonts}{base}{style}</style></head>
<body><a class="skip" href="#report">跳转到报告</a><div class="shell">
<aside class="sidebar"><div class="brandline"><span class="brand-mark">R</span>Report Something</div>
<div class="side-tools"><input id="toc-search" type="search" placeholder="查找章节" aria-label="查找章节">{picker}</div>
<p class="side-title">{title}</p><p class="sidebar-title">本页目录</p><nav aria-label="报告目录">{toc}</nav></aside>
<main id="report">{mobile_picker}<details class="mobile-toc"><summary>本页目录</summary><nav>{toc}</nav></details>
<header class="report-head"><p class="eyebrow">RESEARCH REPORT</p><h1 class="report-title">{title}</h1></header>
{body}</main></div><script>{script}</script></body></html>"##,
        fonts = include_str!("../assets/report/fonts.css"),
        base = include_str!("../assets/report/base.css"),
        style = include_str!("../assets/report/style.css"),
        script = include_str!("../assets/report/theme.js")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_tables_and_unique_heading_links() {
        let page = render(
            "研究",
            "## 相同标题\n\n| 指标 | 值 |\n| --- | --- |\n| 收入 | 100 |\n\n## 相同标题",
        );
        assert!(page.contains("<table>"));
        assert!(page.contains("id=\"section-1\""));
        assert!(page.contains("id=\"section-2\""));
        assert!(page.contains("href=\"#section-2\""));
    }

    #[test]
    fn report_cannot_inject_scripts_or_local_urls() {
        let page = render(
            "</title><script>alert(1)</script>",
            "<script>alert(2)</script>\n\n[bad](javascript:alert%281%29) [local](file:///secret) [relative](/Users/private) [ok](https://example.com)\n\n![local](../private.png)",
        );
        assert!(!page.contains("<script>alert"));
        assert!(!page.contains("href=\"javascript:"));
        assert!(!page.contains("file:///secret"));
        assert!(!page.contains("/Users/private"));
        assert!(!page.contains("../private.png"));
        assert!(page.contains("href=\"https://example.com\""));
    }

    #[test]
    fn does_not_repeat_matching_document_title() {
        let page = render("Report", "# Report\n\n## Findings");
        assert_eq!(page.matches(">Report</h1>").count(), 1);
    }

    #[test]
    fn heading_markup_does_not_create_nested_toc_links() {
        let page = render("Report", "## [Source](https://example.com) **bold**");
        assert!(page.contains("href=\"#section-1\">Source bold</a>"));
    }
}
