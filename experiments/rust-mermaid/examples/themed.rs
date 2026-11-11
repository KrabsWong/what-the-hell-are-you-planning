//! Render one input with a named upstream palette, using only the public API.
use beautiful_mermaid_rs::{RenderOptions, render, theme};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: themed INPUT.mmd THEME OUTPUT.svg".into());
    }
    let options = RenderOptions {
        colors: theme::theme(&args[2]).ok_or("Unknown theme")?,
        ..RenderOptions::default()
    };
    std::fs::write(
        &args[3],
        render(&std::fs::read_to_string(&args[1])?, &options)?,
    )?;
    Ok(())
}
