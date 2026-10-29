//! Bounded local text extraction. Call import_material on a worker, cached_text before a run.
use super::{Attachment, MAX_BYTES, import_file, resolve};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const CACHE_VERSION: u32 = 1;
const MAX_CACHE_BYTES: u64 = 512 * 1024;
#[cfg(target_os = "macos")]
const HELPER_SOURCE: &str = include_str!("materials/extract.swift");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialPage {
    pub number: usize,
    pub method: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialText {
    pub pages: Vec<MaterialPage>,
    pub total_pages: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}
impl MaterialText {
    pub fn prompt_text(&self) -> String {
        let mut output = format!(
            "Local extraction: {} of {} page(s). Partial: {}.\n",
            self.pages.len(),
            self.total_pages,
            self.truncated
        );
        for warning in &self.warnings {
            output.push_str(&format!("Extraction warning: {warning}\n"));
        }
        for page in &self.pages {
            output.push_str(&format!(
                "\n[Page {} · {}]\n{}\n",
                page.number, page.method, page.text
            ));
        }
        output
    }
    fn validate(&self) -> Result<(), String> {
        if self.pages.is_empty()
            || self.pages.len() > 40
            || self.total_pages == 0
            || self.pages.iter().any(|p| {
                p.number == 0
                    || p.number > self.total_pages
                    || p.text.trim().is_empty()
                    || !matches!(p.method.as_str(), "PDFKit text" | "Vision OCR")
            })
            || self.pages.windows(2).any(|p| p[0].number >= p[1].number)
            || self
                .pages
                .iter()
                .map(|p| p.text.chars().count())
                .sum::<usize>()
                > 18_000
            || self.warnings.len() > 60
            || self.warnings.iter().any(|w| w.len() > 4000)
        {
            return Err("Invalid material extraction. Remove and add this material again.".into());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedMaterial {
    version: u32,
    source_sha256: String,
    text_sha256: String,
    material: MaterialText,
}

/// Copies first, then extracts on the caller's worker. Failed imports are not attached to a task.
pub fn import_material(data_dir: &Path, source: &Path) -> Result<Attachment, String> {
    let attachment = import_file(data_dir, source)?;
    if let Err(error) = extract_and_cache(data_dir, &attachment) {
        // This copy has not escaped to a task or run, so removal cannot affect history.
        if let Ok(path) = resolve(data_dir, &attachment) {
            let _ = fs::remove_file(path);
        }
        return Err(format!("{}: {error}", attachment.name));
    }
    Ok(attachment)
}
/// No subprocesses. Rejects missing or changed source/cache instead of silently using stale OCR.
pub fn cached_text(data_dir: &Path, attachment: &Attachment) -> Result<MaterialText, String> {
    let source_hash = source_digest(data_dir, attachment)?;
    let path = cache_path(data_dir, attachment)?;
    let bytes = read_regular(&path, MAX_CACHE_BYTES).map_err(|e| {
        format!(
            "Material text is unavailable ({e}). Remove and add {} again to extract it.",
            attachment.name
        )
    })?;
    let cached: CachedMaterial = serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "Invalid material cache: {e}. Remove and add {} again.",
            attachment.name
        )
    })?;
    let text_hash = digest(&serde_json::to_vec(&cached.material).map_err(|e| e.to_string())?);
    if cached.version != CACHE_VERSION
        || cached.source_sha256 != source_hash
        || cached.text_sha256 != text_hash
    {
        return Err(format!(
            "{} changed after extraction. Remove and add the material again before running.",
            attachment.name
        ));
    }
    cached.material.validate()?;
    Ok(cached.material)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn source_digest(data_dir: &Path, attachment: &Attachment) -> Result<String, String> {
    let bytes = read_regular(&resolve(data_dir, attachment)?, MAX_BYTES)?;
    if bytes.len() as u64 != attachment.size {
        return Err(format!(
            "{} changed after import. Remove and add it again.",
            attachment.name
        ));
    }
    Ok(digest(&bytes))
}
fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("Expected a bounded regular file, not a link or folder.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("File exceeds the material size limit.".into());
    }
    Ok(bytes)
}
fn cache_root(data_dir: &Path) -> Result<PathBuf, String> {
    let root = data_dir.join("material-cache");
    if !root.exists() {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    }
    let metadata = fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Material cache must be a local directory, not a symbolic link.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(root)
}
fn cache_path(data_dir: &Path, attachment: &Attachment) -> Result<PathBuf, String> {
    resolve(data_dir, attachment)?;
    let name = Path::new(&attachment.relative_path)
        .file_name()
        .ok_or("Missing material filename")?
        .to_string_lossy();
    Ok(cache_root(data_dir)?.join(format!("{name}.json")))
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("Cannot create material file: {e}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}
fn extract_and_cache(data_dir: &Path, attachment: &Attachment) -> Result<(), String> {
    let before = source_digest(data_dir, attachment)?;
    let material = extract_native(data_dir, &resolve(data_dir, attachment)?)?;
    material.validate()?;
    if before != source_digest(data_dir, attachment)? {
        return Err("Material changed during extraction. Add it again.".into());
    }
    let cached = CachedMaterial {
        version: CACHE_VERSION,
        source_sha256: before,
        text_sha256: digest(&serde_json::to_vec(&material).map_err(|e| e.to_string())?),
        material,
    };
    private_write(
        &cache_path(data_dir, attachment)?,
        &serde_json::to_vec(&cached).map_err(|e| e.to_string())?,
    )
}
#[cfg(not(target_os = "macos"))]
fn extract_native(_: &Path, _: &Path) -> Result<MaterialText, String> {
    Err("Local PDF and screenshot reading currently requires macOS PDFKit/Vision. Add the material text as a comment on this platform.".into())
}
#[cfg(target_os = "macos")]
fn extract_native(data_dir: &Path, source: &Path) -> Result<MaterialText, String> {
    use std::{process::Command, sync::Mutex, time::Duration};
    static COMPILER: Mutex<()> = Mutex::new(());
    let root = cache_root(data_dir)?;
    let helper = root.join(format!("reader-{}", digest(HELPER_SOURCE.as_bytes())));
    {
        let _guard = COMPILER
            .lock()
            .map_err(|_| "Material compiler lock failed")?;
        if !helper.exists() {
            let source_file = root.join(format!("reader-{}.swift", std::process::id()));
            let binary = root.join(format!("reader-{}.building", std::process::id()));
            if source_file.exists() || binary.exists() {
                return Err(
                    "An earlier material reader build was interrupted. Restart the app to retry."
                        .into(),
                );
            }
            private_write(&source_file, HELPER_SOURCE.as_bytes())?;
            let mut command = Command::new("/usr/bin/xcrun");
            command
                .args(["swiftc", "-O", "-module-cache-path"])
                .arg(root.join("swift-modules"))
                .arg(&source_file)
                .arg("-o")
                .arg(&binary);
            let result = run_bounded(command, Duration::from_secs(120));
            let _ = fs::remove_file(&source_file);
            if let Err(error) = result {
                let _ = fs::remove_file(&binary);
                return Err(format!(
                    "Cannot build local PDF/OCR reader. Install Xcode Command Line Tools and retry. {error}"
                ));
            }
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            fs::rename(&binary, &helper).map_err(|e| e.to_string())?;
        }
        if !fs::symlink_metadata(&helper)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("Material reader must be a regular file.".into());
        }
    }
    let mut command = Command::new(&helper);
    command.arg(source);
    let output = run_bounded(command, Duration::from_secs(120))?;
    serde_json::from_slice(&output).map_err(|e| format!("Invalid PDF/OCR reader response: {e}"))
}
#[cfg(target_os = "macos")]
fn run_bounded(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    use std::{
        os::unix::process::CommandExt,
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = |stream: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            stream
                .take(MAX_CACHE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let out = reader(Box::new(stdout));
    let err = reader(Box::new(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(30)),
            other => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break Err(match other { Err(e) => e.to_string(), _ => "Local material extraction timed out after 120 seconds. Split or simplify the material and retry.".into() });
            }
        }
    };
    let output = out
        .join()
        .map_err(|_| "Material reader output failed")?
        .map_err(|e| e.to_string())?;
    let errors = err
        .join()
        .map_err(|_| "Material reader diagnostics failed")?
        .map_err(|e| e.to_string())?;
    let status = status?;
    if !status.success() {
        return Err(String::from_utf8_lossy(&errors)
            .chars()
            .take(2000)
            .collect());
    }
    if output.len() as u64 > MAX_CACHE_BYTES {
        return Err("Material reader response exceeded its size limit.".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_and_invalid_page_metadata() {
        let mut text = MaterialText {
            pages: vec![],
            total_pages: 1,
            truncated: false,
            warnings: vec![],
        };
        assert!(text.validate().is_err());
        text.pages.push(MaterialPage {
            number: 1,
            method: "PDFKit text".into(),
            text: "Evidence".into(),
        });
        assert!(text.validate().is_ok());
        assert!(text.prompt_text().contains("[Page 1 · PDFKit text]"));
        text.pages.push(text.pages[0].clone());
        assert!(text.validate().is_err());
    }
    #[test]
    fn cache_detects_source_and_transcript_changes() {
        let root = std::env::temp_dir().join(format!("material-cache-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.pdf");
        fs::write(&source, b"%PDF-1.7\n").unwrap();
        let attachment = import_file(&root, &source).unwrap();
        let material = MaterialText {
            pages: vec![MaterialPage {
                number: 1,
                method: "PDFKit text".into(),
                text: "original".into(),
            }],
            total_pages: 1,
            truncated: false,
            warnings: vec![],
        };
        let cached = CachedMaterial {
            version: CACHE_VERSION,
            source_sha256: source_digest(&root, &attachment).unwrap(),
            text_sha256: digest(&serde_json::to_vec(&material).unwrap()),
            material,
        };
        let cache = cache_path(&root, &attachment).unwrap();
        private_write(&cache, &serde_json::to_vec(&cached).unwrap()).unwrap();
        assert!(cached_text(&root, &attachment).is_ok());
        let mut changed = cached;
        changed.material.pages[0].text = "changed".into();
        fs::write(&cache, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(cached_text(&root, &attachment).is_err());
        changed.material.pages[0].text = "original".into();
        fs::write(&cache, serde_json::to_vec(&changed).unwrap()).unwrap();
        fs::write(resolve(&root, &attachment).unwrap(), b"%PDF-1.8\n").unwrap();
        assert!(cached_text(&root, &attachment).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_cache_directories_and_entries() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("material-cache-links-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, root.join("material-cache")).unwrap();
        assert!(cache_root(&root).is_err());
        fs::remove_file(root.join("material-cache")).unwrap();
        let cache = cache_root(&root).unwrap();
        fs::write(outside.join("fake.json"), b"{}").unwrap();
        symlink(outside.join("fake.json"), cache.join("fake.json")).unwrap();
        assert!(read_regular(&cache.join("fake.json"), MAX_CACHE_BYTES).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Uses macOS PDFKit/Vision and compiles the local helper; run outside the agent sandbox"]
    fn native_pdf_screenshot_and_scan_are_read_and_blank_is_rejected() {
        let root =
            std::env::temp_dir().join(format!("material-native-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for (name, bytes, marker, method) in [
            (
                "supplement.pdf",
                include_bytes!("test-fixtures/supplement.pdf").as_slice(),
                "MEMORY-PDF-7341",
                "PDFKit text",
            ),
            (
                "supplement.png",
                include_bytes!("test-fixtures/supplement.png").as_slice(),
                "MEMORY-IMAGE-9265",
                "Vision OCR",
            ),
            (
                "scanned.pdf",
                include_bytes!("test-fixtures/scanned.pdf").as_slice(),
                "MEMORY-IMAGE-9265",
                "Vision OCR",
            ),
        ] {
            let source = root.join(name);
            fs::write(&source, bytes).unwrap();
            let attachment = import_material(&root, &source).unwrap();
            let material = cached_text(&root, &attachment).unwrap();
            assert_eq!(material.pages[0].method, method);
            assert_eq!(material.pages[0].number, 1);
            assert!(material.prompt_text().contains(marker));
            assert!(!material.truncated);
        }
        let before = fs::read_dir(root.join("attachments")).unwrap().count();
        let blank = root.join("blank.pdf");
        fs::write(&blank, include_bytes!("test-fixtures/blank.pdf")).unwrap();
        assert!(
            import_material(&root, &blank)
                .unwrap_err()
                .contains("No readable text")
        );
        assert_eq!(
            fs::read_dir(root.join("attachments")).unwrap().count(),
            before
        );
        fs::remove_dir_all(root).unwrap();
    }
}
