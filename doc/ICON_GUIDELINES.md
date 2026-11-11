# Interface Icon Guidelines

## Source and selection

Use [Lucide Icons](https://lucide.dev/icons/) for all application interface icons. Search the official catalog, open the candidate's detail page, and confirm its meaning at the intended size. Check that the icon exists in the project's pinned `gpui-kit-assets` dependency; the website can be newer than the dependency. Do not edit Cargo's registry cache or upgrade dependencies solely to access an unnecessary icon.

The current `gpui-kit` dependency is 0.9.1. Its assets package records Lucide provenance in `lucide.json` and includes `LICENSE-LUCIDE`. Existing `IconName` components already use Lucide. `IconName::Close` is the package alias for Lucide's `x`.

### Semantic choices

| Purpose | Lucide icon / Rust variant |
| --- | --- |
| Replanned research | `notebook-pen` / `NotebookPen`, with a Replanned tooltip |
| Edit a document | `file-pen-line` / `FilePenLine` |
| Refresh a resource | `refresh-cw` / `RefreshCw` |
| Retry execution | `rotate-cw` / `RotateCw` |
| Completed / selected | `check` / `Check` |
| Pending / running / blocked | `Circle` / `CircleDot` / `CircleAlert` |
| Close | `x` / `Close` |
| Contextual help | `circle-question-mark` / `CircleQuestionMark`, with a topic-specific accessible name and tooltip |
| Inspect a diagram | `expand` / `Expand`, with an Open diagram accessible name |
| External link | `external-link` / `ExternalLink` |
| Drill down / advance | `ChevronRight` / `ArrowRight` |
| Priority: none, low, medium, high, urgent | `Minus`, `ArrowDown`, `Equal`, `ArrowUp`, `CircleAlert` |

Replanned means a revised research plan, not a Git branch or an operation currently refreshing. Notebook-and-pen is a visual cue, not a complete explanation: retain the Replanned tooltip, accessible label, and original-topic context. Use the existing theme accent rather than inventing another status color.

## Native app implementation

1. Reference `gpui_kit::assets::IconName::<Variant>`.
2. Add the variant to `assets::icon_assets!(AppIcons, [...])` in `src/ui_assets.rs`. This embeds SVG bytes for both debug and release; runtime must not depend on the current directory or a remote CDN.
3. Use `controls::tool_icon(id, icon, accessible_name)` for icon-only actions. Use `.icon(icon)` on labeled buttons. Use `component::Icon::new(icon).size_4()` for passive indicators. Keep a fixed footprint and theme-derived color. Icon-only actions use the main-branch ghost appearance: the icon stays visible without a permanent button surface or border, while its full hit area, hover/focus feedback, tooltip and accessible name remain intact.
4. Reuse `ExecutionStatus::icon()` for stage/status mappings rather than duplicating glyph mappings. Property menus accept `Option<IconName>`.
5. Do not draw UI icons with Unicode characters, emoji, custom SVG paths, or geometric `Div` shapes. Ordinary text punctuation and shortcut symbols remain text.

The diagram Expand control is an explicit visual exception: retain a small secondary button fill and outline at rest so its click target is clear inside the diagram frame.

Do not replace the product logo, Dock artwork, native macOS window controls, user content, charts, or mathematical rendering with Lucide icons.

## Exported report controls

`assets/report/theme-picker.html` embeds Lucide SVG markup from the same pinned assets package for Sun, Moon, Monitor, ChevronDown, and Check. Keep the upstream 24×24 viewBox, stroke width 2, and `currentColor`; size through CSS. Decorative SVGs use `aria-hidden="true"`; the enclosing control provides its name. Preserve `assets/report/LUCIDE-LICENSE`. Exported HTML includes the notice so standalone files retain attribution.

## Verification

- Run `python3 script/check_icons.py`: all Rust `IconName` references must be registered in `AppIcons`.
- Run `cargo test --quiet`: embedded asset tests verify actual bytes.
- Run relevant isolated QA (`--qa-board-topics`, `--qa-settings`, `--qa-affordance`, or `--qa-onboarding`) and inspect light/dark screenshots for missing icons, alignment, and accessible action labels.
- QA fixtures must not restart the user's real app or modify their workspace.
