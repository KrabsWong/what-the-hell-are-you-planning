//! Publish generated report assets through a dedicated, disposable Git checkout.
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::{Component, Path, PathBuf}, process::{Command, Stdio}, time::{Duration, Instant}};
use std::os::unix::{fs::DirBuilderExt, process::CommandExt};

pub fn validate(repository: &str, branch: &str) -> Result<(), String> {
    let path = repository.strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("git@github.com:"))
        .ok_or("Use an HTTPS or SSH github.com repository URL without credentials.")?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.is_empty() || p.starts_with('.') || !p.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))) {
        return Err("Enter a GitHub repository URL containing owner/repository.".into());
    }
    if branch.is_empty() || branch.starts_with('-') || branch.starts_with("refs/") {
        return Err("Enter a branch name, such as main.".into());
    }
    git(None, &["check-ref-format", &format!("refs/heads/{branch}")])?;
    Ok(())
}

struct Checkout(PathBuf);
impl Drop for Checkout { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

/// Credentials come from the user's Git credential helper or SSH agent, never app settings.
pub fn publish(root: &Path, repository: &str, branch: &str, files: &[(String, Vec<u8>)], site_url: &str, progress: impl Fn(&str)) -> Result<String, String> {
    validate(repository, branch)?;
    publish_with_progress(root, repository, branch, files, site_url, &progress)
}

#[cfg(test)]
fn publish_inner(root: &Path, repository: &str, branch: &str, files: &[(String, Vec<u8>)]) -> Result<String, String> {
    publish_with_progress(root, repository, branch, files, "https://reports.example.com", &|_| {})
}

fn publish_with_progress(root: &Path, repository: &str, branch: &str, files: &[(String, Vec<u8>)], site_url: &str, progress: &dyn Fn(&str)) -> Result<String, String> {
    progress("Fetching repository");
    if files.is_empty() { return Err("There are no report files to publish.".into()); }
    fs::create_dir_all(root).map_err(|_| "Could not create the report publishing folder.")?;
    let name = format!("report-publish-{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let checkout = Checkout(root.join(name));
    fs::DirBuilder::new().mode(0o700).create(&checkout.0).map_err(|_| "Could not prepare the report checkout.")?;
    let dir = Some(checkout.0.as_path());
    git(dir, &["init", "--quiet"])?;
    // This checkout never executes hooks from user configuration or remote content.
    git(dir, &["config", "core.hooksPath", "/dev/null"])?;
    git(dir, &["remote", "add", "origin", repository])?;
    git(dir, &["fetch", "--no-tags", "origin", "+refs/heads/*:refs/remotes/origin/*"])?;
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote = git(dir, &["show-ref", "--verify", "--hash", &remote_ref]).ok();
    if let Some(base) = &remote {
        git(dir, &["checkout", "--detach", base.trim()])?;
    } else {
        let heads = git(dir, &["for-each-ref", "--format=%(refname)", "refs/remotes/origin/"])?;
        if !heads.trim().is_empty() {
            return Err("The target branch does not exist. Create it in GitHub first to preserve the existing site.".into());
        }
        git(dir, &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")])?;
    }
    progress("Preparing report commit");
    for (name, bytes) in files {
        let path = safe_destination(&checkout.0, name)?;
        fs::create_dir_all(path.parent().unwrap()).map_err(|_| "Could not create report directory.")?;
        fs::write(&path, bytes).map_err(|_| "Could not write report assets.")?;
        git(dir, &["add", "--", name])?;
    }
    for name in prepare_entry_pages(&checkout.0)? {
        git(dir, &["add", "--", name])?;
    }
    prepare_repository_index(&checkout.0, site_url)?;
    git(dir, &["add", "--", "README.md", "edgeone.json", "build-reports.mjs"])?;
    let changed = git(dir, &["diff", "--cached", "--name-only"])?;
    if changed.trim().is_empty() {
        return git(dir, &["rev-parse", "HEAD"]).map(|s| s.trim().to_owned());
    }
    git(dir, &["-c", "user.name=Planning Reports", "-c", "user.email=reports@users.noreply.github.com", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Publish research report", "-m", "Update generated HTML report assets and preserve existing report URLs.\n\nValidation: generated assets checked for safe paths; outgoing Git objects scanned before push."])?;
    let head = git(dir, &["rev-parse", "HEAD"] )?.trim().to_owned();
    // Scan the exact outgoing history against the fetched destination before each push.
    progress("Checking outgoing history");
    scan_outgoing(&checkout.0, &head, remote.as_deref().map(str::trim))?;
    // A concurrent remote update is rejected by Git; retry starts with a fresh fetch.
    progress("Pushing to GitHub");
    git(dir, &["push", "origin", &format!("{head}:refs/heads/{branch}")])?;
    Ok(head)
}

const LEGACY_ENTRY: &str = "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"robots\" content=\"noindex,nofollow,noarchive\"><meta name=\"referrer\" content=\"no-referrer\"><title>Report Something</title><body><p>请使用完整的报告链接访问。</p></body></html>";

const ENTRY_TEMPLATE: &str = include_str!("../assets/report-publishing/entry.html");

fn entry_page(not_found: bool) -> String {
    let (title, intro) = if not_found {
        ("404 · 报告未找到", "请确认报告链接是否完整，或稍后重试。")
    } else {
        ("研究报告", "请通过分享链接访问具体报告。")
    };
    ENTRY_TEMPLATE.replace("{{TITLE}}", title).replace("{{INTRO}}", intro)
}

fn prepare_entry_pages(root: &Path) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    for (name, content) in [("index.html", entry_page(false)), ("404.html", entry_page(true)), ("robots.txt", "User-agent: *\nDisallow: /\n".into())] {
        let path = safe_destination(root, name)?;
        let replace = match fs::read_to_string(&path) {
            Ok(existing) => {
                let previous = include_str!("../assets/report-publishing/entry-v1.html")
                    .replace("{{TITLE}}", if name == "404.html" { "404 · 报告未找到" } else { "研究报告" })
                    .replace("{{INTRO}}", if name == "404.html" { "请确认报告链接是否完整，或稍后重试。" } else { "请通过分享链接访问具体报告。" });
                name != "robots.txt" && (existing == LEGACY_ENTRY || existing == previous)
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => false, // Preserve custom or unreadable site files.
        };
        if replace {
            fs::write(path, content).map_err(|_| "Could not create site entry files.")?;
            changed.push(name);
        }
    }
    Ok(changed)
}

const INDEX_START: &str = "<!-- planning-reports:start -->";
const INDEX_END: &str = "<!-- planning-reports:end -->";
const BUILD: &str = include_str!("../assets/report-publishing/build.mjs");

fn prepare_repository_index(root: &Path, site_url: &str) -> Result<(), String> {
    // Install the deployment boundary before introducing a private repository index.
    let config_path = safe_destination(root, "edgeone.json")?;
    let mut config: serde_json::Value = if config_path.exists() {
        serde_json::from_slice(&fs::read(&config_path).map_err(|_| "Cannot read edgeone.json.")?)
            .map_err(|_| "Invalid edgeone.json; configure the report deployment before publishing.")?
    } else { serde_json::json!({}) };
    let object = config.as_object_mut().ok_or("edgeone.json must be an object.")?;
    for (key, value) in [("buildCommand", "node build-reports.mjs"), ("outputDirectory", "report-public")] {
        if object.get(key).is_some_and(|existing| existing.as_str() != Some(value)) {
            return Err(format!("Existing edgeone.json {key} conflicts with the private report index. Configure {key} as {value} before retrying."));
        }
        object.insert(key.into(), value.into());
    }
    let build_path = safe_destination(root, "build-reports.mjs")?;
    if build_path.exists() && fs::read_to_string(&build_path).map_err(|_| "Cannot read build-reports.mjs.")? != BUILD {
        return Err("Existing build-reports.mjs differs from the report publisher. Review it before retrying.".into());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| "Cannot enumerate report directories.")? {
        let entry = entry.map_err(|_| "Cannot read report directory.")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() != 32 || !name.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) { continue; }
        let page = safe_destination(root, &format!("{name}/index.html"))?;
        if !page.is_file() { continue; }
        let html = fs::read_to_string(page).map_err(|_| "Cannot read existing report HTML.")?;
        let title = html.split_once("<title>").and_then(|(_, rest)| rest.split_once("</title>").map(|(title, _)| title))
            .unwrap_or("Untitled report").trim_end_matches(" · Report Something");
        let title = title.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
        // Encode Markdown table/control punctuation; report titles remain literal text.
        let title: String = title.chars().map(|c| match c {
            '\n' | '\r' => " ".into(),
            c if "|<>[]*_`\\".contains(c) => format!("&#{};", c as u32),
            _ => c.to_string(),
        }).collect();
        entries.push((name, title));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut index = format!("{INDEX_START}\n## 报告索引\n\n此索引仅供私有 GitHub 仓库内查看，不包含在 EdgeOne 部署产物中。\n\n| Topic | 目录 | 分享链接 |\n| --- | --- | --- |\n");
    for (id, title) in entries {
        index.push_str(&format!("| {title} | [{id}](./{id}/index.html) | [打开报告]({site_url}/{id}/) |\n"));
    }
    index.push_str(&format!("\n{INDEX_END}"));
    let readme_path = safe_destination(root, "README.md")?;
    let existing = if readme_path.exists() { fs::read_to_string(&readme_path).map_err(|_| "Cannot read README.md.")? } else { "# Report Something\n".into() };
    let readme = match (existing.find(INDEX_START), existing.find(INDEX_END)) {
        (Some(start), Some(end)) if end > start => format!("{}{}{}", &existing[..start], index, &existing[end + INDEX_END.len()..]),
        (None, None) => format!("{}\n\n{index}\n", existing.trim_end()),
        _ => return Err("README report index markers are malformed; repair them before publishing.".into()),
    };
    fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).map_err(|_| "Cannot write EdgeOne deployment configuration.")?;
    fs::write(build_path, BUILD).map_err(|_| "Cannot write public report build script.")?;
    fs::write(readme_path, readme).map_err(|_| "Cannot write report index.")?;
    Ok(())
}

fn safe_destination(root: &Path, name: &str) -> Result<PathBuf, String> {
    let mut path = root.to_owned();
    if name.is_empty() { return Err("Empty report asset path.".into()); }
    for component in Path::new(name).components() {
        let Component::Normal(part) = component else { return Err("Unsafe report asset path.".into()); };
        let part = part.to_string_lossy();
        if part.starts_with('.') || part.contains('\\') { return Err("Hidden or unsafe report asset path.".into()); }
        path.push(part.as_ref());
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Report destination contains a symbolic link.".into());
        }
    }
    Ok(path)
}

fn scan_outgoing(dir: &Path, head: &str, base: Option<&str>) -> Result<(), String> {
    let range = base.map(|base| format!("{base}..{head}")).unwrap_or_else(|| head.to_owned());
    let objects = git(Some(dir), &["rev-list", "--objects", &range])?;
    for object in objects.lines() {
        let oid = object.split_once(' ').map_or(object, |(oid, _)| oid);
        let kind = git(Some(dir), &["cat-file", "-t", oid])?;
        if !matches!(kind.trim(), "blob" | "commit" | "tag") { continue; }
        let size = git(Some(dir), &["cat-file", "-s", oid])?.trim().parse::<usize>().map_err(|_| "Could not determine outgoing object size.")?;
        if size > 32 * 1024 * 1024 { return Err("An outgoing file is too large to scan safely.".into()); }
        let content = git(Some(dir), &["cat-file", "-p", oid])?;
        if suspected_secret(&content) {
            let fingerprint = format!("{:x}", Sha256::digest(content.as_bytes()));
            return Err(format!("Push stopped: suspected credential in outgoing object {}; fingerprint {}. Remove the credential before retrying.", &oid[..12.min(oid.len())], &fingerprint[..12]));
        }
    }
    Ok(())
}

// Scan both original bytes and visible text: rendering must not hide assignments
// behind emphasis, code spans, HTML tags, or escaped punctuation.
fn suspected_secret(content: &str) -> bool {
    scan_secret_text(content) || scan_secret_text(&visible_text(content))
}

fn visible_text(content: &str) -> String {
    let mut text = String::with_capacity(content.len());
    let mut rest = content;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            if let Some(end) = rest.find('>') {
                let tag = rest[1..end].trim_start_matches('/').split_whitespace().next().unwrap_or("");
                if matches!(tag, "p" | "div" | "br" | "tr" | "li" | "pre") { text.push('\n'); }
                rest = &rest[end + 1..];
                continue;
            }
        }
        if rest.starts_with('&') {
            if let Some(end) = rest.find(';').filter(|end| *end <= 12) {
                let entity = &rest[1..end];
                let decoded = match entity {
                    "quot" => Some('"'), "apos" | "#39" => Some('\''),
                    "amp" => Some('&'), "lt" => Some('<'), "gt" => Some('>'),
                    "nbsp" => Some(' '), "colon" => Some(':'), "equals" => Some('='),
                    value if value.starts_with("#x") || value.starts_with("#X") => u32::from_str_radix(&value[2..], 16).ok().and_then(char::from_u32),
                    value if value.starts_with('#') => value[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                if let Some(decoded) = decoded {
                    text.push(decoded); rest = &rest[end + 1..]; continue;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        text.push(c); rest = &rest[c.len_utf8()..];
    }
    text
}

fn scan_secret_text(content: &str) -> bool {
    for line in content.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("private key-----") || lower.contains("authorization: bearer ") || lower.contains("authorization: basic ") { return true; }
        for prefix in ["ghp_", "github_pat_", "gho_", "ghs_", "sk-proj-", "AKIA", "ASIA", "xoxb-", "xoxp-"] {
            if line.match_indices(prefix).any(|(i, _)| line[i + prefix.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').count() >= 16) { return true; }
        }
        for url in line.split_whitespace().filter(|s| s.contains("://")) {
            if let Some((_, rest)) = url.split_once("://") {
                if rest.split('/').next().is_some_and(|authority| authority.contains('@') && authority.contains(':')) { return true; }
            }
        }
        for key in ["api_key", "apikey", "api_token", "access_token", "secret_key", "client_secret", "password", "aws_secret_access_key"] {
            for (start, _) in lower.match_indices(key) {
                if lower[..start].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric()) { continue; }
                let tail = lower[start + key.len()..].trim_start_matches(|c: char| c.is_whitespace() || "\"'`*_".contains(c));
                let Some(tail) = tail.strip_prefix('=').or_else(|| tail.strip_prefix(':')) else { continue; };
                let value = tail.trim_start_matches(|c: char| c.is_whitespace() || "\"'`*".contains(c))
                    .split(|c: char| c.is_whitespace() || "\"'`,;<>".contains(c)).next().unwrap_or("");
                if value.len() >= 12 && !value.contains("example") && !value.contains("placeholder") && !value.starts_with('$') {
                    return true;
                }
            }
        }
    }
    false
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.args(["-c", "core.hooksPath=/dev/null", "-c", "credential.interactive=false"])
        .args(args).env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "Never")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=15")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    if let Some(dir) = dir { command.current_dir(dir); }
    let mut child = command.spawn().map_err(|_| "Git is unavailable. Install Git and configure GitHub authentication.")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |stream: Box<dyn Read + Send>| std::thread::spawn(move || { let mut bytes = Vec::new(); let result = stream.take(64 * 1024 * 1024).read_to_end(&mut bytes); (result, bytes) });
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < Duration::from_secs(120) => std::thread::sleep(Duration::from_millis(50)),
            _ => { unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL); } let _ = child.kill(); let _ = child.wait(); break Err("Git operation timed out. Check network and GitHub authentication.".to_owned()); }
        }
    };
    let (read_result, bytes) = out.join().map_err(|_| "Could not read Git output.")?;
    let _ = err.join();
    if !status?.success() { return Err(format!("Git {} failed. Check repository access, branch permissions and remote updates, then retry.", args.first().unwrap_or(&"operation"))); }
    read_result.map_err(|_| "Could not read Git output.")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_pages_upgrade_only_legacy_defaults_and_preserve_custom_content() {
        let root = std::env::temp_dir().join(format!("report-entry-test-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        fs::create_dir_all(&root).unwrap();
        let _cleanup = Checkout(root.clone());
        assert_eq!(prepare_entry_pages(&root).unwrap(), ["index.html", "404.html", "robots.txt"]);
        let home = fs::read_to_string(root.join("index.html")).unwrap();
        let missing = fs::read_to_string(root.join("404.html")).unwrap();
        assert_ne!(home, missing);
        assert!(home.contains("请通过分享链接访问具体报告。"));
        assert!(missing.contains("404 · 报告未找到"));
        for page in [&home, &missing] {
            assert!(page.contains("noindex,nofollow,noarchive"));
            assert!(page.contains("prefers-color-scheme: dark"));
            assert!(!page.contains("{{"));
            assert!(page.contains("https://weekly.tw93.fun/fonts/jinkai.css"));
            assert!(page.contains("TsangerJinKai02"));
            assert!(!page.contains("<nav"));
            assert!(!page.contains("Yooooo"));
        }
        assert!(prepare_entry_pages(&root).unwrap().is_empty());
        let previous = include_str!("../assets/report-publishing/entry-v1.html")
            .replace("{{TITLE}}", "404 · 报告未找到")
            .replace("{{INTRO}}", "请确认报告链接是否完整，或稍后重试。");
        fs::write(root.join("404.html"), previous).unwrap();
        assert_eq!(prepare_entry_pages(&root).unwrap(), ["404.html"]);
        assert_eq!(fs::read_to_string(root.join("404.html")).unwrap(), missing);
        fs::write(root.join("index.html"), "Custom home").unwrap();
        fs::write(root.join("404.html"), LEGACY_ENTRY).unwrap();
        fs::write(root.join("robots.txt"), "Custom rules").unwrap();
        assert_eq!(prepare_entry_pages(&root).unwrap(), ["404.html"]);
        assert_eq!(fs::read_to_string(root.join("index.html")).unwrap(), "Custom home");
        assert_eq!(fs::read_to_string(root.join("404.html")).unwrap(), missing);
        assert_eq!(fs::read_to_string(root.join("robots.txt")).unwrap(), "Custom rules");
        fs::write(root.join("404.html"), "Custom error").unwrap();
        fs::write(root.join("index.html"), LEGACY_ENTRY).unwrap();
        assert_eq!(prepare_entry_pages(&root).unwrap(), ["index.html"]);
        assert_eq!(fs::read_to_string(root.join("index.html")).unwrap(), home);
        assert_eq!(fs::read_to_string(root.join("404.html")).unwrap(), "Custom error");
    }

    #[test]
    fn rejects_credentials_and_unsafe_paths() {
        assert!(validate("https://github.com/owner/reports.git", "main").is_ok());
        assert!(validate("git@github.com:owner/reports.git", "reports/live").is_ok());
        for url in ["https://token@github.com/a/b", "https://example.com/a/b", "https://github.com/a/b?token=x"] { assert!(validate(url, "main").is_err()); }
        for branch in ["-bad", "main:other", "../main"] { assert!(validate("https://github.com/a/b", branch).is_err()); }
        for path in ["../secret", "/tmp/report", ".git/config", "a/../../x", "a\\b"] { assert!(safe_destination(Path::new("/tmp"), path).is_err()); }
    }
    #[test]
    fn rejects_symlink_destinations() {
        let root = std::env::temp_dir().join(format!("report-path-test-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        fs::create_dir_all(&root).unwrap();
        let _cleanup = Checkout(root.clone());
        std::os::unix::fs::symlink("/tmp", root.join("linked")).unwrap();
        assert!(safe_destination(&root, "linked/index.html").is_err());
    }
    #[test]
    fn repository_index_preserves_readme_and_is_excluded_from_build() {
        let root = std::env::temp_dir().join(format!("report-index-test-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        fs::create_dir_all(&root).unwrap();
        let _cleanup = Checkout(root.clone());
        let id = "a".repeat(32);
        let old_id = "b".repeat(32);
        for (id, title) in [(&id, "Topic &amp; &lt;private&gt; | [label] · Report Something"), (&old_id, "Older topic")] {
            fs::create_dir(root.join(id)).unwrap();
            fs::write(root.join(id).join("index.html"), format!("<title>{title}</title>Report")).unwrap();
        }
        fs::write(root.join("README.md"), "# Existing repository notes\nKeep this introduction.\n").unwrap();
        fs::write(root.join("index.html"), "Public entry").unwrap();
        fs::write(root.join("private-notes.txt"), "Do not deploy").unwrap();
        fs::write(root.join("edgeone.json"), r#"{"headers":[]}"#).unwrap();
        prepare_repository_index(&root, "https://reports.example.com").unwrap();
        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        assert!(readme.contains("Keep this introduction."));
        assert!(readme.contains("Older topic"));
        assert!(readme.contains("Topic & &#60;private&#62; &#124; &#91;label&#93;"));
        assert!(readme.contains(&format!("https://reports.example.com/{id}/")));
        prepare_repository_index(&root, "https://reports.example.com").unwrap();
        assert_eq!(readme, fs::read_to_string(root.join("README.md")).unwrap());
        let config: serde_json::Value = serde_json::from_slice(&fs::read(root.join("edgeone.json")).unwrap()).unwrap();
        assert_eq!(config["headers"], serde_json::json!([]));
        assert_eq!(config["outputDirectory"], "report-public");
        for _ in 0..2 {
            let output = Command::new("node").arg("build-reports.mjs").current_dir(&root).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let public = root.join("report-public");
            assert!(!public.join("README.md").exists());
            assert!(!public.join("private-notes.txt").exists());
            assert!(!public.join("edgeone.json").exists());
            assert!(!public.join("build-reports.mjs").exists());
            assert_eq!(fs::read(public.join(&old_id).join("index.html")).unwrap(), fs::read(root.join(&old_id).join("index.html")).unwrap());
            assert!(public.join(&id).join("index.html").is_file());
        }
        fs::write(root.join("edgeone.json"), r#"{"outputDirectory":"custom"}"#).unwrap();
        assert!(prepare_repository_index(&root, "https://reports.example.com").is_err());
        assert_eq!(readme, fs::read_to_string(root.join("README.md")).unwrap());
    }

    #[test]
    fn preserves_reports_and_retries_idempotently() {
        let root = std::env::temp_dir().join(format!("report-git-test-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        fs::create_dir_all(&root).unwrap();
        let cleanup = Checkout(root.clone());
        let remote = root.join("remote.git");
        git(None, &["init", "--bare", remote.to_str().unwrap()]).unwrap();
        let files = vec![("first/index.html".into(), b"first".to_vec()), ("index.html".into(), b"existing landing".to_vec())];
        let stages = std::cell::RefCell::new(Vec::new());
        let first = publish_with_progress(&root, remote.to_str().unwrap(), "main", &files, "https://reports.example.com", &|stage| stages.borrow_mut().push(stage.to_string())).unwrap();
        assert_eq!(*stages.borrow(), ["Fetching repository", "Preparing report commit", "Checking outgoing history", "Pushing to GitHub"]);
        assert!(git(Some(&remote), &["show", "main:robots.txt"]).unwrap().contains("Disallow: /"));
        assert_eq!(first, publish_inner(&root, remote.to_str().unwrap(), "main", &files).unwrap());
        publish_inner(&root, remote.to_str().unwrap(), "main", &[("second/index.html".into(), b"second".to_vec())]).unwrap();
        assert_eq!(git(Some(&remote), &["show", "main:first/index.html"]).unwrap(), "first");
        assert_eq!(git(Some(&remote), &["show", "main:index.html"]).unwrap(), "existing landing");
        drop(cleanup);
    }
}
