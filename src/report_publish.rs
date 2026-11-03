//! Durable Topic publication snapshots. Publishing never changes research status.
use crate::{app::TaskApp, domain::Store, research::ResearchKind, views::*};
use gpui_kit::{component::{input::Input, Disableable}, prelude::FluentBuilder, *};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::mpsc};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PublishingConfig {
    pub repository: String,
    pub branch: String,
    pub base_url: String,
    pub automatic: bool,
}
impl Default for PublishingConfig {
    fn default() -> Self {
        Self { repository: String::new(), branch: "main".into(), base_url: "https://report.krabs.wang".into(), automatic: false }
    }
}
impl PublishingConfig {
    fn validate(&self) -> Result<(), String> {
        crate::report_git::validate(&self.repository, &self.branch)?;
        let host = self.base_url.strip_prefix("https://").ok_or("The public site URL must use HTTPS.")?;
        if host.is_empty() || !host.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c)) || !host.contains('.') {
            return Err("Enter an HTTPS site origin, without a path, credentials or query.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Publication {
    pub id: String,
    pub topic: u64,
    pub fingerprint: String,
    pub config: PublishingConfig,
    pub title: String,
    pub markdown: String,
    pub status: String,
    pub commit: Option<String>,
    pub message: String,
}
impl Publication {
    pub fn url(&self) -> String { format!("{}/{}/", self.config.base_url, self.id) }
    fn presentation(&self) -> (&str, &str) {
        match self.status.as_str() {
            "Pushed · verifying live page" | "Pushed · awaiting deployment" =>
                ("Awaiting deployment", "Report pushed to GitHub. Waiting for the public page to become available."),
            "Online" => ("Online", "Report is live and ready to share."),
            "Failed" | "Interrupted" => (&self.status, if self.message.is_empty() { "Publishing stopped. You can retry from the button above." } else { &self.message }),
            _ => (&self.status, "Preparing and publishing the report. Your research results are saved."),
        }
    }
}
pub enum PublishEvent { Progress(String, Option<String>), Complete(PublishOutcome) }
pub struct PublishOutcome { commit: Option<String>, online: bool, error: Option<String> }

#[derive(Debug, PartialEq)]
enum PublicationAction { Publish(bool), Retry(String), Verify(String), Open(String) }
impl PublicationAction {
    fn label(&self) -> &'static str {
        match self { Self::Publish(false) => "Publish report", Self::Publish(true) => "Publish update", Self::Retry(_) => "Retry publishing", Self::Verify(_) => "Check status", Self::Open(_) => "View report" }
    }
    fn help(&self) -> &'static str {
        match self { Self::Publish(_) => "Publish the current final reports to GitHub", Self::Retry(_) => "Retry publishing the saved report snapshot", Self::Verify(_) => "Check the public page only; does not commit or push to GitHub", Self::Open(_) => "Open the published report" }
    }
}
fn publication_action(publication: Option<&Publication>, changed: bool) -> PublicationAction {
    let Some(p) = publication else { return PublicationAction::Publish(false); };
    if changed { return PublicationAction::Publish(true); }
    if p.status == "Online" { PublicationAction::Open(p.url()) }
    else if p.commit.is_some() && p.status != "Failed" { PublicationAction::Verify(p.id.clone()) }
    else { PublicationAction::Retry(p.id.clone()) }
}

