//! Pure permanent-deletion regressions; all records are synthetic.
use crate::{
    archive_app::ArchiveTarget,
    domain::{PlanningOwner, Stage, Store},
    report_publish::{Publication, PublishingConfig},
};

const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Find evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare evidence","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report findings","kind":"advise","depends_on":[1]}]}"#;

fn fixture() -> (Store, u64, u64, Vec<u64>) {
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = "/tmp".into();
    store.set_default_model("fixture/model".into());
    let topic = store.create_topic(1, "Archived topic").unwrap();
    let goal = store.create_research_goal_in_topic("Goal".into(), "Scope".into(), 1, Some(topic)).unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    (store, topic, goal, steps)
}

fn completed_run(store: &mut Store, task: u64) -> u64 {
    let mut run = store.prepare_run(task).unwrap();
    let id = run.id;
    run.status = "Succeeded".into();
    run.output = "Synthetic saved evidence".into();
    store.push_run(run);
    store.set_stage(task, Stage::Done).unwrap();
    id
}

#[test]
fn archived_topic_deletion_cascades_lazy_results_and_all_planning_attempts() {
    let (mut store, topic, goal, steps) = fixture();
    let request = store.create_research_request(1, Some(topic), "Synthetic request").unwrap();
    store.request_mut(request).unwrap().goal_ids = vec![goal];
    let result = completed_run(&mut store, steps[0]);
    let mut planning_ids = Vec::new();
    for _ in 0..2 {
        let mut planning = store.runs().iter().find(|run| run.id == result).unwrap().clone();
        planning.id = store.allocate_run_id().unwrap();
        planning.task_id = 0;
        planning.research_kind = None;
        planning.planning_owner = Some(PlanningOwner { request_id: request, revision: 1, automation_id: None });
        planning_ids.push(planning.id);
        store.push_run(planning);
    }
    store.request_mut(request).unwrap().planning_run_id = planning_ids.last().copied();
    store.run_mut(result).unwrap().output.clear();
    store.mark_lazy_body(result);
    let child = store.create_task("Manual child", 1, Stage::Planned, Some(goal)).unwrap();
    let nested = store.create_task("Nested child", 1, Stage::Planned, Some(child)).unwrap();
    let copy = store.create_topic(1, "Surviving copy").unwrap();
    store.topic_mut(copy).unwrap().replanned_from = Some(topic);
    store.push_publication(Publication {
        id: "fixture-publication".into(), topic, fingerprint: "fixture".into(),
        config: PublishingConfig::default(), title: "Old report".into(), markdown: "Deleted result".into(),
        status: "Online".into(), commit: None, message: String::new(),
    });
    store.set_topic_archived(topic, true).unwrap();
    let target = ArchiveTarget::Topic(topic);
    let reviewed = store.archive_deletion(target).unwrap();
    assert!(reviewed.tasks.contains(&nested));
    assert!(planning_ids.iter().all(|id| reviewed.runs.contains(id)));
    store.delete_archived(target, &reviewed).unwrap();
    assert!(store.topic(topic).is_none());
    assert!(store.tasks().is_empty());
    assert!(store.runs().is_empty());
    assert!(store.research_requests().is_empty());
    assert!(store.publications().is_empty());
    assert!(!store.run_body_is_lazy(result));
    assert_eq!(store.topic(copy).unwrap().replanned_from, None);
    assert!(store.project(1).is_some());
}

#[test]
fn archived_goal_deletion_prunes_shared_request_and_preserves_unrelated_research() {
    let (mut store, topic, goal, steps) = fixture();
    let other = store.create_research_goal_in_topic("Other goal".into(), "Other scope".into(), 1, Some(topic)).unwrap();
    let other_steps = store.apply_research_plan(other, PLAN).unwrap();
    let other_run = completed_run(&mut store, other_steps[0]);
    completed_run(&mut store, steps[0]);
    store.task_mut(steps[0]).unwrap().parent_id = None;
    let request = store.create_research_request(1, Some(topic), "Shared request").unwrap();
    store.request_mut(request).unwrap().goal_ids = vec![goal, other];
    let retained_task = store.task(other).unwrap().clone();
    let retained_run = store.runs().iter().find(|run| run.id == other_run).unwrap().clone();
    store.archive(goal).unwrap();
    let target = ArchiveTarget::Task(goal);
    let reviewed = store.archive_deletion(target).unwrap();
    assert!(!reviewed.requests.contains(&request));
    store.delete_archived(target, &reviewed).unwrap();
    assert!(store.task(goal).is_none());
    assert!(steps.iter().all(|id| store.task(*id).is_none()));
    assert_eq!(store.task(other), Some(&retained_task));
    assert_eq!(store.runs(), &[retained_run]);
    assert_eq!(store.research_requests()[0].goal_ids, vec![other]);
    assert!(store.topic(topic).is_some());
}

