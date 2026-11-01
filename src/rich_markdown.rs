//! Native, offline diagrams and math. Preparation happens in TextView's parser,
//! not in its per-frame renderer; the original Markdown stays authoritative.
use gpui_kit::{
    base::text::{
        InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode, MarkdownParseContext,
        MarkdownPlugin, markdown_ast::Node,
    },
    component::Theme,
    *,
};
use std::sync::{Arc, LazyLock};

pub fn extensions() -> MarkdownExtensions {
    static EXTENSIONS: LazyLock<MarkdownExtensions> = LazyLock::new(|| {
        MarkdownExtensions::default()
            .plugin(RichBlock)
            .plugin(InlineMath)
    });
    EXTENSIONS.clone()
}

struct Graphic {
    light: Arc<Image>,
    dark: Arc<Image>,
    width: f32,
    height: f32,
    baseline: f32,
}

impl Graphic {
    fn image(&self, cx: &App) -> Arc<Image> {
        if Theme::global(cx).is_dark() {
            self.dark.clone()
        } else {
            self.light.clone()
        }
    }
}

fn prepare(source: &str, diagram: bool, display: bool) -> Result<Graphic, String> {
    let limit = if diagram { 16_384 } else { 4_096 };
    if source.len() > limit || (diagram && source.lines().count() > 256) {
        return Err("This expression is too large to preview.".into());
    }
    let (light, dark, baseline) = if diagram {
        // Strict mode reports unsupported/invalid syntax instead of drawing an error diagram.
        let parsed =
            mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|e| e.to_string())?;
        let options = mermaid_rs_renderer::RenderOptions::default();
        let layout =
            mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
        let light = mermaid_rs_renderer::render_svg(&layout, &options.theme, &options.layout);
        let dark_theme = mermaid_rs_renderer::Theme {
            font_family: options.theme.font_family.clone(),
            font_size: options.theme.font_size,
            ..mermaid_rs_renderer::Theme::dark()
        };
        let dark = mermaid_rs_renderer::render_svg(&layout, &dark_theme, &options.layout);
        (light, dark, 0.)
    } else {
        let font = latex_rust::MathFont::stix_two_math().map_err(|e| e.to_string())?;
        let mut options = latex_rust::SvgOptions {
            display,
            ..Default::default()
        };
        let ast = latex_rust::parse(source).map_err(|e| e.to_string())?;
        let style = if display {
            latex_rust::MathStyle::Display
        } else {
            latex_rust::MathStyle::Text
        };
        let math_box =
            latex_rust::layout_with_em_size_pt(&ast, &font, style, &options.font_size_pt)
                .map_err(|e| e.to_string())?;
        let baseline = f32::from_bits(math_box.height.to_ieee32_bits()) * 16.;
        let light =
            latex_rust::render_svg(&math_box, &font, &options).map_err(|e| e.to_string())?;
        options.color = latex_rust::Color::rgb(235, 235, 235);
        let dark = latex_rust::render_svg(&math_box, &font, &options).map_err(|e| e.to_string())?;
        (light, dark, baseline)
    };
    let tree =
        usvg::Tree::from_str(&light, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size();
    if size.width() > 8192. || size.height() > 8192. || size.width() * size.height() > 16_000_000. {
        return Err("This diagram or formula is too large to preview.".into());
    }
    Ok(Graphic {
        light: Arc::new(Image::from_bytes(ImageFormat::Svg, light.into_bytes())),
        dark: Arc::new(Image::from_bytes(ImageFormat::Svg, dark.into_bytes())),
        width: size.width(),
        height: size.height(),
        baseline,
    })
}

fn node(
    name: &'static str,
    source: &str,
    original: &str,
    diagram: bool,
    display: bool,
) -> MarkdownNode {
    MarkdownNode::new(name, prepare(source, diagram, display))
        .text(source.to_owned())
        .markdown(original.to_owned())
}

struct RichBlock;
impl MarkdownPlugin for RichBlock {
    fn name(&self) -> &str {
        "rich-block"
    }
    fn is_block(&self) -> bool {
        true
    }
    fn parse(&self, ast: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let (source, diagram) = match ast {
            Node::Code(code) if code.lang.as_deref() == Some("mermaid") => {
                (code.value.as_str(), true)
            }
            Node::Code(code) if matches!(code.lang.as_deref(), Some("math" | "latex")) => {
                (code.value.as_str(), false)
            }
            Node::Math(math) => (math.value.as_str(), false),
            _ => return None,
        };
        Some(node(
            "rich-block",
            source,
            cx.node_source(ast).unwrap_or(source),
            diagram,
            true,
        ))
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        match node
            .data::<Result<Graphic, String>>()
            .expect("rich block data")
        {
            Ok(graphic) => div()
                .w_full()
                .py_2()
                .child(
                    div()
                        .w(px(graphic.width))
                        .max_w_full()
                        .rounded_md()
                        .overflow_hidden()
                        .child(
                            img(graphic.image(cx))
                                .w(px(graphic.width))
                                .max_w_full()
                                .aspect_ratio(graphic.width / graphic.height)
                                .object_fit(ObjectFit::Contain),
                        ),
                )
                .into_any_element(),
            Err(error) => div()
                .w_full()
                .p_3()
                .child(format!(
                    "Preview unavailable: {error}\n\n{}",
                    node.as_text()
                ))
                .into_any_element(),
        }
    }
}

