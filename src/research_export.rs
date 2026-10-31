//! Export a completed goal's validated results without exposing runtime logs or credentials.
use crate::{app::TaskApp, domain::Store, research::ResearchKind};
use gpui_kit::Context;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

pub struct ResearchExport {
    pub name: String,
    pub files: Vec<(String, Vec<u8>)>,
    resources: Vec<(String, crate::attachments::Attachment)>,
}

fn filename(text: &str) -> String {
    let mut name = String::new();
    for c in text.chars() {
        let c = if c.is_alphanumeric() || c == '-' {
            c
        } else {
            '_'
        };
        if name.len() + c.len_utf8() > 80 {
            break;
        }
        if c != '_' || !name.ends_with('_') {
            name.push(c);
        }
    }
    let name = name.trim_matches('_');
    if name.is_empty() {
        "research".into()
    } else {
        name.into()
    }
}

fn markdown_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(['\r', '\n'], " ")
}

impl Store {
    pub fn research_export(&self, goal: u64) -> Result<ResearchExport, String> {
        let task = self
            .task(goal)
            .filter(|task| {
                task.research
                    .as_ref()
                    .is_some_and(|r| r.kind == ResearchKind::Goal)
            })
            .ok_or("Select a research goal to export.")?;
        if !self.goal_tasks_complete(goal) {
            return Err(
                "Finish all research steps with current results before exporting this goal.".into(),
            );
        }
        let identifier = self.identifier(task);
        let mut files = vec![];
        let mut index = format!(
            "# {}\n\nGoal: {}\n\nExported: {}\n\n{}\n\n## Documents\n\n",
            task.title,
            identifier,
            crate::opencode::timestamp(),
            task.description
        );
        let mut materials = task.attachments.clone();
        for id in self.research_children(goal) {
            let step = self.task(id).ok_or("A research step is missing.")?;
            let result = self
                .research_result(id)
                .ok_or("A research result is no longer current.")?;
            let category = match step.research.as_ref().map(|r| r.kind) {
                Some(ResearchKind::Collect) => "evidence",
                Some(ResearchKind::Analyze) => "analysis",
                Some(ResearchKind::Advise) => "report",
                _ => continue,
            };
            let step_id = self.identifier(step);
            let stem = format!(
                "{category}/{}_{id}_{}_{}",
                category,
                filename(&step_id),
                filename(&step.title)
            );
            let document = format!("{stem}.md");
            index.push_str(&format!(
                "- [{step_id} · {category}]({document}) — {}\n",
                step.title.replace('\n', " ")
            ));
            let mut markdown = result.output.clone();
            markdown.push_str("\n\n---\n\n## References / 引用来源\n\n");
            if result.sources.is_empty() {
                markdown.push_str(
                    "No separately recorded web sources. See citations in the document above.\n",
                );
            }
            for source in &result.sources {
                markdown.push_str(&format!(
                    "- {}\n  - URL: <{}>\n  - Published: {}\n",
                    markdown_text(&source.title),
                    source.url,
                    markdown_text(&source.published_at)
                ));
            }
            files.push((document.clone(), markdown.into_bytes()));
            index.push_str(&format!(
                "  - Run: {} · Started: {} · Finished: {}\n",
                result.id,
                result.started_at,
                result.ended_at.as_deref().unwrap_or("unknown")
            ));
            materials.extend(step.attachments.clone());
            materials.extend(result.attachments.clone());
        }
        materials.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        materials.dedup_by(|a, b| a.relative_path == b.relative_path);
        let mut resources = vec![];
        if !materials.is_empty() {
            index.push_str("\n## 引用资源 / Uploaded resources\n\n");
        }
        for (i, attachment) in materials.into_iter().enumerate() {
            let extension = match attachment.media_type.as_str() {
                "application/pdf" => "pdf",
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => {
                    return Err(format!(
                        "Unsupported uploaded resource: {}",
                        attachment.name
                    ));
                }
            };
            let stem = Path::new(&attachment.name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("resource");
            let path = format!("引用资源/{:03}_{}.{}", i + 1, filename(stem), extension);
            index.push_str(&format!(
                "- [{}]({path})\n",
                markdown_text(&attachment.name)
            ));
            resources.push((path, attachment));
        }
        index.push_str("\nEach document contains the complete saved Markdown result and its recorded references. Uploaded original files are included in 引用资源 when present. Failed attempts, technical logs and other working-directory files are excluded.\n");
        files.push(("README.md".into(), index.into_bytes()));
        Ok(ResearchExport {
            name: format!("{}_{}.zip", filename(&identifier), filename(&task.title)),
            files,
            resources,
        })
    }
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl ResearchExport {
    pub fn write_zip(&self, destination: &Path, data_dir: Option<&Path>) -> Result<(), String> {
        let work = std::env::temp_dir().join(format!(
            "research-export-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&work)
            .map_err(|e| format!("Could not prepare export: {e}"))?;
        let staging = Staging(work);
        let content = staging.0.join("documents");
        fs::create_dir(&content).map_err(|e| e.to_string())?;
        for (name, bytes) in &self.files {
            let path = content.join(name);
            fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(path, bytes).map_err(|e| format!("Could not write export document: {e}"))?;
        }
        for (name, attachment) in &self.resources {
            let root = data_dir.ok_or("The attachment storage location is unavailable.")?;
            let source = crate::attachments::resolve(root, attachment).map_err(|error| {
                format!(
                    "Cannot export uploaded resource '{}': {error}",
                    attachment.name
                )
            })?;
            let metadata = fs::metadata(&source).map_err(|e| e.to_string())?;
            if !metadata.is_file()
                || metadata.len() != attachment.size
                || metadata.len() > crate::attachments::MAX_BYTES
            {
                return Err(format!(
                    "Uploaded resource '{}' has changed or is unreadable. Restore the original file before exporting.",
                    attachment.name
                ));
            }
            let target = content.join(name);
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            let copied = fs::copy(source, target)
                .map_err(|e| format!("Could not copy '{}': {e}", attachment.name))?;
            if copied != attachment.size {
                return Err(format!(
                    "Uploaded resource '{}' changed during export.",
                    attachment.name
                ));
            }
        }
        let archive = staging.0.join("research.zip");
        let result = Command::new("/usr/bin/zip")
            .current_dir(&content)
            .args(["-q", "-r"])
            .arg(&archive)
            .arg(".")
            .output()
            .map_err(|e| format!("Could not create ZIP: {e}"))?;
        if !result.status.success() {
            return Err("Could not create the research ZIP archive.".into());
        }
        // Copy to a sibling first; an incomplete write must never replace an existing export.
        let parent = destination.parent().ok_or("Choose a destination folder.")?;
        let temporary = parent.join(format!(
            ".research-export-{}-{}.tmp",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let result = (|| -> std::io::Result<()> {
            let mut target = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            std::io::copy(&mut fs::File::open(archive)?, &mut target)?;
            target.flush()?;
            target.sync_all()?;
            fs::rename(&temporary, destination)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|e| format!("Could not save the ZIP file: {e}"))
    }
}

impl TaskApp {
    pub fn export_research_goal(&mut self, goal: u64, cx: &mut Context<Self>) {
        let export = match self.store.research_export(goal) {
            Ok(export) => export,
            Err(error) => {
                self.error = error;
                cx.notify();
                return;
            }
        };
        let directory = self
            .workspace_root
            .clone()
            .unwrap_or_else(std::env::temp_dir);
        let data_dir = self.data_dir().ok();
        let path = cx.prompt_for_new_path(&directory, Some(&export.name));
        cx.spawn(async move |this, cx| {
            let destination = match path.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => return,
                _ => {
                    let _ = this.update(cx, |a, cx| {
                        a.error = "Could not open the export save dialog.".into();
                        cx.notify();
                    });
                    return;
                }
            };
            let saved_path = destination.clone();
            let _ = this.update(cx, |a, cx| {
                a.toast = "Exporting research ZIP…".into();
                cx.notify();
            });
            let result = cx
                .background_executor()
                .spawn(async move { export.write_zip(&destination, data_dir.as_deref()) })
                .await;
            let _ = this.update(cx, |a, cx| {
                match result {
                    Ok(()) => {
                        a.error.clear();
                        a.toast = format!("Research exported to {}", saved_path.display());
                        cx.reveal_path(&saved_path);
                    }
                    Err(error) => {
                        a.error = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::Stage, research::Source};

    fn completed_goal() -> (Store, u64, Vec<u64>) {
        let mut store = Store::empty();
        store.projects[0].directory = "/tmp".into();
        store.default_model = "test/model".into();
        let goal = store
            .create_research_goal("完整研究/中文".into(), "Scope".into(), 1)
            .unwrap();
        let ids = store.apply_research_plan(goal, r#"{"tasks":[{"title":"../证据:公告","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"分析/同名","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"报告","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        for id in &ids {
            let mut run = store.prepare_run(*id).unwrap();
            run.status = "Succeeded".into();
            run.output = format!(
                "# 完整文档 {id}\n\n{}\nEND_OF_REPORT\n",
                "Evidence 数据\n".repeat(1000)
            );
            run.sources = vec![Source {
                title: "Source".into(),
                url: "https://example.com".into(),
                published_at: "unknown".into(),
            }];
            store.runs.push(run);
            store.task_mut(*id).unwrap().stage = Stage::Done;
        }
        store.sync_goal_progress(goal);
        (store, goal, ids)
    }

    #[test]
    fn zip_contains_complete_markdown_documents_and_inline_sources() {
        let (store, goal, ids) = completed_goal();
        let export = store.research_export(goal).unwrap();
        assert!(export.name.ends_with(".zip"));
        assert!(!export.name.contains('/'));
        assert_eq!(export.files.len(), 4);
        assert!(export.files.iter().all(|(name, _)| name.ends_with(".md")));
        for (category, id) in ["evidence", "analysis", "report"].into_iter().zip(ids) {
            let document = export
                .files
                .iter()
                .find(|(name, _)| {
                    name.starts_with(&format!("{category}/")) && name.ends_with(".md")
                })
                .unwrap();
            let markdown = String::from_utf8(document.1.clone()).unwrap();
            assert!(markdown.starts_with(&store.research_result(id).unwrap().output));
            assert!(markdown.contains("https://example.com"));
            assert!(markdown.contains("## References"));
            assert!(!document.0.contains(".."));
        }
        let root = std::env::temp_dir().join(format!(
            "research-export-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let staging = Staging(root);
        let zip = staging.0.join("研究.zip");
        export.write_zip(&zip, None).unwrap();
        assert!(
            Command::new("/usr/bin/unzip")
                .arg("-tq")
                .arg(&zip)
                .output()
                .unwrap()
                .status
                .success()
        );
        for (name, bytes) in &export.files {
            let extracted = Command::new("/usr/bin/unzip")
                .arg("-p")
                .arg(&zip)
                .arg(name)
                .output()
                .unwrap();
            assert!(extracted.status.success(), "{name}");
            assert_eq!(&extracted.stdout, bytes);
        }
        // Failed save must preserve an existing destination and leave no partial files.
        let directory = staging.0.join("existing");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("keep"), "untouched").unwrap();
        assert!(export.write_zip(&directory, None).is_err());
        assert_eq!(
            fs::read_to_string(directory.join("keep")).unwrap(),
            "untouched"
        );
        assert_eq!(fs::read_dir(&staging.0).unwrap().count(), 2);
    }

    #[test]
    fn uploaded_resources_are_deduplicated_and_copied_without_name_collisions() {
        let (mut store, goal, ids) = completed_goal();
        let root = std::env::temp_dir().join(format!(
            "research-resources-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let staging = Staging(root);
        let source = staging.0.join("财报.pdf");
        let first_bytes = b"%PDF-1.4 first original resource";
        let second_bytes = b"%PDF-1.4 second original resource";
        fs::write(&source, first_bytes).unwrap();
        let first = crate::attachments::import_file(&staging.0, &source).unwrap();
        fs::write(&source, second_bytes).unwrap();
        let second = crate::attachments::import_file(&staging.0, &source).unwrap();
        // One uploaded file is inherited by multiple steps; another has the same name.
        store
            .task_mut(goal)
            .unwrap()
            .attachments
            .push(first.clone());
        store
            .task_mut(ids[0])
            .unwrap()
            .attachments
            .push(second.clone());
        for id in &ids {
            let signature = store.research_signature(*id).unwrap();
            let run = store
                .runs
                .iter_mut()
                .find(|run| run.task_id == *id)
                .unwrap();
            run.input_signature = signature;
            run.attachments = vec![first.clone(), second.clone()];
        }
        let export = store.research_export(goal).unwrap();
        assert_eq!(export.resources.len(), 2);
        assert_ne!(export.resources[0].0, export.resources[1].0);
        let readme = String::from_utf8(
            export
                .files
                .iter()
                .find(|(name, _)| name == "README.md")
                .unwrap()
                .1
                .clone(),
        )
        .unwrap();
        let zip = staging.0.join("研究.zip");
        export.write_zip(&zip, Some(&staging.0)).unwrap();
        for (name, attachment) in &export.resources {
            assert!(name.starts_with("引用资源/"));
            assert!(name.ends_with(".pdf"));
            assert!(readme.contains(name));
            let extracted = Command::new("/usr/bin/unzip")
                .arg("-p")
                .arg(&zip)
                .arg(name)
                .output()
                .unwrap();
            assert!(extracted.status.success());
            let original =
                fs::read(crate::attachments::resolve(&staging.0, attachment).unwrap()).unwrap();
            assert_eq!(extracted.stdout, original);
        }
        let listing = Command::new("/usr/bin/unzip")
            .arg("-Z1")
            .arg(&zip)
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&listing.stdout).contains(".json"));
        // Never silently export a package with a missing uploaded resource.
        let saved = fs::read(&zip).unwrap();
        fs::remove_file(crate::attachments::resolve(&staging.0, &first).unwrap()).unwrap();
        assert!(
            export
                .write_zip(&zip, Some(&staging.0))
                .unwrap_err()
                .contains("财报.pdf")
        );
        assert_eq!(fs::read(&zip).unwrap(), saved);
    }

    #[test]
    fn incomplete_failed_or_stale_results_cannot_be_exported() {
        let (mut store, goal, ids) = completed_goal();
        store
            .task_mut(ids[0])
            .unwrap()
            .description
            .push_str(" changed");
        assert!(store.research_export(goal).is_err());
        let (mut store, goal, _) = completed_goal();
        store.runs.last_mut().unwrap().status = "Failed".into();
        assert!(store.research_export(goal).is_err());
        assert!(store.research_export(999).is_err());
    }

    #[test]
    fn filenames_are_bounded_and_cannot_escape_archive_directories() {
        for value in [
            "../../x",
            "/absolute/path",
            "",
            ".",
            "\\windows\\path",
            &"长标题".repeat(100),
        ] {
            let name = filename(value);
            assert!(!name.is_empty());
            assert!(name.len() <= 80);
            assert!(!name.contains(['/', '\\', '.']));
        }
    }
}
