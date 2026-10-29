//! Immutable local copies of task materials. Removing a task reference keeps the copy for undo.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[path = "materials.rs"]
mod materials;
pub use materials::{cached_text, import_material};

pub const MAX_BYTES: u64 = 25 * 1024 * 1024;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub relative_path: String,
    pub media_type: String,
    pub size: u64,
}

pub fn import_file(data_dir: &Path, source: &Path) -> Result<Attachment, String> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Choose a file with a valid UTF-8 name.")?
        .to_string();
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (extension, media_type) = match extension.as_str() {
        "pdf" => ("pdf", "application/pdf"),
        "png" => ("png", "image/png"),
        "jpg" | "jpeg" => ("jpg", "image/jpeg"),
        "webp" => ("webp", "image/webp"),
        _ => return Err("Choose a PDF, PNG, JPEG, or WebP file.".into()),
    };
    // Inspect first so selecting a FIFO or device cannot block while opening it.
    if !fs::metadata(source)
        .map_err(|e| format!("Cannot inspect attachment: {e}"))?
        .is_file()
    {
        return Err("Choose a regular file, not a folder or device.".into());
    }
    let mut input = File::open(source).map_err(|e| format!("Cannot open attachment: {e}"))?;
    let metadata = input
        .metadata()
        .map_err(|e| format!("Cannot inspect attachment: {e}"))?;
    if !metadata.is_file() {
        return Err("Choose a regular file, not a folder or device.".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("Attachments must be 25 MiB or smaller.".into());
    }
    let mut bytes = Vec::new();
    (&mut input)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read attachment: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Attachments must be 25 MiB or smaller.".into());
    }
    let valid = match extension {
        "pdf" => bytes.starts_with(b"%PDF-"),
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" => bytes.starts_with(b"\xff\xd8\xff"),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        _ => false,
    };
    if !valid {
        return Err("The file contents do not match its PDF or image extension.".into());
    }

    let root = data_dir.join("attachments");
    fs::create_dir_all(&root).map_err(|e| format!("Cannot create attachment folder: {e}"))?;
    // A managed folder must not redirect copied materials elsewhere through a symlink.
    if fs::symlink_metadata(&root)
        .map_err(|e| format!("Cannot inspect attachment folder: {e}"))?
        .file_type()
        .is_symlink()
    {
        return Err("The attachment folder must not be a symbolic link.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("Cannot protect attachment folder: {e}"))?;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Cannot timestamp attachment: {e}"))?
        .as_nanos();
    let filename = format!(
        "{}-{timestamp}-{}.{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed),
        extension
    );
    let destination = root.join(&filename);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(&destination)
        .map_err(|e| format!("Cannot create attachment copy: {e}"))?;
    let save = output.write_all(&bytes).and_then(|()| output.sync_all());
    if let Err(e) = save {
        let _ = fs::remove_file(&destination);
        return Err(format!("Cannot save attachment copy: {e}"));
    }
    #[cfg(unix)]
    File::open(&root)
        .and_then(|folder| folder.sync_all())
        .map_err(|e| format!("Cannot sync attachment folder: {e}"))?;
    Ok(Attachment {
        name,
        relative_path: format!("attachments/{filename}"),
        media_type: media_type.into(),
        size: bytes.len() as u64,
    })
}

