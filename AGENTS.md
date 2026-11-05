# Project rules for coding agents

Read `PRODUCT.md` and `DESIGN.md` before changing the interface. Keep implementation changes scoped to the user's request.

## Icons: Lucide only

- Find interface icons at https://lucide.dev/icons/. Check the icon's meaning, not just its shape. Prefer familiar task semantics; do not use Git icons for research planning.
- Follow `doc/ICON_GUIDELINES.md` for selection, implementation, resource embedding, and verification. This applies to navigation, buttons, statuses, menus, onboarding, and exported report controls.
- Reuse the Lucide assets in the pinned `gpui-kit-assets` dependency through `gpui_kit::assets::IconName`. Do not add another icon library or use Unicode/emoji as interface icons.
- Every referenced icon must be embedded in `src/ui_assets.rs` via `AppIcons`, including icons used only while loading or handling errors. An enum compiling does not prove its asset is bundled.
- Use `controls::tool_icon` for icon-only actions, `Button::icon` for labeled actions, and `component::Icon` for passive indicators. Supply meaningful tooltips and accessible names for icon-only controls.
- Before finishing icon changes, run `python3 script/check_icons.py`, `cargo test --quiet`, and the relevant isolated UI QA. Check both light and dark screenshots. Do not restart the user's running app unless requested.
- Product branding, native macOS window controls, diagrams, mathematical notation, punctuation, keyboard shortcut labels, and user-authored content are not interface icons; preserve them.

## Maintained documentation

When changing research behavior, keep `doc/RESEARCH_WORKFLOW_STRATEGY.md` consistent with the implementation. When changing icon conventions, update `doc/ICON_GUIDELINES.md`.
