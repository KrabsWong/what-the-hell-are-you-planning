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
            "Pushed · verifying live page" =>
                ("GitHub synced · checking page", "Report pushed to GitHub successfully. Checking the public page; no new push is needed."),
            "Pushed · awaiting deployment" =>
                ("GitHub synced · awaiting deployment", if self.message.is_empty() { "Report pushed to GitHub successfully. Waiting for the public page to serve this version." } else { &self.message }),
            "Pushed · site access error" =>
                ("GitHub synced · site access error", if self.message.is_empty() { "Report pushed to GitHub successfully, but the public page could not be verified. Use Check status to retry without pushing again." } else { &self.message }),
            "Online" => ("Online", "Report is live and ready to share."),
            "Failed" | "Interrupted" => (&self.status, if self.message.is_empty() { "Publishing stopped. You can retry from the button above." } else { &self.message }),
            _ => (&self.status, "Preparing and publishing the report. Your research results are saved."),
        }
    }
}
pub enum PublishEvent { Progress(String, Option<String>), Complete(PublishOutcome) }
pub struct PublishOutcome { commit: Option<String>, page: Option<PageCheck>, error: Option<String> }

#[derive(Debug, PartialEq)]
enum PageCheck { Online, Awaiting(String), AccessError(String) }

fn classify_page(curl_code: Option<i32>, output: &[u8], marker: &str) -> PageCheck {
    let synced = "Report pushed to GitHub successfully.";
    if curl_code != Some(0) {
        let reason = match curl_code {
            Some(5) => "Could not resolve the proxy",
            Some(6) => "Could not resolve the site hostname",
            Some(7) => "Could not connect to the site",
            Some(28) => "Page check timed out after 15 seconds",
            Some(35) => "TLS connection failed",
            Some(60) => "Site TLS certificate verification failed",
            Some(63) => "Page exceeds the 10 MB verification limit",
            _ => "Page request failed",
        };
        return PageCheck::AccessError(format!("{synced} {reason} (curl {}). Check the site or network, then use Check status; no new push is needed.", curl_code.map(|code| code.to_string()).unwrap_or_else(|| "terminated".into())));
    }
    let output = String::from_utf8_lossy(output);
    let Some((body, code)) = output.rsplit_once('\n') else {
        return PageCheck::AccessError(format!("{synced} Page check returned no HTTP status. Use Check status to retry."));
    };
    let code = code.trim().parse::<u16>().unwrap_or(0);
    match code {
        200..=299 if body.contains(marker) => PageCheck::Online,
        200..=299 => PageCheck::Awaiting(format!("{synced} HTTP {code}: the page is reachable but does not yet contain this report version. Wait for deployment/cache refresh, then use Check status.")),
        404 | 423 => PageCheck::Awaiting(format!("{synced} HTTP {code}: the report page is not available yet. Check EdgeOne deployment, then use Check status.")),
        _ => {
            let detail = if code == 545 { " EdgeOne reports an edge function execution error." } else { "" };
            PageCheck::AccessError(format!("{synced} Public page returned HTTP {code}.{detail} Check the hosting service, then use Check status; no new push is needed."))
        }
    }
}

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
    else if p.commit.is_some() { PublicationAction::Verify(p.id.clone()) }
    else { PublicationAction::Retry(p.id.clone()) }
}

impl Store {
    pub(crate) fn topic_share_content(&self, topic: u64) -> Option<(String, String)> {
        self.topic_publication_content(topic).ok()
    }