struct InlineMath;
impl MarkdownPlugin for InlineMath {
    fn name(&self) -> &str {
        "inline-math"
    }
    fn parse(&self, ast: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::InlineMath(math) = ast else {
            return None;
        };
        // Avoid claiming ordinary currency pairs such as "$5 and $10".
        if math.value.starts_with(char::is_whitespace) || math.value.ends_with(char::is_whitespace)
        {
            return None;
        }
        Some(node(
            "inline-math",
            &math.value,
            cx.node_source(ast).unwrap_or(&math.value),
            false,
            false,
        ))
    }
    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        match node
            .data::<Result<Graphic, String>>()
            .expect("inline math data")
        {
            Ok(graphic) => {
                let scale = f32::from(context.font_size()) / 16.;
                Some(
                    InlineElement::new(
                        img(graphic.image(cx))
                            .w(px(graphic.width * scale))
                            .h(px(graphic.height * scale)),
                    )
                    .with_baseline(px(graphic.baseline * scale)),
                )
            }
            // Keep delimiters visible when unsupported syntax cannot be rendered.
            Err(_) => Some(InlineElement::new(node.as_markdown().to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prepare;
    use gpui_kit::SvgRenderer;
    use std::sync::Arc;

    #[test]
    fn renders_diagrams_and_latex_offline() {
        for source in [
            "flowchart LR\n A[输入] --> B[处理] --> C[输出]",
            "sequenceDiagram\n participant A\n participant B\n A->>B: Hello",
        ] {
            let g = prepare(source, true, true).unwrap();
            assert!(g.width > 10. && g.height > 10.);
            // Exercise the same SVG decoder used by the application, including labels.
            g.light
                .to_image_data(SvgRenderer::new(Arc::new(())))
                .unwrap();
        }
        for source in [
            r"E=mc^2",
            r"\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
            r"\sum_{i=1}^{n} x_i",
        ] {
            let g = prepare(source, false, true).unwrap();
            assert!(g.width > 0. && g.height > 0.);
            g.light
                .to_image_data(SvgRenderer::new(Arc::new(())))
                .unwrap();
            g.dark
                .to_image_data(SvgRenderer::new(Arc::new(())))
                .unwrap();
        }
    }

    #[test]
    fn invalid_and_oversized_input_has_a_recoverable_error() {
        assert!(prepare("not a diagram", true, true).is_err());
        assert!(prepare(r"\notARealCommand{x}", false, false).is_err());
        assert!(prepare(&"x".repeat(4097), false, true).is_err());
        assert!(prepare(&"A-->B\n".repeat(257), true, true).is_err());
    }
}

#[cfg(feature = "qa")]
pub fn verify_frames() {
    use gpui_kit::test::TestWindowExt;
    const SAMPLE: &str = r#"# 图表与公式 · Rendering acceptance

Architecture / 架构图：

```mermaid
flowchart LR
    A[用户请求] --> B[研究引擎]
    B --> C[证据与结论]
```

Inline math: $E=mc^2$ and $\frac{1}{2}$ beside normal text.

$$
\sigma^2 = \frac{1}{n}\sum_{i=1}^{n}(x_i-\bar{x})^2
$$

| 类型 | 内容 |
| --- | --- |
| 公式 | $x^2+y^2=z^2$ |

Literal code: `$x^2$`. Currency: \$5 and \$10.

```latex
\notARealCommand{x}
```
"#;
    struct Preview;
    impl Render for Preview {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .p_4()
                .bg(rgb(crate::appearance::palette(cx).panel))
                .text_color(rgb(crate::appearance::palette(cx).text))
                .child(
                    base::text::TextView::markdown("preview", SAMPLE)
                        .markdown_extensions(extensions())
                        .w_full(),
                )
        }
    }
    std::fs::create_dir_all(".local/rich-markdown-qa").unwrap();
    for (name, appearance, width) in [
        ("light", crate::appearance::Appearance::Light, 900.),
        ("dark", crate::appearance::Appearance::Dark, 900.),
        ("narrow", crate::appearance::Appearance::Light, 420.),
    ] {
        let mut cx = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(crate::ui_assets::AppAssets),
            platform::current_headless_renderer,
        );
        cx.update(|cx| {
            init(cx);
            crate::appearance::apply(appearance, None, cx);
        });
        let handle: AnyWindowHandle = cx
            .open_window(size(px(width), px(1000.)), |w, cx| {
                let view = cx.new(|_| Preview);
                cx.new(|cx| base::Root::new(view, w, cx))
            })
            .unwrap()
            .into();
        for _ in 0..60 {
            cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                .unwrap();
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        cx.capture_screenshot(handle)
            .unwrap()
            .save(format!(".local/rich-markdown-qa/{name}.png"))
            .unwrap();
        cx.update(|cx| cx.quit());
    }
    println!("Rich Markdown frames saved in .local/rich-markdown-qa");
}
