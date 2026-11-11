use beautiful_mermaid_rs::{RenderOptions, layout, render, theme};
use std::{path::Path, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: beautiful-mermaid-rs INPUT.mmd OUTPUT_DIRECTORY".into());
    }
    let source = std::fs::read_to_string(&args[1])?;
    let output = Path::new(&args[2]);
    std::fs::create_dir_all(output)?;
    let start = Instant::now();
    for (name, palette) in [("light", "zinc-light"), ("dark", "zinc-dark")] {
        let options = RenderOptions {
            colors: theme::theme(palette).ok_or("Missing theme")?,
            ..RenderOptions::default()
        };
        std::fs::write(
            output.join(format!("{name}.svg")),
            render(&source, &options)?,
        )?;
    }
    // Typed graph geometry is available for flowcharts and states. Other diagram
    // families expose their own positioned representations through their modules.
    if let Ok(positioned) = layout(&source, &RenderOptions::default()) {
        std::fs::write(
            output.join("positioned.json"),
            serde_json::to_string_pretty(&positioned)?,
        )?;
    }
    eprintln!(
        "Rendered light/dark SVGs in {:.2} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}