#[test]
fn deleting_last_goal_removes_emptied_archived_topic_and_its_planning_history() {
    let (mut store, topic, goal, steps) = fixture();
    let request = store.create_research_request(1, Some(topic), "Synthetic request").unwrap();
    store.request_mut(request).unwrap().goal_ids = vec![goal];
    let result = completed_run(&mut store, steps[0]);
    let copy = store.create_topic(1, "Surviving copy").unwrap();
    store.topic_mut(copy).unwrap().replanned_from = Some(topic);
    store.set_topic_archived(topic, true).unwrap();
    let target = ArchiveTarget::Task(goal);
    let reviewed = store.archive_deletion(target).unwrap();
    assert!(reviewed.empty_topics.contains(&topic));
    assert!(reviewed.requests.contains(&request));
    assert!(reviewed.runs.contains(&result));
    store.delete_archived(target, &reviewed).unwrap();
    assert!(store.topic(topic).is_none());
    assert!(store.topics().iter().any(|topic| topic.id == copy));
    assert_eq!(store.topic(copy).unwrap().replanned_from, None);
    assert!(store.tasks().is_empty());
    assert!(store.research_requests().is_empty());
    assert!(store.runs().is_empty());
}

#[test]
fn deleting_one_goal_keeps_archived_topic_with_surviving_goals() {
    let (mut store, topic, goal, _) = fixture();
    let other = store.create_research_goal_in_topic("Other goal".into(), "Other scope".into(), 1, Some(topic)).unwrap();
    store.set_topic_archived(topic, true).unwrap();
    let target = ArchiveTarget::Task(goal);
    let reviewed = store.archive_deletion(target).unwrap();
    assert!(reviewed.empty_topics.is_empty());
    store.delete_archived(target, &reviewed).unwrap();
    assert!(store.task(goal).is_none());
    assert!(store.task(other).is_some());
    assert!(store.topic(topic).is_some());
}

#[test]
fn deleting_last_archived_goal_never_removes_an_active_topic() {
    let (mut store, topic, goal, _) = fixture();
    store.archive(goal).unwrap();
    let target = ArchiveTarget::Task(goal);
    let reviewed = store.archive_deletion(target).unwrap();
    assert!(reviewed.empty_topics.is_empty());
    store.delete_archived(target, &reviewed).unwrap();
    assert!(store.tasks().is_empty());
    assert!(store.topic(topic).is_some());
}

#[test]
fn archive_deletion_refuses_nonarchived_targets_and_individual_research_steps() {
    let (mut store, topic, goal, steps) = fixture();
    assert!(store.archive_deletion(ArchiveTarget::Topic(topic)).is_err());
    assert!(store.archive_deletion(ArchiveTarget::Task(goal)).is_err());
    store.archive(goal).unwrap();
    assert!(store.archive_deletion(ArchiveTarget::Task(steps[0])).is_err());
}

#[test]
fn archive_deletion_refuses_active_and_recoverable_runs_without_mutation() {
    let (mut store, _, goal, steps) = fixture();
    let run = completed_run(&mut store, steps[0]);
    store.archive(goal).unwrap();
    for status in ["Queued", "Running", "Cancelling", "Recovering", "Interrupted"] {
        store.run_mut(run).unwrap().status = status.into();
        assert!(store.archive_deletion(ArchiveTarget::Task(goal)).is_err(), "{status}");
        assert!(store.task(goal).is_some());
        assert_eq!(store.runs().len(), 1);
    }
}

#[test]
fn archive_deletion_refuses_automation_owned_targets_and_descendants() {
    let (mut store, topic, goal, steps) = fixture();
    store.set_topic_archived(topic, true).unwrap();
    store.task_mut(goal).unwrap().automation_id = Some(99);
    assert!(store.archive_deletion(ArchiveTarget::Task(goal)).is_err());
    store.task_mut(goal).unwrap().automation_id = None;
    store.task_mut(steps[0]).unwrap().automation_id = Some(99);
    assert!(store.archive_deletion(ArchiveTarget::Task(goal)).is_err());
    assert!(store.archive_deletion(ArchiveTarget::Topic(topic)).is_err());
    store.task_mut(steps[0]).unwrap().automation_id = None;
    store.topic_mut(topic).unwrap().automation_id = Some(99);
    assert!(store.archive_deletion(ArchiveTarget::Topic(topic)).is_err());
}

#[test]
fn archive_deletion_refuses_surviving_research_dependencies() {
    let (mut store, topic, goal, steps) = fixture();
    let other = store.create_research_goal("Other goal".into(), "Scope".into(), 1).unwrap();
    let other_steps = store.apply_research_plan(other, PLAN).unwrap();
    store.task_mut(other_steps[0]).unwrap().research.as_mut().unwrap().depends_on.push(steps[0]);
    store.set_topic_archived(topic, true).unwrap();
    assert!(store.archive_deletion(ArchiveTarget::Task(goal)).is_err());
    assert!(store.archive_deletion(ArchiveTarget::Topic(topic)).is_err());
    assert!(store.task(goal).is_some());
    assert!(store.task(other).is_some());
}

#[test]
fn archive_deletion_rechecks_reviewed_scope_and_archive_status() {
    let (mut store, _, goal, _) = fixture();
    store.archive(goal).unwrap();
    let target = ArchiveTarget::Task(goal);
    let reviewed = store.archive_deletion(target).unwrap();
    let child = store.create_root_task_without_topic("Added after review", 1, Stage::Planned).unwrap();
    store.task_mut(child).unwrap().parent_id = Some(goal);
    assert!(store.delete_archived(target, &reviewed).is_err());
    assert!(store.task(goal).is_some());
    assert!(store.task(child).is_some());
    let reviewed = store.archive_deletion(target).unwrap();
    store.restore(goal).unwrap();
    assert!(store.delete_archived(target, &reviewed).is_err());
    assert!(store.task(goal).is_some());
}
