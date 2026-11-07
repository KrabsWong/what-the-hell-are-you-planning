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

// Only inspect code blocks emitted by the Markdown renderer, never raw user HTML.
fn render_diagrams(html: &str) -> String {
    const OPEN: &str = "<pre><code class=\"language-mermaid\">";
    const CLOSE: &str = "</code></pre>";
    let mut rest = html;
    let mut output = String::new();
    while let Some(start) = rest.find(OPEN) {
        output.push_str(&rest[..start]);
        let source_start = start + OPEN.len();
        let Some(end) = rest[source_start..].find(CLOSE) else {
            output.push_str(&rest[start..]);
            return output;
        };
        let end = source_start + end;
        let source = rest[source_start..end].replace("&lt;", "<").replace("&gt;", ">")
            .replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
        match diagram_images(&source) {
            Ok(images) => output.push_str(&images),
            Err(_) => {
                output.push_str("<div class=\"diagram-fallback\"><p>图表暂时无法渲染，以下保留 Mermaid 源码。</p>");
                output.push_str(&rest[start..end + CLOSE.len()]);
                output.push_str("</div>");
            }
        }
        rest = &rest[end + CLOSE.len()..];
    }
    output.push_str(rest);
    output
}

fn diagram_images(source: &str) -> Result<String, String> {
    if source.len() > 16_384 || source.lines().count() > 256 {
        return Err("Diagram exceeds rendering limits".into());
    }
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|e| e.to_string())?;
    let options = mermaid_rs_renderer::RenderOptions::default();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
    let mut images = String::from("<figure class=\"report-diagram\">");
    for (name, theme) in [("light", options.theme), ("dark", mermaid_rs_renderer::Theme::dark())] {
        let svg = mermaid_rs_renderer::render_svg(&layout, &theme, &options.layout);
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
        let size = tree.size();
        if size.width() > 8192. || size.height() > 8192. || size.width() * size.height() > 16_000_000. {
            return Err("Diagram exceeds image limits".into());
        }
        // SVGs are isolated image documents, not active inline HTML. Encode every byte.
        let data: String = svg.bytes().map(|byte| format!("%{byte:02X}")).collect();
        images.push_str(&format!("<img class=\"diagram-{name}\" alt=\"研究报告流程图\" src=\"data:image/svg+xml,{data}\">"));
    }
    images.push_str("<details><summary>查看 Mermaid 源码</summary><pre><code>");
    images.push_str(&escape(source));
    images.push_str("</code></pre></details></figure>");
    Ok(images)
}

fn internal_gap_html() -> String {
    format!(
        "<blockquote>\n<p>{}</p>\n</blockquote>",
        crate::research::EVIDENCE_GAP_NOTE.trim_start_matches("> ")
    )
}
fn public_gap_note() -> String {
    format!(
        "<aside class=\"evidence-notice\" aria-label=\"研究局限\"><strong>证据说明</strong><p>{}</p></aside>",
        crate::research::EVIDENCE_GAP_NOTICE
    )
}

// The page owns h1. Remove only a repeated report title, never repeated sections
// separated by substantive content. A generated evidence notice is not content.
fn normalize_headings(title: &str, html: &str) -> String {
    let html = html.replace(&internal_gap_html(), &public_gap_note());
    let mut rest = html.as_str();
    let mut result = String::new();
    let mut previous = escape(title);
    while let Some((start, level)) = (1..=6)
        .filter_map(|level| rest.find(&format!("<h{level}>")).map(|p| (p, level)))
        .min_by_key(|(p, _)| *p)
    {
        let end_tag = format!("</h{level}>");
        let Some(length) = rest[start + 4..].find(&end_tag) else { break; };
        let end = start + 4 + length;
        let content = &rest[start + 4..end];
        let between = &rest[..start];
        let is_title_repeat = level == 1 && heading_text(content).trim() == previous.trim()
            && between.replace(&public_gap_note(), "").replace("<hr />", "").trim().is_empty();
        result.push_str(between);
        if !is_title_repeat {
            let output_level = level.max(2);
            result.push_str(&format!("<h{output_level}>{content}</h{output_level}>"));
        }
        previous = heading_text(content);
        rest = &rest[end + end_tag.len()..];
    }
    result.push_str(rest);
    result
}

