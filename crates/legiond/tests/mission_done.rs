//! A mission lands onto main three ways: its operator handing off to done,
//! the commander's mark_done tool, and the admin's mark-done command.

mod common;

use std::process::Command as Process;

use common::ThrowawayLegion;
use legion_proto::{
    tools::command_for_tool_call, Command, EntryKind, LogFilter, MissionStatus, Reply, COMMANDER,
};
use serde_json::json;

const LANDED_FILE: &str = "landed.txt";

fn git(folder: &std::path::Path, arguments: &[&str]) -> String {
    let output = Process::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@test"])
        .args(arguments)
        .current_dir(folder)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A deployment whose mission 1 has an operator's commit on its branch.
async fn mission_with_a_commit(name: &str) -> (ThrowawayLegion, String) {
    let mut legion = ThrowawayLegion::start(name).await;
    legion.set_up_folder().await;
    let deployment = legion.start_operator_on_a_mission().await;
    let worktrees = git(&legion.repo, &["worktree", "list", "--porcelain"]);
    let worktree = worktrees.lines().filter_map(|line| line.strip_prefix("worktree ")).find(|path| path.contains("worktrees")).unwrap().to_string();
    std::fs::write(std::path::Path::new(&worktree).join(LANDED_FILE), "work").unwrap();
    git(std::path::Path::new(&worktree), &["add", LANDED_FILE]);
    git(std::path::Path::new(&worktree), &["commit", "-q", "-m", "feat: work"]);
    (legion, deployment)
}

async fn status_of(legion: &mut ThrowawayLegion, deployment: &str) -> MissionStatus {
    let Reply::Mission { mission, .. } = legion.send(Command::MissionRead { deployment: deployment.into(), mission: 1 }).await else { panic!("no mission") };
    mission.status
}

fn has_landed(legion: &ThrowawayLegion) -> bool {
    legion.repo.join(LANDED_FILE).exists()
}

async fn entries(legion: &mut ThrowawayLegion, deployment: &str) -> Vec<legion_proto::Entry> {
    let filter = LogFilter { mission: Some(1), ..Default::default() };
    let Reply::Entries { entries } = legion.send(Command::Log { deployment: deployment.into(), filter }).await else { panic!("no log") };
    entries
}

#[tokio::test]
async fn a_handoff_to_done_marks_it_done_and_the_finish_lands_it() {
    let (mut legion, deployment) = mission_with_a_commit("handoff-done").await;
    let arguments = json!({ "mission": 1, "next": "done", "note": "all checks passed" });
    legion.send(command_for_tool_call("handoff", &arguments, &deployment, OPERATOR_POSITION).unwrap()).await;
    assert_eq!(status_of(&mut legion, &deployment).await, MissionStatus::Done);
    let done = entries(&mut legion, &deployment).await.into_iter().find(|entry| entry.kind == EntryKind::Done).unwrap();
    assert_eq!(done.text, "all checks passed");
    assert!(!has_landed(&legion));
    legion.send(Command::MissionFinish { deployment, mission: 1 }).await;
    assert!(has_landed(&legion));
}

#[tokio::test]
async fn the_commanders_mark_done_tool_lands_the_mission() {
    let (mut legion, deployment) = mission_with_a_commit("tool-done").await;
    let arguments = json!({ "mission": 1, "why": "the checker handed off to done" });
    legion.send(command_for_tool_call("mark_done", &arguments, &deployment, COMMANDER).unwrap()).await;
    assert_eq!(status_of(&mut legion, &deployment).await, MissionStatus::Done);
    assert!(has_landed(&legion));
}

#[tokio::test]
async fn the_admins_mark_done_lands_the_mission_and_logs_who_and_why() {
    let (mut legion, deployment) = mission_with_a_commit("admin-done").await;
    legion.send(Command::MissionMarkDone { deployment: deployment.clone(), mission: 1, why: "operator forgot".into() }).await;
    assert!(has_landed(&legion));
    let marked = entries(&mut legion, &deployment).await.into_iter().find(|entry| entry.kind == EntryKind::Done).unwrap();
    assert_eq!((marked.from.as_str(), marked.text.as_str()), (legion_proto::HUMAN, "marked done by admin: operator forgot"));
}

#[tokio::test]
async fn marking_a_done_mission_done_again_is_refused() {
    let (mut legion, deployment) = mission_with_a_commit("twice-done").await;
    legion.send(Command::MissionMarkDone { deployment: deployment.clone(), mission: 1, why: "first".into() }).await;
    let again = legion.client.ask(Command::MissionMarkDone { deployment, mission: 1, why: "second".into() }).await;
    assert!(again.unwrap_err().contains("already done"));
}

const OPERATOR_POSITION: &str = common::OPERATOR;
