//! An operator whose model runs on a harness template starts that
//! template's command, filled from the launch, with Legion's settings.

mod common;

use common::{fake_harness, ThrowawayLegion, OPERATOR};

const MODELS: &str = r#"{
    "fake-model": { "harness": "fake", "address": "http://fake:1/v1", "model": "fake-1" },
    "ghost-model": { "harness": "ghost", "model": "ghost-1" }
}"#;

fn fake_template() -> String {
    let template = serde_json::json!({
        "name": "fake",
        "command": fake_harness(),
        "args": ["--model", "{modelProvider}/{model}", "--port", "{port}", ["--mode", "{permissionMode}"], "--prompt", "{firstPrompt}"],
        "env": { "FAKE_CONFIG": "{configFile}", "FAKE_TOOLS": "{toolsConfigFile}" },
        "tools": "mcp",
        "toolName": "legion_{tool}",
        "configFile": {
            "address": "{address}",
            "system": "{file:{systemPromptFile}}",
            "folder": "{workingFolder}",
            "legion": { "command": ["{legionCommand}", "mcp"], "environment": { "LEGION_DEPLOYMENT": "{deployment}", "LEGION_POSITION": "{position}", "LEGION_SOCKET": "{socket}" } }
        }
    });
    serde_json::to_string_pretty(&template).unwrap()
}

fn add_models(legion: &ThrowawayLegion) {
    let path = legion.setup_file("legion.json");
    let mut settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    settings["models"] = serde_json::from_str(MODELS).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();
    std::fs::create_dir_all(legion.setup_file("harnesses")).unwrap();
    std::fs::write(legion.setup_file("harnesses/fake.json"), fake_template()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_template_session_starts_its_filled_command() {
    let mut legion = ThrowawayLegion::start("template").await;
    legion.set_up_folder().await;
    add_models(&legion);
    let deployment = legion.start_deployment_with_a_mission().await;

    legion.set_operator_model("ghost-model").await;
    assert_eq!(legion.start_operator(&deployment).await.unwrap_err(), "no harness ghost in .legion/harnesses/");

    legion.set_operator_model("fake-model").await;
    legion.start_operator(&deployment).await.unwrap();
    let launch = legion.normalized(&legion.record_of(OPERATOR), &deployment);
    let lines: Vec<&str> = launch.lines().collect();
    let has = |line: &str| lines.contains(&line);

    assert!(has("cwd: <home>/.local/share/legion/folders/repo-<id>/worktrees/0001-tiny"), "{launch}");
    let arguments: Vec<&str> = lines.iter().filter_map(|line| line.strip_prefix("arg: ")).collect();
    assert_eq!(arguments[..3], ["--model", "fake-model/fake-1", "--port"], "{launch}");
    assert!(arguments[3].parse::<u16>().is_ok_and(|port| port > 0), "{launch}");
    assert_eq!(arguments[4..], ["--prompt", "Start on mission 1."], "no permission mode leaves --mode out: {launch}");

    [
        "env: LEGION_DEPLOYMENT=<deployment>",
        "env: LEGION_POSITION=echoer",
        "env: LEGION_SOCKET=<home>/.local/share/legion/legiond.sock",
        "env: HINDSIGHT_NO_CAPTURE=1",
        "env: TERM=xterm-256color",
        "env: PATH=<bin>:/usr/bin:/bin",
        "env: FAKE_CONFIG=<home>/.local/share/legion/prompts/<deployment>-echoer.fake.json",
        "env: FAKE_TOOLS=<home>/.local/share/legion/prompts/<deployment>-echoer.tools.json",
        r#"  "address": "http://fake:1/v1","#,
        r#"  "folder": "<home>/.local/share/legion/folders/repo-<id>/worktrees/0001-tiny","#,
        r#"  "system": "{file:<home>/.local/share/legion/prompts/<deployment>-echoer.md}""#,
        r#"      "<bin>/legion","#,
    ]
    .iter()
    .for_each(|line| assert!(has(line), "missing {line:?} in:\n{launch}"));
    assert!(!launch.contains("LEGION_ADDON") && !launch.contains("LEGION_PERMISSION_MODE") && !launch.contains("CLAUDE_AUTOCOMPACT"), "{launch}");
    assert!(!launch.contains("--plugin-dir"), "{launch}");

    let tools = std::fs::read_to_string(legion.data_file(&format!("prompts/{deployment}-echoer.tools.json"))).unwrap();
    assert!(tools.contains("\"mcpServers\""), "{tools}");
    let prompt = std::fs::read_to_string(legion.data_file(&format!("prompts/{deployment}-echoer.md"))).unwrap();
    assert!(prompt.contains("are in your tool list as legion_<name>.") && !prompt.contains("mcp__legion__"), "the instructions use the template's tool names");

    let sessions = legion.sessions(&deployment).await;
    let operator = sessions.iter().find(|session| session.position == OPERATOR).unwrap();
    assert!(!operator.can_see_state && !operator.is_stuck_starting);
    assert_eq!(operator.model.as_deref(), Some("fake-model"));
    let commander = sessions.iter().find(|session| session.position == "commander").unwrap();
    assert!(commander.can_see_state, "the commander stays on Claude Code");
}