pub fn render(title: &str, markdown: &str) -> String {
    // GFM enables tables, task lists and strikethrough. Dangerous HTML/protocols stay disabled.
    let html = markdown::to_html_with_options(markdown, &markdown::Options::gfm())
        .unwrap_or_else(|_| format!("<pre>{}</pre>", escape(markdown)));
    let html = render_diagrams(&public_urls(&html));
    let html = normalize_headings(title, &html);
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
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; font-src https://gw.alipayobjects.com; img-src https: http: data:; base-uri 'none'; form-action 'none'">
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
    fn mermaid_exports_isolated_theme_images_and_preserves_source() {
        let page = render("Diagram", "```mermaid\nflowchart LR\n A[客户] --> B[律师]\n```\n\n```rust\nlet a = 1;\n```");
        assert_eq!(page.matches("src=\"data:image/svg+xml,").count(), 2);
        assert!(page.contains("查看 Mermaid 源码"));
        assert!(page.contains("A[客户] --&gt; B[律师]"));
        assert!(page.contains("language-rust"));
        assert!(!page.split("<figure class=\"report-diagram\">").nth(1).unwrap().split("</figure>").next().unwrap().contains("<svg"));
        assert!(!page.contains("diagram-fallback\""));
        let unsafe_page = render("Unsafe", "```mermaid\nflowchart LR\n A[\"<script>alert(1)</script>\"] --> B[ok]\n```\n\n![injected](data:image/svg+xml,evil)");
        assert!(!unsafe_page.contains("<script>alert(1)"));
        assert!(!unsafe_page.contains("src=\"data:image/svg+xml,evil"));
    }

    #[test]
    fn invalid_and_oversized_diagrams_keep_readable_code() {
        for source in ["not a diagram".to_string(), format!("flowchart LR\n A[{}]", "x".repeat(16_385))] {
            let page = render("Fallback", &format!("```mermaid\n{source}\n```"));
            assert!(page.contains("图表暂时无法渲染"));
            assert!(page.contains("language-mermaid"));
            assert!(!page.contains("src=\"data:image/svg+xml,"));
        }
    }

    #[test]
    fn generated_goal_title_and_internal_notice_do_not_duplicate_report_title() {
        let input = "## 法律 AI 研究\n\n> Evidence gaps: this step finished with incomplete evidence. Preserve its limitations in downstream conclusions.\n\n# **法律 AI 研究**\n\n## 摘要\n\n证据不足，不能确认付费需求。";
        let page = render("Topic", input);
        assert_eq!(page.matches("<h1 ").count(), 1);
        assert_eq!(page.matches(">法律 AI 研究</h2>").count(), 1);
        assert!(!page.contains("Preserve its limitations"));
        assert!(page.contains("证据说明"));
        assert!(page.contains("不能确认付费需求"));
        // Both desktop and mobile TOCs link to the same unique heading.
        assert_eq!(page.matches("href=\"#section-1\">法律 AI 研究</a>").count(), 2);
        assert!(page.contains("id=\"section-2\">摘要</h2>"));
    }

    #[test]
    fn distinct_reports_and_repeated_real_sections_are_preserved() {
        let page = render("Topic", "## Goal A\n\n# Goal A\n\n内容 A\n\n## Goal B\n\n# Goal B\n\n## 摘要\n\n内容 B\n\n# 摘要\n\n补充内容");
        assert_eq!(page.matches("<h1 ").count(), 1);
        assert!(page.contains(">Goal A</h2>"));
        assert!(page.contains(">Goal B</h2>"));
        assert_eq!(page.matches(">摘要</h2>").count(), 2);
        assert!(page.contains("补充内容"));
    }

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
