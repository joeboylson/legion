//! A folder with no `models` and no harnesses starts Claude Code exactly as
//! it did before harness templates: same arguments, same environment.

mod common;

use common::{ThrowawayLegion, OPERATOR};

const GOLDEN: &str = include_str!("testdata/claude_operator_launch.txt");

#[tokio::test(flavor = "multi_thread")]
async fn claude_starts_as_before() {
    let mut legion = ThrowawayLegion::start("claude").await;
    legion.set_up_folder().await;
    let deployment = legion.start_operator_on_a_mission().await;
    let launch = legion.normalized(&legion.record_of(OPERATOR), &deployment);
    if launch != GOLDEN.trim_end() {
        let path = std::env::temp_dir().join("claude_operator_launch.txt");
        std::fs::write(&path, &launch).unwrap();
        panic!("Claude's launch changed; this run's is in {}", path.display());
    }
}
