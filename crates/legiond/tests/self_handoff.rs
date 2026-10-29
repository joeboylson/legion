//! A handoff from an operator to itself starts no one, so it's refused.

mod common;

use common::{ThrowawayLegion, OPERATOR};
use legion_proto::{Command, EntryKind, NewEntry};

#[tokio::test]
async fn an_operator_handing_a_mission_to_itself_is_refused() {
    let mut legion = ThrowawayLegion::start("self-handoff").await;
    legion.set_up_folder().await;
    let deployment = legion.start_operator_on_a_mission().await;
    let mut operator = legion.client_as(&deployment, OPERATOR).await;
    let handoff = NewEntry { kind: EntryKind::Handoff, mission: Some(1), to: None, text: format!("→ {OPERATOR}: again"), answers: None };
    let refused = operator.ask(Command::Post { deployment, entry: handoff }).await.unwrap_err();
    assert!(refused.contains("hands the mission to yourself"), "{refused}");
}