impl Store {
    pub(crate) fn topic_share_content(&self, topic: u64) -> Option<(String, String)> {
        let topic = self.topics.iter().find(|t| t.id == topic && !t.archived)?;
        if self.topic_planning_status(topic.id).is_some() { return None; }
        let goals: Vec<_> = self.tasks.iter().filter(|t| t.topic_id == Some(topic.id)
            && t.research.as_ref().is_some_and(|r| r.kind == ResearchKind::Goal)
            && !self.task_is_archived(t.id)).collect();
        if goals.is_empty() || goals.iter().any(|g| !self.goal_tasks_complete(g.id)) { return None; }
        let mut markdown = String::new();
        for goal in goals {
            let reports: Vec<_> = self.research_children(goal.id).into_iter().filter(|id| self.task(*id)
                .and_then(|t| t.research.as_ref()).is_some_and(|r| r.kind == ResearchKind::Advise)).collect();
            if reports.is_empty() { return None; }
            markdown.push_str(&format!("## {}\n\n", goal.title.replace(['\n', '\r'], " ")));
            for report in reports {
                let run = self.research_result(report)?;
                markdown.push_str(&run.output);
                markdown.push_str("\n\n### 引用来源\n\n");
                for source in &run.sources {
                    // Titles are text, never raw HTML; the renderer handles URL policy.
                    markdown.push_str(&format!("- {}: <{}> ({})\n", source.title.replace(['\n', '\r'], " "), source.url, source.published_at));
                }
                markdown.push('\n');
            }
        }
        Some((topic.name.clone(), markdown))
    }
}
fn fingerprint(config: &PublishingConfig, title: &str, markdown: &str) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(&(&config.repository, &config.branch, &config.base_url, title, markdown)).unwrap()))
}
fn reuse_publication(publications: &mut Vec<Publication>, topic: u64, fingerprint: &str) -> Option<String> {
    let index = publications.iter().position(|p| p.topic == topic && p.fingerprint == fingerprint)?;
    // The last publication is the current version shown by the Topic header.
    let publication = publications.remove(index);
    let id = publication.id.clone();
    publications.push(publication);
    Some(id)
}
fn random_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).map_err(|_| "Could not generate a secure report ID.")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn publish(root: std::path::PathBuf, publication: Publication, tx: &mpsc::Sender<PublishEvent>) -> PublishOutcome {
    let _ = tx.send(PublishEvent::Progress("Generating HTML".into(), None));
    let marker = format!("<!-- report-version:{} -->", publication.fingerprint);
    let html = format!("{}\n{marker}", crate::report_html::render(&publication.title, &publication.markdown));
    let files = vec![(format!("{}/index.html", publication.id), html.into_bytes())];
    let commit = match crate::report_git::publish(&root, &publication.config.repository, &publication.config.branch, &files, &publication.config.base_url, |stage| { let _ = tx.send(PublishEvent::Progress(stage.into(), None)); }) {
        Ok(commit) => commit,
        Err(error) => return PublishOutcome { commit: None, online: false, error: Some(error) },
    };
    let _ = tx.send(PublishEvent::Progress("Pushed · verifying live page".into(), Some(commit.clone())));
    verify_page(&publication, commit)
}