    pub(crate) fn topic_publication_blocker(&self, topic_id: u64) -> Option<String> {
        let Some(topic) = self.topics().iter().find(|topic| topic.id == topic_id) else {
            return Some("Choose an active research topic before publishing.".into());
        };
        if topic.archived { return Some("Restore this topic before publishing.".into()); }
        if topic.automation_id.is_some() { return Some("Automation topics cannot be published from Research.".into()); }
        if let Some(status) = self.topic_planning_status(topic.id) {
            return Some(format!("Finish topic planning before publishing. Current planning status: {status}."));
        }
        let goals: Vec<_> = self.tasks().iter().filter(|task| task.topic_id == Some(topic.id)
            && task.research.as_ref().is_some_and(|research| research.kind == ResearchKind::Goal)
            && !self.task_is_archived(task.id)).collect();
        if goals.is_empty() { return Some("Add at least one research goal before publishing.".into()); }
        for goal in goals {
            if goal.stage != crate::domain::Stage::Done {
                return Some(format!("Complete the research goal \"{}\" before publishing. Current status: {}.",
                    goal.title.replace(['\n', '\r'], " "), goal.stage.label()));
            }
            if let Some(blocker) = self.goal_publication_blocker(goal.id) { return Some(blocker); }
            let reports: Vec<_> = self.research_children(goal.id).into_iter().filter(|id| self.task(*id)
                .and_then(|task| task.research.as_ref())
                .is_some_and(|research| research.kind == ResearchKind::Advise)).collect();
            if reports.is_empty() {
                return Some(format!("The research goal \"{}\" has no final report step.", goal.title.replace(['\n', '\r'], " ")));
            }
            for report in reports {
                if !self.publication_result_available_or_loadable(report) {
                    return Some(format!("The final report \"{}\" does not have a current saved result. Wait for its rerun to finish, or rerun the report after its inputs change.",
                        self.task(report).map(|task| task.title.replace(['\n', '\r'], " ")).unwrap_or_else(|| "Final report".into())));
                }
            }
        }
        None
    }

    fn goal_publication_blocker(&self, goal_id: u64) -> Option<String> {
        let goal_title = self.task(goal_id).map(|task| task.title.replace(['\n', '\r'], " "))
            .unwrap_or_else(|| "Research goal".into());
        if !self.has_research_plan(goal_id) {
            return Some(format!("Generate and approve a research plan for \"{goal_title}\" before publishing."));
        }
        for step in self.research_children(goal_id) {
            let Some(task) = self.task(step) else { continue };
            if task.stage != crate::domain::Stage::Done {
                return Some(format!("Wait for the research step \"{}\" to finish before publishing. Current status: {}.",
                    task.title.replace(['\n', '\r'], " "), task.stage.label()));
            }
            if !self.publication_result_available_or_loadable(step) {
                return Some(format!("The research step \"{}\" does not have a current saved result. Rerun it after its inputs change, then publish.",
                    task.title.replace(['\n', '\r'], " ")));
            }
        }
        None
    }

    fn publication_result_available_or_loadable(&self, task_id: u64) -> bool {
        self.research_result_available_or_pending(task_id)
            || self
                .runs()
                .iter()
                .rev()
                .find(|run| run.task_id == task_id && run.status != "Simulated")
                .is_some_and(|run| run.status == "Succeeded" && run.output.is_empty())
    }

    pub(crate) fn topic_publication_result_run_ids(&self, topic_id: u64) -> Vec<u64> {
        let Some(topic) = self.topics().iter().find(|topic| topic.id == topic_id) else { return vec![] };
        let mut ids = std::collections::BTreeSet::new();
        let goals = self.tasks().iter().filter(|task| task.topic_id == Some(topic.id)
            && task.research.as_ref().is_some_and(|research| research.kind == ResearchKind::Goal)
            && !self.task_is_archived(task.id));
        for goal in goals {
            for step in self.research_children(goal.id) {
                if let Some(run) = self.runs().iter().rev()
                    .find(|run| run.task_id == step && run.status != "Simulated")
                    && run.status == "Succeeded"
                {
                    ids.insert(run.id);
                }
            }
        }
        ids.into_iter().collect()
    }

