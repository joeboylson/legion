//! A copy given a batch carries on with the rest of it when one mission in
//! the batch finishes, instead of its session ending with the work stranded.

mod common;

use common::{ThrowawayLegion, OPERATOR};
use legion_proto::{Activity, Command, EntryKind, LogFilter, NewEntry, Reply};

const QUEUED_MISSION: u32 = 2;

/// An operator on mission 1, with mission 2 handed off to it as well. In a
/// folder without git, so finishing removes no worktree.
async fn operator_with_a_queued_mission(name: &str) -> (ThrowawayLegion, String) {
    let mut legion = ThrowawayLegion::start(name).await;
    std::fs::remove_dir_all(legion.repo.join(".git")).unwrap();
    legion.set_up_folder().await;
    let deployment = legion.start_operator_on_a_mission().await;
    legion.send(Command::MissionAdd { deployment: deployment.clone(), title: "second".into(), body: "Say echo again.".into() }).await;
    let handoff = NewEntry { kind: EntryKind::Handoff, mission: Some(QUEUED_MISSION), to: None, text: format!("→ {OPERATOR}: reviewed, merge it"), answers: None };
    legion.send(Command::Post { deployment: deployment.clone(), entry: handoff }).await;
    (legion, deployment)
}

#[tokio::test]
async fn finishing_one_mission_moves_its_copy_onto_the_queued_one() {
    let (mut legion, deployment) = operator_with_a_queued_mission("carry-on").await;
    legion.send(Command::MissionMarkDone { deployment: deployment.clone(), mission: 1, why: "merged".into() }).await;
    let operator = legion.sessions(&deployment).await.into_iter().find(|session| session.position == OPERATOR).unwrap();
    assert_eq!(operator.mission, Some(QUEUED_MISSION));
    assert_ne!(operator.activity, Activity::Ended);
    let filter = LogFilter { mission: Some(QUEUED_MISSION), kinds: Some(vec![EntryKind::Message]), ..Default::default() };
    let Reply::Entries { entries } = legion.send(Command::Log { deployment, filter }).await else { panic!("no log") };
    let told = entries.into_iter().find(|entry| entry.to.as_deref() == Some(OPERATOR)).unwrap();
    assert!(told.text.starts_with("Mission 1 is finished. Carry on with mission 2"));
    assert!(told.text.ends_with("reviewed, merge it"));
}