fn verify_page(publication: &Publication, commit: String) -> PublishOutcome {
    let marker = format!("<!-- report-version:{} -->", publication.fingerprint);
    // Verify this exact snapshot, not a stale cached page or a successful Git push.
    for _ in 0..6 {
        if let Ok(output) = std::process::Command::new("curl")
            .args(["--silent", "--fail", "--max-time", "15", "--proto", "=https", "--max-filesize", "10000000", &publication.url()]).output()
        {
            if output.status.success() && String::from_utf8_lossy(&output.stdout).contains(&marker) {
                return PublishOutcome { commit: Some(commit), online: true, error: None };
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
    PublishOutcome { commit: Some(commit), online: false, error: Some("Pushed to GitHub; the exact page is not live yet. Check EdgeOne deployment, then use Check status.".into()) }
}

impl TaskApp {
    pub(crate) fn recover_publications(&mut self) {
        self.publishing_scan_requested = true;
        for publication in &mut self.store.publications {
            if matches!(publication.status.as_str(), "Publishing" | "Generating HTML" | "Fetching repository" | "Preparing report commit" | "Checking outgoing history" | "Pushing to GitHub" | "Pushed · verifying live page") {
                publication.status = "Interrupted".into();
                publication.message = "Publishing was interrupted. Retry safely using the same report URL.".into();
            }
        }
    }
    pub(crate) fn refresh_publishing_inputs(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let config = &self.store.report_publishing;
        for (input, value) in self.publishing_inputs.iter().zip([config.repository.clone(), config.branch.clone(), config.base_url.clone()]) {
            input.update(cx, |s, cx| s.set_value(value, w, cx));
        }
    }
    fn save_publishing_settings(&mut self, cx: &mut Context<Self>) {
        let config = PublishingConfig {
            repository: self.publishing_inputs[0].read(cx).value().trim().into(),
            branch: self.publishing_inputs[1].read(cx).value().trim().into(),
            base_url: self.publishing_inputs[2].read(cx).value().trim().trim_end_matches('/').into(),
            automatic: self.store.report_publishing.automatic,
        };
        if let Err(error) = config.validate() { self.show_save_error(error, cx); return; }
        self.store.report_publishing = config;
        self.publishing_scan_requested = true;
        self.confirm_save("Report publishing settings saved", cx);
        self.start_pump(cx);
        cx.notify();
    }
    pub(crate) fn request_topic_publication(&mut self, topic: u64, cx: &mut Context<Self>) {
        if self.publishing_job.is_some() { return; }
        let config = self.store.report_publishing.clone();
        if let Err(error) = config.validate() { self.show_save_error(error, cx); return; }
        let Some((title, markdown)) = self.store.topic_share_content(topic) else {
            self.show_save_error("Finish all active goals and planning requests with current reports before publishing this topic.", cx); return;
        };
        let fingerprint = fingerprint(&config, &title, &markdown);
        let id = if let Some(id) = reuse_publication(&mut self.store.publications, topic, &fingerprint) {
            id
        } else {
            let id = match random_id() { Ok(id) => id, Err(error) => { self.show_save_error(error, cx); return; } };
            self.store.publications.push(Publication { id: id.clone(), topic, fingerprint, config, title, markdown,
                status: "Pending".into(), commit: None, message: String::new() });
            id
        };
        self.start_publication(&id, cx);
    }
    fn start_publication(&mut self, id: &str, cx: &mut Context<Self>) {
        self.start_publication_mode(id, false, cx);
    }
    fn start_publication_mode(&mut self, id: &str, verify_only: bool, cx: &mut Context<Self>) {
        if self.publishing_job.is_some() { return; }
        let Some(root) = self.workspace_root.clone() else { self.show_save_error("Choose a persistent workspace before publishing.", cx); return; };
        if self.storage.is_none() { self.show_save_error("Workspace storage is unavailable. Reopen your workspace before publishing.", cx); return; }
        let Some(p) = self.store.publications.iter_mut().find(|p| p.id == id) else { return; };
        p.status = if verify_only { "Pushed · verifying live page" } else { "Generating HTML" }.into(); p.message.clear();
        let publication = p.clone();
        self.persist_if_changed(true);
        if !self.storage_error.is_empty() { self.show_save_error(self.storage_error.clone(), cx); return; }
        let (tx, rx) = mpsc::channel();
        self.publishing_job = Some((id.into(), rx));
        self.toast = if verify_only { "Checking report availability…" } else { "Publishing report · generating HTML" }.into();
        std::thread::spawn(move || {
            let outcome = if verify_only {
                verify_page(&publication, publication.commit.clone().unwrap_or_default())
            } else {
                publish(root.join("workspace/report-publishing"), publication, &tx)
            };
            let _ = tx.send(PublishEvent::Complete(outcome));
        });
        self.start_pump(cx); cx.notify();
    }
    pub(crate) fn poll_report_publishing(&mut self, cx: &mut Context<Self>) {
        let event = self.publishing_job.as_ref().and_then(|(id, rx)| match rx.try_recv() {
            Ok(event) => Some((id.clone(), event)),
            Err(mpsc::TryRecvError::Disconnected) => Some((id.clone(), PublishEvent::Complete(PublishOutcome { commit: None, online: false, error: Some("Publishing worker stopped. Retry this publication.".into()) }))),
            Err(mpsc::TryRecvError::Empty) => None,
        });
        if let Some((id, event)) = event {
            if let Some(p) = self.store.publications.iter_mut().find(|p| p.id == id) {
                match event {
                    PublishEvent::Progress(stage, commit) => {
                        p.status = stage;
                        if commit.is_some() { p.commit = commit; }
                        p.message = if p.commit.is_some() { "GitHub has received the report. Waiting for EdgeOne and checking the public page.".into() } else { String::new() };
                    }
                    PublishEvent::Complete(outcome) => {
                        self.publishing_job = None;
                        self.publishing_scan_requested = true;
                        p.status = if outcome.online { "Online" } else if outcome.commit.is_some() { "Pushed · awaiting deployment" } else { "Failed" }.into();
                        p.commit = outcome.commit.or(p.commit.take());
                        p.message = outcome.error.unwrap_or_default();
                    }
                }
                self.toast = format!("Report publication · {}", p.status);
            }
            self.persist_if_changed(true); cx.notify();
        }
        if !self.publishing_scan_requested || self.publishing_job.is_some() || !self.store.report_publishing.automatic || self.store.report_publishing.repository.is_empty()
            || self.workspace_root.is_none() || self.storage.is_none() || !self.storage_error.is_empty() { return; }
        self.publishing_scan_requested = false;
        let ready = self.store.topics.iter().find_map(|topic| {
            let (title, markdown) = self.store.topic_share_content(topic.id)?;
            let fingerprint = fingerprint(&self.store.report_publishing, &title, &markdown);
            (!self.store.publications.iter().any(|p| p.topic == topic.id && p.fingerprint == fingerprint)).then_some(topic.id)
        });
        if let Some(topic) = ready { self.request_topic_publication(topic, cx); }
    }
    pub(crate) fn publishing_settings_panel(&self, cx: &mut Context<Self>) -> Div {
        col().gap_4()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Report publishing"))
            .child(label("Push finished Topic reports to a dedicated GitHub repository connected to EdgeOne. Configure Git authentication on this computer first. Original attachments and execution logs are not published.", cx))
            .child(label("GitHub repository", cx)).child(Input::new(&self.publishing_inputs[0]).id("publishing-repository"))
            .child(label("Target branch", cx)).child(Input::new(&self.publishing_inputs[1]).id("publishing-branch"))
            .child(label("Public site URL", cx)).child(Input::new(&self.publishing_inputs[2]).id("publishing-url"))
            .child(button("save-publishing", "Save publishing settings").disabled(self.publishing_job.is_some()).on_click(cx.listener(|a, _, _, cx| a.save_publishing_settings(cx))))
            .child(select_button("automatic-publishing", if self.store.report_publishing.automatic { "✓ Publish completed Topics automatically" } else { "Publish completed Topics automatically" }, self.store.report_publishing.automatic)
                .disabled(self.publishing_job.is_some())
                .on_click(cx.listener(|a, _, _, cx| {
                    if !a.store.report_publishing.automatic {
                        if let Err(error) = a.store.report_publishing.validate() { a.show_save_error(error, cx); return; }
                    }
                    a.store.report_publishing.automatic = !a.store.report_publishing.automatic;
                    a.publishing_scan_requested = true;
                    a.confirm_save("Automatic publishing preference saved", cx); a.start_pump(cx); cx.notify();
                })))
            .child(label("Enabling this also publishes existing completed Topics. All active goals must be complete. New report content creates a new random URL; retries keep the same URL. Anyone with the link can read it.", cx))
            .child(label("Import your existing reports into the repository before connecting EdgeOne. Git pushes never force-overwrite remote history. Keep the repository private and configure EdgeOne to deploy this branch.", cx))
    }
    pub(crate) fn topic_publish_button(&self, topic: u64, cx: &mut Context<Self>) -> impl IntoElement {
        // Rendering only checks workflow state; exact input signatures are validated on publish.
        let goals: Vec<_> = self.store.tasks.iter().filter(|t| t.topic_id == Some(topic)
            && t.research.as_ref().is_some_and(|r| r.kind == ResearchKind::Goal)
            && !self.store.task_is_archived(t.id)).collect();
        let ready = !goals.is_empty() && goals.iter().all(|g| g.stage == crate::domain::Stage::Done)
            && self.store.topic_planning_status(topic).is_none()
            && self.store.topics.iter().any(|t| t.id == topic && !t.archived);
        let publication = self.store.publications.iter().rev().find(|p| p.topic == topic);
        let busy = self.publishing_job.as_ref().is_some_and(|(id, _)| publication.is_some_and(|p| &p.id == id));
        let changed = !busy && ready && publication.is_some_and(|p| self.store.topic_share_content(topic)
            .is_some_and(|(title, markdown)| fingerprint(&self.store.report_publishing, &title, &markdown) != p.fingerprint));
        let action = publication_action(publication, changed);
        let text = if busy {
            if publication.is_some_and(|p| p.status == "Pushed · verifying live page") { "Checking…" } else { "Publishing…" }
        } else { action.label() };
        button("publish-topic-report", text)
            .when(busy, |b| b.icon(assets::IconName::Loader))
            .loading_icon(assets::IconName::Loader)
            .loading(busy)
            .disabled(self.publishing_job.is_some() || (!ready && matches!(action, PublicationAction::Publish(_))))
            .tooltip(action.help())
            .on_click(cx.listener(move |a, _, _, cx| match &action {
                PublicationAction::Publish(_) => a.request_topic_publication(topic, cx),
                PublicationAction::Retry(id) => a.start_publication(id, cx),
                PublicationAction::Verify(id) => a.start_publication_mode(id, true, cx),
                PublicationAction::Open(url) => cx.open_url(url),
            }))
    }

    pub(crate) fn topic_publication_panel(&self, topic: u64, cx: &mut Context<Self>) -> Div {
        let colors = crate::appearance::palette(cx);
        let mut panel = col().px_6().gap_2();
        for p in self.store.publications.iter().filter(|p| p.topic == topic).rev().take(5) {
            let url = p.url(); let open_url = url.clone();
            let busy = self.publishing_job.as_ref().is_some_and(|(id, _)| id == &p.id);
            let (status, description) = p.presentation();
            let color = if status == "Awaiting deployment" { colors.warning } else if busy { colors.accent } else if p.status == "Online" { colors.success }
                else if matches!(p.status.as_str(), "Failed" | "Interrupted") { colors.danger } else { colors.warning };
            panel = panel.child(col().gap_1().px_3().py_2().rounded_md()
                .bg(rgb(colors.side)).border_1().border_color(rgb(colors.subtle_line))
                .child(row().flex_wrap().gap_2()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).px_2().py_1().rounded_md()
                        .text_color(rgb(color)).bg(rgba((color << 8) | 20)).child(status.to_string()))
                    .child(row().min_w_0().max_w_full()
                    .child(link_button(format!("open-report-{}", p.id), "").min_w_0().flex_1()
                        .accessibility_label(format!("Open report: {url}"))
                        .tooltip(url.clone())
                        .child(div().min_w_0().text_sm().underline().overflow_hidden().text_ellipsis().whitespace_nowrap().child(format!("{url} ↗")))
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&open_url))))
                    .child(crate::controls::tool_icon(format!("copy-report-{}", p.id), assets::IconName::Copy, "Copy report link")
                        .on_click(cx.listener(move |a, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                            a.toast = "Report link copied".into(); cx.notify();
                        })))))
                .child(div().id(format!("report-description-{}", p.id)).min_h(px(20.)).child(label(description.to_string(), cx)).map(observed)));
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{PublishingConfig, fingerprint};
    use crate::domain::{Store, Stage};
    #[test]
    fn deployment_check_keeps_status_and_description_stable() {
        let mut p = super::Publication { id: "a".repeat(32), topic: 1, fingerprint: "snapshot".into(), config: Default::default(),
            title: "Topic".into(), markdown: "Report".into(), status: "Pushed · awaiting deployment".into(), commit: Some("commit".into()), message: "Previous check timed out".into() };
        let before = (p.presentation().0.to_owned(), p.presentation().1.to_owned());
        p.status = "Pushed · verifying live page".into(); p.message.clear();
        assert_eq!(p.presentation(), (before.0.as_str(), before.1.as_str()));
        p.status = "Pushed · awaiting deployment".into(); p.message = "Still waiting".into();
        assert_eq!(p.presentation(), (before.0.as_str(), before.1.as_str()));
        p.status = "Online".into(); p.message.clear();
        assert!(!p.presentation().1.is_empty());
    }
    #[test]
    fn header_action_matches_publication_state_and_current_content() {
        use super::{Publication, PublicationAction, publication_action};
        let mut p = Publication { id: "a".repeat(32), topic: 1, fingerprint: "snapshot".into(), config: Default::default(),
            title: "Topic".into(), markdown: "Report".into(), status: "Failed".into(), commit: None, message: String::new() };
        assert_eq!(publication_action(None, false), PublicationAction::Publish(false));
        assert_eq!(publication_action(Some(&p), false), PublicationAction::Retry(p.id.clone()));
        p.status = "Pushed · awaiting deployment".into(); p.commit = Some("commit".into());
        assert_eq!(publication_action(Some(&p), false), PublicationAction::Verify(p.id.clone()));
        p.status = "Online".into();
        assert_eq!(publication_action(Some(&p), false), PublicationAction::Open(p.url()));
        assert_eq!(publication_action(Some(&p), true), PublicationAction::Publish(true));
        p.status = "Failed".into();
        assert_eq!(publication_action(Some(&p), false), PublicationAction::Retry(p.id.clone()));
    }
    #[test]
    fn returning_to_an_older_snapshot_makes_it_current_without_changing_its_url() {
        use super::{Publication, PublicationAction, publication_action, reuse_publication};
        let first = Publication { id: "a".repeat(32), topic: 1, fingerprint: "first".into(), config: Default::default(),
            title: "Topic".into(), markdown: "First report".into(), status: "Online".into(), commit: Some("first-commit".into()), message: String::new() };
        let mut second = first.clone();
        second.id = "b".repeat(32); second.fingerprint = "second".into(); second.markdown = "Second report".into();
        let mut other_topic = first.clone();
        other_topic.id = "c".repeat(32); other_topic.topic = 2;
        let mut publications = vec![first.clone(), second.clone(), other_topic.clone()];
        assert_eq!(reuse_publication(&mut publications, 1, "first"), Some(first.id.clone()));
        assert_eq!(publications, vec![second, other_topic, first.clone()]);
        let current = publications.iter().rev().find(|p| p.topic == 1).unwrap();
        assert_eq!(current.url(), first.url());
        assert_eq!(publication_action(Some(current), false), PublicationAction::Open(first.url()));
        assert_eq!(reuse_publication(&mut publications, 1, "first"), Some(first.id));
        assert_eq!(publications.len(), 3);
        let before = publications.clone();
        assert_eq!(reuse_publication(&mut publications, 1, "missing"), None);
        assert_eq!(publications, before);
    }
    #[test]
    fn settings_reject_credentials_and_non_github_remotes() {
        for repository in ["https://token@github.com/a/b", "https://evil.com/a/b", "git@github.com:a/../b", "https://github.com/a/b?token=x"] {
            assert!(PublishingConfig { repository: repository.into(), ..Default::default() }.validate().is_err());
        }
        assert!(PublishingConfig { repository: "git@github.com:owner/reports.git".into(), ..Default::default() }.validate().is_ok());
    }
    #[test]
    fn snapshot_identity_tracks_content_and_destination_not_toggle() {
        let mut config = PublishingConfig::default();
        let original = fingerprint(&config, "Title", "Report");
        config.automatic = true;
        assert_eq!(original, fingerprint(&config, "Title", "Report"));
        config.branch = "publish".into();
        assert_ne!(original, fingerprint(&config, "Title", "Report"));
        assert_ne!(original, fingerprint(&PublishingConfig::default(), "Title", "Updated report"));
    }
    #[test]
    fn empty_or_pending_topics_cannot_publish() {
        let mut store = Store::empty();
        let topic = store.create_topic(1, "Topic").unwrap();
        assert!(store.topic_share_content(topic).is_none());
        store.create_research_goal_in_topic("Goal".into(), "Brief".into(), 1, Some(topic)).unwrap();
        assert!(store.topic_share_content(topic).is_none());
    }
    #[test]
    fn only_current_complete_topic_reports_are_shared_and_snapshots_persist() {
        let mut store = Store::empty();
        store.projects[0].directory = "/tmp".into();
        store.default_model = "test/model".into();
        let topic = store.create_topic(1, "Research").unwrap();
        let goal = store.create_research_goal_in_topic("Goal".into(), "Scope".into(), 1, Some(topic)).unwrap();
        let ids = store.apply_research_plan(goal, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        for id in &ids {
            let mut run = store.prepare_run(*id).unwrap();
            run.status = "Succeeded".into();
            run.output = if *id == ids[2] { "Public final report" } else { "PRIVATE_WORKING_NOTES" }.into();
            store.runs.push(run);
            store.task_mut(*id).unwrap().stage = Stage::Done;
        }
        store.sync_goal_progress(goal);
        let (title, markdown) = store.topic_share_content(topic).unwrap();
        assert!(markdown.contains("Public final report"));
        assert!(!markdown.contains("PRIVATE_WORKING_NOTES"));
        let config = PublishingConfig::default();
        store.publications.push(super::Publication { id: "a".repeat(32), topic,
            fingerprint: fingerprint(&config, &title, &markdown), config, title, markdown,
            status: "Interrupted".into(), commit: None, message: "Retry".into() });
        let restored: Store = serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        assert_eq!(restored.publications, store.publications);
        store.task_mut(ids[0]).unwrap().description.push_str(" Changed evidence");
        assert!(store.topic_share_content(topic).is_none());
        assert!(restored.topic_share_content(topic).is_some());
        let mut store = restored;
        store.create_research_goal_in_topic("Unfinished second goal".into(), "Scope".into(), 1, Some(topic)).unwrap();
        assert!(store.topic_share_content(topic).is_none());
    }

}