    pub(crate) fn topic_publication_content(&self, topic_id: u64) -> Result<(String, String), String> {
        if let Some(blocker) = self.topic_publication_blocker(topic_id) { return Err(blocker); }
        let topic = self
            .topics()
            .iter()
            .find(|t| t.id == topic_id)
            .ok_or("Choose an active research topic before publishing.")?;
        let goals: Vec<_> = self.tasks().iter().filter(|t| t.topic_id == Some(topic.id)
            && t.research.as_ref().is_some_and(|r| r.kind == ResearchKind::Goal)
            && !self.task_is_archived(t.id)).collect();
        let mut markdown = String::new();
        for goal in goals {
            let reports: Vec<_> = self.research_children(goal.id).into_iter().filter(|id| self.task(*id)
                .and_then(|t| t.research.as_ref()).is_some_and(|r| r.kind == ResearchKind::Advise)).collect();
            markdown.push_str(&format!("## {}\n\n", goal.title.replace(['\n', '\r'], " ")));
            for report in reports {
                let run = self.research_result(report).ok_or_else(|| format!(
                    "The final report \"{}\" does not have a current saved result.",
                    self.task(report).map(|task| task.title.replace(['\n', '\r'], " ")).unwrap_or_else(|| "Final report".into())
                ))?;
                markdown.push_str(&run.output);
                markdown.push_str("\n\n### Sources\n\n");
                for source in &run.sources {
                    // Titles are text, never raw HTML; the renderer handles URL policy.
                    markdown.push_str(&format!("- {}: <{}> ({})\n", source.title.replace(['\n', '\r'], " "), source.url, source.published_at));
                }
                markdown.push('\n');
            }
        }
        Ok((topic.name.clone(), markdown))
    }
}
fn fingerprint(config: &PublishingConfig, title: &str, markdown: &str) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(&(&config.repository, &config.branch, &config.base_url, title, markdown)).unwrap()))
}
/// A Topic has one permanent public URL. The first successfully pushed
/// publication owns it; earlier builds also created one record per content
/// version, so older duplicates are treated as history rather than as new
/// canonical URLs.
pub(crate) fn current_publication_index(publications: &[Publication], topic: u64) -> Option<usize> {
    let mut first = None;
    for (index, publication) in publications.iter().enumerate() {
        if publication.topic != topic { continue; }
        if publication.commit.is_some() { return Some(index); }
        first.get_or_insert(index);
    }
    first
}
fn claim_publication(
    store: &mut Store,
    topic: u64,
    config: PublishingConfig,
    title: String,
    markdown: String,
    fingerprint: String,
) -> Result<String, String> {
    let Some(index) = current_publication_index(store.publications(), topic) else {
        let id = random_id()?;
        store.push_publication(Publication { id: id.clone(), topic, fingerprint, config, title, markdown,
            status: "Pending".into(), commit: None, message: String::new() });
        return Ok(id);
    };
    let mut publication = store.publications()[index].clone();
    let id = publication.id.clone();
    store.remove_publication(&id);
    // The URL is now permanent, so extra records for the same Topic are no
    // longer separate public versions. Their pages remain in Git history.
    store.retain_publications(|p| p.topic != topic);
    if publication.fingerprint != fingerprint {
        publication.fingerprint = fingerprint;
        publication.config = config;
        publication.title = title;
        publication.markdown = markdown;
        publication.status = "Pending".into();
        publication.commit = None;
        publication.message.clear();
    }
    let id = publication.id.clone();
    store.push_publication(publication);
    Ok(id)
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
        Err(error) => return PublishOutcome { commit: None, page: None, error: Some(error) },
    };
    let _ = tx.send(PublishEvent::Progress("Pushed · verifying live page".into(), Some(commit.clone())));
    verify_page(&publication, commit)
}