pub fn resolve(data_dir: &Path, attachment: &Attachment) -> Result<PathBuf, String> {
    let relative = Path::new(&attachment.relative_path);
    let mut parts = relative.components();
    if parts.next() != Some(Component::Normal("attachments".as_ref()))
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
        || attachment.relative_path.contains('\\')
    {
        return Err("Invalid managed attachment path.".into());
    }
    let root = data_dir.join("attachments");
    if fs::symlink_metadata(&root)
        .map_err(|e| format!("Attachment folder is unavailable: {e}"))?
        .file_type()
        .is_symlink()
    {
        return Err("The attachment folder must not be a symbolic link.".into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("Attachment folder is unavailable: {e}"))?;
    let path = data_dir
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("Attachment is unavailable: {e}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("Attachment must be a regular file inside its managed folder.".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "task-manager-attachment-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn source(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, data).unwrap();
            path
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn accepts_supported_headers_and_keeps_copies_after_source_removal() {
        let temp = Temp::new();
        for (name, bytes, mime) in [
            ("报告.PDF", b"%PDF-1.7\n".as_slice(), "application/pdf"),
            ("shot.png", b"\x89PNG\r\n\x1a\n".as_slice(), "image/png"),
            ("shot.jpeg", b"\xff\xd8\xff\xe0".as_slice(), "image/jpeg"),
            ("shot.webp", b"RIFF0000WEBP".as_slice(), "image/webp"),
        ] {
            let source = temp.source(name, bytes);
            let attachment = import_file(&temp.0, &source).unwrap();
            fs::remove_file(source).unwrap();
            assert_eq!(attachment.name, name);
            assert_eq!(attachment.media_type, mime);
            assert_eq!(attachment.size, bytes.len() as u64);
            assert_eq!(
                fs::read(resolve(&temp.0, &attachment).unwrap()).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn rejects_unsupported_or_disguised_files_and_folders() {
        let temp = Temp::new();
        for (name, bytes) in [
            ("run.sh", b"%PDF-1.7".as_slice()),
            ("fake.pdf", b"<html>".as_slice()),
            ("fake.png", b"%PDF-1.7".as_slice()),
            ("fake.jpg", b"".as_slice()),
            ("fake.webp", b"RIFF0000FAKE".as_slice()),
        ] {
            assert!(import_file(&temp.0, &temp.source(name, bytes)).is_err());
        }
        let folder = temp.0.join("folder.pdf");
        fs::create_dir(&folder).unwrap();
        assert!(import_file(&temp.0, &folder).is_err());
        assert!(!temp.0.join("attachments").exists());
    }

    #[test]
    fn rejects_oversize_and_accepts_exact_limit() {
        let temp = Temp::new();
        let path = temp.source("large.pdf", b"%PDF-1.7");
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(import_file(&temp.0, &path).is_err());
        file.set_len(MAX_BYTES).unwrap();
        let attachment = import_file(&temp.0, &path).unwrap();
        assert_eq!(attachment.size, MAX_BYTES);
    }

    #[test]
    fn repeated_names_get_distinct_immutable_copies() {
        let temp = Temp::new();
        let source = temp.source("research.pdf", b"%PDF-first");
        let first = import_file(&temp.0, &source).unwrap();
        fs::write(&source, b"%PDF-second").unwrap();
        let second = import_file(&temp.0, &source).unwrap();
        assert_ne!(first.relative_path, second.relative_path);
        assert_eq!(
            fs::read(resolve(&temp.0, &first).unwrap()).unwrap(),
            b"%PDF-first"
        );
        assert_eq!(
            fs::read(resolve(&temp.0, &second).unwrap()).unwrap(),
            b"%PDF-second"
        );
    }

    #[test]
    fn rejects_paths_outside_managed_folder_and_non_files() {
        let temp = Temp::new();
        let source = temp.source("research.pdf", b"%PDF-first");
        let mut attachment = import_file(&temp.0, &source).unwrap();
        for path in [
            "../research.pdf",
            "attachments/../research.pdf",
            "attachments/nested/file.pdf",
            "/tmp/file.pdf",
            "research.pdf",
            "attachments\\file.pdf",
            "attachments",
        ] {
            attachment.relative_path = path.into();
            assert!(resolve(&temp.0, &attachment).is_err(), "{path}");
        }
        fs::create_dir(temp.0.join("attachments/folder.pdf")).unwrap();
        attachment.relative_path = "attachments/folder.pdf".into();
        assert!(resolve(&temp.0, &attachment).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn protects_copies_and_rejects_symlink_escape() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = Temp::new();
        let source = temp.source("research.pdf", b"%PDF-first");
        let mut attachment = import_file(&temp.0, &source).unwrap();
        assert_eq!(
            fs::metadata(resolve(&temp.0, &attachment).unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(temp.0.join("attachments"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        symlink(&source, temp.0.join("attachments/escape.pdf")).unwrap();
        attachment.relative_path = "attachments/escape.pdf".into();
        assert!(resolve(&temp.0, &attachment).is_err());

        let other = Temp::new();
        symlink(temp.0.join("attachments"), other.0.join("attachments")).unwrap();
        assert!(import_file(&other.0, &source).is_err());
        assert!(resolve(&other.0, &attachment).is_err());
    }
}
