//! Bridge configuration for the local ego(lite) runtime. Browser APIs come from its installed guide.
use std::path::{Path, PathBuf};
pub fn executable(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() || !path.is_file() {
        return Err("ego-browser is not installed at this path. Install ego(lite), complete its first launch, then select ~/.local/bin/ego-browser.".into());
    }
    Ok(path)
}
pub fn probe(path: &str, directory: &Path) -> Result<String, String> {
    let path = executable(path)?;
    let guide = crate::opencode::capture(&path, directory, &["skill"])?;
    if !guide.contains("[ego-browser:skill] end of guide") {
        return Err("ego-browser did not return its complete skill guide. Update ego(lite) and finish onboarding.".into());
    }
    Ok("Local runtime ready · browser opens in its own Space".into())
}
pub fn instructions(path: &str, space_name: &str, goal_title: &str) -> Result<String, String> {
    let path = executable(path)?;
    // Serialized strings describe arguments to the agent; they are never executed as shell text here.
    let command = serde_json::to_string(&path.to_string_lossy()).unwrap();
    Ok(format!(
        "\nBROWSER INTEGRATION\nExecutable path (JSON string): {command}\nBefore browser actions execute this executable with the single argument `skill`, read its FULL locally installed guide (ending [ego-browser:skill] end of guide), then use its documented nodejs stdin protocol. Use the local guide's API, not memorized examples. Use one Space named `{space_name}` for this goal ({goal_title}); reuse that exact Space across subtasks. Do not touch the user's other tabs or Spaces. Keep the Space available for later steps and user review. Any script file paths must be absolute; ego's Node runtime may not inherit cwd or environment. Use browser navigation, snapshots, clicks and screenshots to collect evidence. Do not log in, enter credentials, accept purchases or submit financial orders. If a login, CAPTCHA, browser permission prompt or Space handoff requires the user, stop and report blocked with the Space name. Wait for explicit user confirmation before taking over the same Space. Do not bypass the handoff through webfetch, another browser or a new Space.\n"
    ))
}
#[cfg(test)]
mod tests {
    #[test]
    fn missing_runtime_is_actionable() {
        assert!(
            super::executable("/no/such/ego-browser")
                .unwrap_err()
                .contains("Install ego(lite)")
        );
        assert!(super::executable("ego-browser").is_err());
    }
}