fn verify_page(publication: &Publication, commit: String) -> PublishOutcome {
    let marker = format!("<!-- report-version:{} -->", publication.fingerprint);
    // Keep HTTP status separate from the body: an error page must never verify a report.
    for attempt in 0..6 {
        let page = match std::process::Command::new("curl")
            .args(["--silent", "--show-error", "--location", "--max-redirs", "5", "--max-time", "15", "--proto", "=https", "--proto-redir", "=https", "--max-filesize", "10000000", "--write-out", "\n%{http_code}", &publication.url()]).output()
        {
            Ok(output) => classify_page(output.status.code(), &output.stdout, &marker),
            Err(error) => PageCheck::AccessError(format!("Report pushed to GitHub successfully. Could not start the page checker ({:?}). Use Check status to retry; no new push is needed.", error.kind())),
        };
        // Deployment can take time; actual access errors should be visible immediately.
        if !matches!(page, PageCheck::Awaiting(_)) || attempt == 5 {
            return PublishOutcome { commit: Some(commit), page: Some(page), error: None };
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
    unreachable!()
}

impl TaskApp {
    pub(crate) fn recover_publications(&mut self) {
        self.publishing_scan_requested = true;
        let ids: Vec<String> = self
            .store
            .publications()
            .iter()
            .filter(|publication| matches!(publication.status.as_str(), "Publishing" | "Generating HTML" | "Fetching repository" | "Preparing report commit" | "Checking outgoing history" | "Pushing to GitHub" | "Pushed · verifying live page"))
            .map(|publication| publication.id.clone())
            .collect();
        for id in ids {
            let Some(publication) = self.store.publication_mut(&id) else { continue; };
            publication.status = if publication.commit.is_some() { "Pushed · site access error" } else { "Interrupted" }.into();
            publication.message = if publication.commit.is_some() { "Report pushed to GitHub successfully. Page verification was interrupted. Use Check status; no new push is needed." } else { "Publishing was interrupted. Retry safely using the same report URL." }.into();
        }
    }
    pub(crate) fn refresh_publishing_inputs(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let config = &self.store.report_publishing();
        for (input, value) in self.publishing_inputs.iter().zip([config.repository.clone(), config.branch.clone(), config.base_url.clone()]) {
            input.update(cx, |s, cx| s.set_value(value, w, cx));
        }
    }
    fn save_publishing_settings(&mut self, cx: &mut Context<Self>) {
        let config = PublishingConfig {
            repository: self.publishing_inputs[0].read(cx).value().trim().into(),
            branch: self.publishing_inputs[1].read(cx).value().trim().into(),
            base_url: self.publishing_inputs[2].read(cx).value().trim().trim_end_matches('/').into(),
            automatic: self.store.report_publishing().automatic,
        };
        if let Err(error) = config.validate() { self.show_save_error(error, cx); return; }
        if self
            .apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::Publishing { config },
                "Report publishing settings saved",
                cx,
            )
            .is_none()
        {
            return;
        }
        self.publishing_scan_requested = true;
        self.start_pump(cx);
        cx.notify();
    }
    pub(crate) fn request_topic_publication(&mut self, topic: u64, cx: &mut Context<Self>) {
        if self.publishing_job.is_some() { return; }
        let config = self.store.report_publishing().clone();
        if let Err(error) = config.validate() { self.show_action_error(error, cx); return; }
        if let Err(error) = self.prepare_topic_publication_bodies(topic) { self.show_action_error(error, cx); return; }
        let (title, markdown) = match self.store.topic_publication_content(topic) {
            Ok(content) => content,
            Err(error) => { self.show_action_error(error, cx); return; }
        };
        let fingerprint = fingerprint(&config, &title, &markdown);
        if let Some(index) = current_publication_index(&self.store.publications(), topic) {
            let current = self.store.publications()[index].clone();
            if current.fingerprint == fingerprint {
                if current.status == "Online" {
                    self.toast = "Published report is already current".into();
                    self.error.clear();
                    cx.notify();
                } else if current.commit.is_some() {
                    self.start_publication_mode(&current.id, true, cx);
                } else {
                    self.start_publication(&current.id, cx);
                }
                return;
            }
        }
        let id = match claim_publication(&mut self.store, topic, config, title, markdown, fingerprint) {
            Ok(id) => id,
            Err(error) => { self.show_save_error(error, cx); return; }
        };
        self.start_publication(&id, cx);
    }
    fn prepare_topic_publication_bodies(&mut self, topic: u64) -> Result<(), String> {
        let targets = self.store.topic_publication_result_run_ids(topic)
            .into_iter()
            .filter(|run_id| self.store.runs().iter()
                .find(|run| run.id == *run_id)
                .is_some_and(|run| run.output.is_empty()))
            .collect::<Vec<_>>();
        if targets.is_empty() { return Ok(()); }
        let Some((path, root, incarnation)) = self.live_attach.clone() else {
            return Err("Saved final report content is not loaded. Reopen the workspace and try again.".into());
        };
        let database = crate::sqlite_v4::SqliteV4::open(&path, &root, incarnation)
            .map_err(|error| format!("Could not open saved report content: {error}"))?;
        for run_id in targets {
            let Some(body) = database.body(run_id).map_err(|error| format!("Could not load saved research result {run_id}: {error}"))? else {
                return Err(format!("The saved result body for run {run_id} is missing from durable storage. Open that result to verify it, or rerun the step."));
            };
            if !self.store.hydrate_run_body(run_id, body) {
                return Err("The saved final report changed while it was loading. Try publishing again.".into());
            }
            self.run_body_state.insert(run_id, crate::history_reader::BodyLoad::Loaded);
        }
        Ok(())
    }
    fn start_publication(&mut self, id: &str, cx: &mut Context<Self>) {
        self.start_publication_mode(id, false, cx);
    }
    fn start_publication_mode(&mut self, id: &str, verify_only: bool, cx: &mut Context<Self>) {
        if self.publishing_job.is_some() { return; }
        let Some(root) = self.workspace_root.clone() else { self.show_save_error("Choose a persistent workspace before publishing.", cx); return; };
        if self.storage.is_none() { self.show_save_error("Workspace storage is unavailable. Reopen your workspace before publishing.", cx); return; }
        let Some(p) = self.store.publication_mut(id) else { return; };
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
            Err(mpsc::TryRecvError::Disconnected) => Some((id.clone(), PublishEvent::Complete(PublishOutcome { commit: None, page: None, error: Some("Publishing worker stopped.".into()) }))),
            Err(mpsc::TryRecvError::Empty) => None,
        });
        if let Some((id, event)) = event {
            if let Some(p) = self.store.publication_mut(&id) {
                match event {
                    PublishEvent::Progress(stage, commit) => {
                        p.status = stage;
                        if commit.is_some() { p.commit = commit; }
                        p.message = if p.commit.is_some() { "GitHub has received the report. Waiting for EdgeOne and checking the public page.".into() } else { String::new() };
                    }
                    PublishEvent::Complete(outcome) => {
                        self.publishing_job = None;
                        self.publishing_scan_requested = true;
                        p.commit = outcome.commit.or(p.commit.take());
                        let (status, message) = match outcome.page {
                            Some(PageCheck::Online) => ("Online", String::new()),
                            Some(PageCheck::Awaiting(message)) => ("Pushed · awaiting deployment", message),
                            Some(PageCheck::AccessError(message)) => ("Pushed · site access error", message),
                            None if p.commit.is_some() => ("Pushed · site access error", format!("Report pushed to GitHub successfully. {} Use Check status; no new push is needed.", outcome.error.unwrap_or_else(|| "Page verification stopped.".into()))),
                            None => ("Failed", outcome.error.unwrap_or_default()),
                        };
                        p.status = status.into();
                        p.message = message;
                    }
                }
                self.toast = format!("Report publication · {}", p.status);
            }
            self.persist_if_changed(true); cx.notify();
        }
        if !self.publishing_scan_requested || self.publishing_job.is_some() || !self.store.report_publishing().automatic || self.store.report_publishing().repository.is_empty()
            || self.workspace_root.is_none() || self.storage.is_none() || !self.storage_error.is_empty() { return; }
        self.publishing_scan_requested = false;
        let topics: Vec<u64> = self.store.topics().iter().map(|topic| topic.id).collect();
        for topic in topics {
            if self.store.topic_publication_blocker(topic).is_some() { continue; }
            if self.prepare_topic_publication_bodies(topic).is_err() { continue; }
            let Some((title, markdown)) = self.store.topic_share_content(topic) else { continue; };
            let fingerprint = fingerprint(&self.store.report_publishing(), &title, &markdown);
            let current = current_publication_index(&self.store.publications(), topic)
                .map(|index| &self.store.publications()[index]);
            if current.is_none_or(|p| p.fingerprint != fingerprint) {
                self.request_topic_publication(topic, cx);
                return;
            }
        }
    }
    pub(crate) fn publishing_settings_panel(&self, cx: &mut Context<Self>) -> Div {
        col().gap_4()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Report publishing")))
            .child(label("Push finished Topic reports to a dedicated GitHub repository connected to EdgeOne. Configure Git authentication on this computer first. Original attachments and execution logs are not published.", cx))
            .child(label("GitHub repository", cx)).child(Input::new(&self.publishing_inputs[0]).id("publishing-repository"))
            .child(label("Target branch", cx)).child(Input::new(&self.publishing_inputs[1]).id("publishing-branch"))
            .child(label("Public site URL", cx)).child(Input::new(&self.publishing_inputs[2]).id("publishing-url"))
            .child(button("save-publishing", "Save publishing settings").disabled(self.publishing_job.is_some()).on_click(cx.listener(|a, _, _, cx| a.save_publishing_settings(cx))))
            .child(select_button("automatic-publishing", "Publish completed Topics automatically", self.store.report_publishing().automatic, cx)
                .when(self.store.report_publishing().automatic, |b| b.icon(assets::IconName::Check))
                .disabled(self.publishing_job.is_some())
                .on_click(cx.listener(|a, _, _, cx| {
                    if !a.store.report_publishing().automatic {
                        if let Err(error) = a.store.report_publishing().validate() { a.show_save_error(error, cx); return; }
                    }
                    let automatic = !a.store.report_publishing().automatic;
                    if a
                        .apply_domain_command_undoable(
                            crate::scoped_domain::DomainCommand::PublishingAutomatic {
                                automatic,
                            },
                            "Automatic publishing preference saved",
                            cx,
                        )
                        .is_none()
                    {
                        return;
                    }
                    a.publishing_scan_requested = true; a.start_pump(cx); cx.notify();
                })))
            .child(label("Enabling this also publishes existing completed Topics. All active goals must be complete. A Topic keeps one report URL: new content overwrites it in place so shared links keep working, retries keep the same URL, and earlier versions remain in Git history. Anyone with the link can read it.", cx))
            .child(label("Import your existing reports into the repository before connecting EdgeOne. Git pushes never force-overwrite remote history. Keep the repository private and configure EdgeOne to deploy this branch.", cx))
    }
    pub(crate) fn topic_publish_button(&self, topic: u64, cx: &mut Context<Self>) -> impl IntoElement {
        let blocker = self.store.topic_publication_blocker(topic);
        let ready = blocker.is_none();
        let publication = current_publication_index(&self.store.publications(), topic)
            .map(|index| &self.store.publications()[index]);
        let busy = self.publishing_job.as_ref().is_some_and(|(id, _)| publication.is_some_and(|p| &p.id == id));
        let body_pending = self.store.topic_publication_result_run_ids(topic).iter().any(|run_id| {
            self.store.runs().iter().find(|run| run.id == *run_id)
                .is_some_and(|run| run.output.is_empty())
        });
        let changed = !busy && ready && publication.is_some_and(|p| body_pending || self.store.topic_share_content(topic)
            .is_some_and(|(title, markdown)| fingerprint(&self.store.report_publishing(), &title, &markdown) != p.fingerprint));
        let action = publication_action(publication, changed);
        let text = if busy {
            if publication.is_some_and(|p| p.status == "Pushed · verifying live page") { "Checking…" } else { "Publishing…" }
        } else { action.label() };
        let tooltip = blocker.unwrap_or_else(|| action.help().into());
        button("publish-topic-report", text)
            .when(busy, |b| b.icon(assets::IconName::Loader))
            .loading_icon(assets::IconName::Loader)
            .loading(busy)
            .disabled(self.publishing_job.is_some() || (!ready && matches!(action, PublicationAction::Publish(_))))
            .tooltip(tooltip)
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
        let current = current_publication_index(&self.store.publications(), topic)
            .map(|index| self.store.publications()[index].id.clone());
        let mut history: Vec<_> = self.store.publications().iter().filter(|p| p.topic == topic).collect();
        // The Topic's permanent URL comes first; records from the earlier
        // per-version behavior follow as history.
        history.sort_by_key(|p| Some(&p.id) != current.as_ref());
        for p in history.into_iter().take(5) {
            let url = p.url(); let open_url = url.clone();
            let busy = self.publishing_job.as_ref().is_some_and(|(id, _)| id == &p.id);
            let (status, description) = p.presentation();
            let color = if p.status == "Pushed · awaiting deployment" { colors.warning } else if busy { colors.accent } else if p.status == "Online" { colors.success }
                else if matches!(p.status.as_str(), "Failed" | "Interrupted" | "Pushed · site access error") { colors.danger } else { colors.warning };
            panel = panel.child(col().gap_1().px_3().py_2().rounded(px(crate::appearance::GROUP_RADIUS))
                .bg(rgb(colors.side)).border_1().border_color(rgb(colors.subtle_line))
                .child(row().flex_wrap().gap_2()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).px_2().py_1().rounded(px(crate::appearance::CONTROL_RADIUS))
                        .text_color(rgb(color)).bg(rgba((color << 8) | 20)).child(status.to_string()))
                    .child(row().min_w_0().max_w_full()
                    .child(link_button(format!("open-report-{}", p.id), "").icon(assets::IconName::ExternalLink).min_w_0().flex_1()
                        .accessibility_label(format!("Open report: {url}"))
                        .tooltip(url.clone())
                        .child(div().min_w_0().text_sm().underline().overflow_hidden().text_ellipsis().whitespace_nowrap().child(selectable(url.clone())))
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
fn claim_publication_in_vec(
    publications: &mut Vec<Publication>,
    topic: u64,
    config: PublishingConfig,
    title: String,
    markdown: String,
    fingerprint: String,
) -> Result<String, String> {
    let mut store = Store::empty();
    for publication in publications.drain(..) {
        store.push_publication(publication);
    }
    let result = claim_publication(&mut store, topic, config, title, markdown, fingerprint);
    *publications = store.publications().to_vec();
    result
}

#[cfg(test)]
mod tests {
    use super::{PublishingConfig, fingerprint};
    use crate::domain::{Store, Stage};
    #[test]
    fn page_checks_distinguish_deployment_from_access_errors() {
        use super::{classify_page, PageCheck};
        let marker = "<!-- report-version:snapshot -->";
        assert_eq!(classify_page(Some(0), format!("{marker}\n200").as_bytes(), marker), PageCheck::Online);
        assert!(matches!(classify_page(Some(0), b"older report\n200", marker), PageCheck::Awaiting(message) if message.contains("HTTP 200") && message.contains("version")));
        for code in [404, 423] {
            assert!(matches!(classify_page(Some(0), format!("not ready\n{code}").as_bytes(), marker), PageCheck::Awaiting(message) if message.contains(&format!("HTTP {code}"))));
        }
        for code in [403, 500, 545] {
            assert!(matches!(classify_page(Some(0), format!("{marker} private error details\n{code}").as_bytes(), marker), PageCheck::AccessError(message) if message.contains(&format!("HTTP {code}")) && message.contains("GitHub successfully") && !message.contains("private error details")));
        }
        for (code, reason) in [(6, "hostname"), (28, "timed out"), (60, "certificate")] {
            assert!(matches!(classify_page(Some(code), format!("{marker}\n200").as_bytes(), marker), PageCheck::AccessError(message) if message.contains(reason) && message.contains(&format!("curl {code}"))));
        }
    }
    #[test]
    fn deployment_presentation_keeps_push_success_and_specific_error_visible() {
        use super::{Publication, PublicationAction, publication_action};
        let mut p = Publication { id: "a".repeat(32), topic: 1, fingerprint: "snapshot".into(), config: Default::default(),
            title: "Topic".into(), markdown: "Report".into(), status: "Pushed · awaiting deployment".into(), commit: Some("commit".into()), message: String::new() };
        assert!(p.presentation().0.contains("GitHub synced"));
        assert!(p.presentation().1.contains("pushed to GitHub successfully"));
        p.status = "Pushed · site access error".into();
        p.message = "Report pushed to GitHub successfully. HTTP 545: EdgeOne execution error.".into();
        assert!(p.presentation().0.contains("site access error"));
        assert_eq!(p.presentation().1, p.message);
        assert_eq!(publication_action(Some(&p), false), PublicationAction::Verify(p.id.clone()));
        p.status = "Pushed · verifying live page".into(); p.message.clear();
        assert!(p.presentation().0.contains("checking page"));
        assert!(p.presentation().1.contains("no new push"));
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
        assert_eq!(publication_action(Some(&p), false), PublicationAction::Verify(p.id.clone()));
    }
    #[test]
    fn new_content_overwrites_the_topic_url_instead_of_creating_a_new_one() {
        use super::{claim_publication, current_publication_index, Publication};
        let config = PublishingConfig::default();
        let first = Publication { id: "a".repeat(32), topic: 1, fingerprint: "first".into(), config: config.clone(),
            title: "Topic".into(), markdown: "First report".into(), status: "Online".into(), commit: Some("first-commit".into()), message: String::new() };
        let mut stale = first.clone();
        stale.id = "b".repeat(32); stale.fingerprint = "second".into(); stale.markdown = "Second report".into();
        let mut other_topic = first.clone();
        other_topic.id = "c".repeat(32); other_topic.topic = 2;
        let mut publications = vec![first.clone(), stale, other_topic];

        let id = super::claim_publication_in_vec(&mut publications, 1, config.clone(), "Topic".into(), "Third report".into(), "third".into()).unwrap();
        assert_eq!(id, first.id);
        let current = publications.iter().find(|p| p.topic == 1).unwrap();
        assert_eq!(current.id, first.id);
        assert_eq!(current.markdown, "Third report");
        assert_eq!(current.fingerprint, "third");
        assert_eq!(current.status, "Pending");
        assert_eq!(current.commit, None);
        // Duplicate records from the old per-version URL behavior are removed;
        // other Topics keep their own publication.
        assert_eq!(publications.iter().filter(|p| p.topic == 1).count(), 1);
        assert_eq!(publications.iter().filter(|p| p.topic == 2).count(), 1);
        assert_eq!(current_publication_index(&publications, 1), Some(publications.len() - 1));

        // Republishing the identical snapshot keeps the URL and its live state.
        let mut same = current.clone();
        same.status = "Online".into(); same.commit = Some("third-commit".into());
        *publications.last_mut().unwrap() = same;
        let id = super::claim_publication_in_vec(&mut publications, 1, config, "Topic".into(), "Third report".into(), "third".into()).unwrap();
        assert_eq!(id, first.id);
        assert_eq!(publications.last().unwrap().status, "Online");
        assert_eq!(publications.last().unwrap().commit.as_deref(), Some("third-commit"));
    }

    #[test]
    fn an_unpushed_earlier_record_does_not_steal_the_shared_url() {
        use super::{claim_publication, current_publication_index, Publication};
        let config = PublishingConfig::default();
        let failed = Publication { id: "a".repeat(32), topic: 1, fingerprint: "failed".into(), config: config.clone(),
            title: "Topic".into(), markdown: "Never pushed".into(), status: "Failed".into(), commit: None, message: String::new() };
        let mut pushed = failed.clone();
        pushed.id = "b".repeat(32); pushed.status = "Online".into(); pushed.commit = Some("commit".into());
        let mut publications = vec![failed.clone(), pushed.clone()];
        assert_eq!(current_publication_index(&publications, 1), Some(1));
        let id = super::claim_publication_in_vec(&mut publications, 1, config, "Topic".into(), "Updated".into(), "updated".into()).unwrap();
        assert_eq!(id, pushed.id);
        assert_eq!(publications.len(), 1);
        assert_ne!(publications[0].id, failed.id);
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
        store.project_at_mut(0).unwrap().directory = "/tmp".into();
        store.set_default_model("test/model".into());
        let topic = store.create_topic(1, "Research").unwrap();
        let goal = store.create_research_goal_in_topic("Goal".into(), "Scope".into(), 1, Some(topic)).unwrap();
        let ids = store.apply_research_plan(goal, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        for id in &ids {
            let mut run = store.prepare_run(*id).unwrap();
            run.status = "Succeeded".into();
            run.output = if *id == ids[2] { "Public final report" } else { "PRIVATE_WORKING_NOTES" }.into();
            store.push_run(run);
            store.task_mut(*id).unwrap().stage = Stage::Done;
        }
        store.sync_goal_progress(goal);
        let (title, markdown) = store.topic_share_content(topic).unwrap();
        assert!(markdown.contains("Public final report"));
        assert!(!markdown.contains("PRIVATE_WORKING_NOTES"));
        let bodies: Vec<_> = store.runs().iter().map(|run| (run.id, run.output.clone())).collect();
        for (run_id, _) in &bodies {
            store.run_mut(*run_id).unwrap().output.clear();
        }
        for (run_id, _) in &bodies {
            store.mark_lazy_body(*run_id);
        }
        assert!(store.topic_publication_blocker(topic).is_none());
        assert_eq!(store.topic_publication_result_run_ids(topic).len(), bodies.len());
        assert!(store.topic_share_content(topic).is_none());
        for (run_id, body) in bodies {
            assert!(store.hydrate_run_body(run_id, body));
        }
        assert!(store.topic_share_content(topic).is_some());
        store.task_mut(ids[2]).unwrap().stage = Stage::InProgress;
        store.last_run_mut().unwrap().status = "Running".into();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Done);
        let blocker = store.topic_publication_blocker(topic).unwrap();
        assert!(blocker.contains("Wait for the research step"));
        assert!(blocker.contains("Current status: In progress"));
        store.task_mut(ids[2]).unwrap().stage = Stage::Done;
        store.last_run_mut().unwrap().status = "Succeeded".into();
        let config = PublishingConfig::default();
        store.push_publication(super::Publication { id: "a".repeat(32), topic,
            fingerprint: fingerprint(&config, &title, &markdown), config, title, markdown,
            status: "Interrupted".into(), commit: None, message: "Retry".into() });
        let restored: Store = serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        assert_eq!(restored.publications(), store.publications());
        store.task_mut(ids[0]).unwrap().description.push_str(" Changed evidence");
        assert!(store.topic_share_content(topic).is_none());
        assert!(restored.topic_share_content(topic).is_some());
        let mut store = restored;
        store.create_research_goal_in_topic("Unfinished second goal".into(), "Scope".into(), 1, Some(topic)).unwrap();
        assert!(store.topic_share_content(topic).is_none());
    }

}
