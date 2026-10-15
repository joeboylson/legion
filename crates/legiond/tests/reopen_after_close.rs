//! Closing a deployment and reopening it at once ends with one commander.

mod common;

use common::{wait_for, ThrowawayLegion};
use legion_proto::{Command, Reply};

#[tokio::test(flavor = "multi_thread")]
async fn reopen_right_after_close_leaves_one_commander() {
    let mut legion = ThrowawayLegion::start("reopen").await;
    legion.set_up_folder().await;
    std::fs::write(legion.out.join("slow-exit"), "").unwrap();
    let deployment = legion.start_deployment_with_a_mission().await;
    wait_for(|| std::fs::metadata(legion.out.join("commander.txt")).ok());

    for _ in 0..2 {
        assert!(matches!(legion.send(Command::DeploymentClose { deployment: deployment.clone() }).await, Reply::Done));
        let reply = legion.send(Command::DeploymentReopen { deployment: deployment.clone() }).await;
        assert!(matches!(reply, Reply::Deployment { .. }));
        let sessions = legion.sessions(&deployment).await;
        let commanders = sessions.iter().filter(|session| session.position == "commander").count();
        assert_eq!(commanders, 1, "sessions: {}", sessions.len());
    }
}
