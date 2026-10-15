//! What a session runs: Claude Code with Legion's add-on, or the command
//! filled in from its model's harness template.

use std::{net::TcpListener, path::Path};

use legion_proto::NAME;

use crate::{
    harness::{fill, uses_blank, Blanks, FilledCommand, HarnessTemplate},
    opencode_server::{new_server, OpenCodeServer},
    session_arguments::{session_arguments, LaunchPlan},
};

pub enum SessionProgram {
    /// Claude Code's arguments; legiond adds its add-on.
    Claude(Vec<String>),
    /// With the server Legion reads its state from, when the template runs one.
    Template(FilledCommand, Option<OpenCodeServer>),
}

impl SessionProgram {
    /// A template session has no add-on to say what it's doing.
    pub fn reports_itself(&self) -> bool {
        matches!(self, SessionProgram::Template(..))
    }

    pub fn server(&self) -> Option<&OpenCodeServer> {
        match self {
            SessionProgram::Template(_, server) => server.as_ref(),
            SessionProgram::Claude(_) => None,
        }
    }
}

/// Where a session runs and reaches Legion.
pub struct LaunchPlace<'a> {
    pub deployment_id: &'a str,
    pub tools_config: &'a str,
    pub prompt_file: &'a Path,
    /// Where a template session's tools config goes, for `{toolsConfigFile}`.
    pub tools_config_file: &'a Path,
    /// Where its template's `configFile` goes, if it has one.
    pub config_file: &'a Path,
    pub working_folder: &'a Path,
    pub binary_folder: &'a Path,
    pub socket_path: &'a Path,
}

pub fn write_session_file(path: &Path, text: &str) -> Result<(), String> {
    let folder = path.parent().ok_or_else(|| format!("{} has no folder", path.display()))?;
    std::fs::create_dir_all(folder).map_err(|error| format!("can't make {}: {error}", folder.display()))?;
    std::fs::write(path, text).map_err(|error| format!("can't write {}: {error}", path.display()))
}

/// The port is free when asked; the harness binds it soon after.
fn free_local_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("can't find a free port: {error}"))
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn template_blanks(launch: &LaunchPlan, place: &LaunchPlace, config_file: Option<&Path>, server: &OpenCodeServer) -> Blanks {
    let choice = &launch.model_choice;
    Blanks {
        first_prompt: Some(launch.first_prompt.clone()),
        system_prompt_file: Some(text(place.prompt_file)),
        model: choice.model.clone(),
        model_provider: choice.name.clone(),
        address: choice.address.clone(),
        tools_config: Some(place.tools_config.to_string()),
        tools_config_file: Some(text(place.tools_config_file)),
        config_file: config_file.map(text),
        permission_mode: launch.permission_mode.map(|mode| mode.flag_value().to_string()),
        legion_command: Some(text(&place.binary_folder.join(NAME))),
        deployment: Some(place.deployment_id.to_string()),
        position: Some(launch.position.clone()),
        socket: Some(text(place.socket_path)),
        working_folder: Some(text(place.working_folder)),
        port: Some(server.port.to_string()),
        password: Some(server.password.clone()),
        session: Some(server.tag.clone()),
    }
}

/// Fills the template and writes the session's tools config file and its
/// own config file, if it has one.
fn template_program(template: &HarnessTemplate, launch: &LaunchPlan, place: &LaunchPlace) -> Result<SessionProgram, String> {
    let config_file = template.config_file.as_ref().map(|_| place.config_file);
    let server = new_server(free_local_port()?)?;
    let blanks = template_blanks(launch, place, config_file, &server);
    let filled = fill(template, &blanks)?;
    write_session_file(place.tools_config_file, place.tools_config)?;
    if let (Some(path), Some(config)) = (config_file, &filled.config_file) {
        write_session_file(path, config)?;
    }
    // A template with a port and a password runs a server Legion can ask.
    let has_server = uses_blank(template, "port") && uses_blank(template, "password");
    Ok(SessionProgram::Template(filled, has_server.then_some(server)))
}

pub fn session_program(launch: &LaunchPlan, place: &LaunchPlace) -> Result<SessionProgram, String> {
    match &launch.template {
        None => Ok(SessionProgram::Claude(session_arguments(launch, place.tools_config, place.prompt_file))),
        Some(template) => template_program(template, launch, place),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use super::*;
    use crate::{model_choice::model_choice, setup::ModelEntry};

    const PI: &str = r#"{
  "name": "pi",
  "command": "pi",
  "args": ["--model", "{modelProvider}/{model}", ["--mode", "{permissionMode}"], "--port", "{port}", "--", "{firstPrompt}"],
  "env": { "PI_CONFIG": "{configFile}", "PI_TOOLS": "{toolsConfigFile}" },
  "tools": "mcp",
  "configFile": { "base": "{address}", "legion": ["{legionCommand}", "{deployment}", "{position}", "{socket}", "{workingFolder}", "{systemPromptFile}"] }
}"#;

    fn scratch_folder() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("{NAME}-programs-{}", crate::ids::new_id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn spark_plan(template: Option<HarnessTemplate>) -> LaunchPlan {
        let models = BTreeMap::from([("spark-fast".to_string(), ModelEntry { harness: "pi".into(), address: Some("http://spark:8002/v1".into()), model: "qwen".into() })]);
        LaunchPlan {
            position: "builder".into(),
            system_prompt: "be good".into(),
            model_choice: model_choice(Some("spark-fast"), &models),
            template,
            permission_mode: None,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            first_prompt: "Start on mission 1.".into(),
            clear_at: None,
        }
    }

    #[test]
    fn a_template_is_filled_from_the_launch_and_its_files_written() {
        let folder = scratch_folder();
        let prompt_file = folder.join("d1-builder.md");
        let config_file = folder.join("d1-builder.pi.json");
        let tools_file = folder.join("d1-builder.tools.json");
        let place = LaunchPlace {
            deployment_id: "d1",
            tools_config: "{\"mcpServers\":{}}",
            prompt_file: &prompt_file,
            tools_config_file: &tools_file,
            config_file: &config_file,
            working_folder: Path::new("/work"),
            binary_folder: Path::new("/bin"),
            socket_path: Path::new("/s.sock"),
        };
        let launch = spark_plan(Some(serde_json::from_str(PI).unwrap()));
        let SessionProgram::Template(filled, server) = session_program(&launch, &place).unwrap() else { panic!("not a template") };
        let port_at = filled.args.iter().position(|argument| argument == "--port").unwrap() + 1;
        assert!(filled.args[port_at].parse::<u16>().unwrap() > 0);
        assert_eq!(server, None, "a template with no password runs no server Legion can ask");
        assert_eq!(filled.args[..2], ["--model", "spark-fast/qwen"], "no permission mode leaves its group out");
        assert_eq!(filled.args[port_at + 1..], ["--", "Start on mission 1."]);
        assert_eq!(filled.env["PI_CONFIG"], text(&config_file));
        assert_eq!(filled.env["PI_TOOLS"], text(&tools_file));
        assert_eq!(std::fs::read_to_string(&tools_file).unwrap(), place.tools_config);
        let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_file).unwrap()).unwrap();
        assert_eq!(config["base"], "http://spark:8002/v1");
        assert_eq!(config["legion"], serde_json::json!(["/bin/legion", "d1", "builder", "/s.sock", "/work", text(&prompt_file)]));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn no_template_runs_claude_code() {
        let folder = scratch_folder();
        let prompt_file = folder.join("d1-builder.md");
        let tools_file = folder.join("d1-builder.tools.json");
        let place = LaunchPlace { deployment_id: "d1", tools_config: "{}", prompt_file: &prompt_file, tools_config_file: &tools_file, config_file: &folder, working_folder: &folder, binary_folder: &folder, socket_path: &folder };
        let program = session_program(&spark_plan(None), &place).unwrap();
        assert!(!program.reports_itself());
        assert!(matches!(program, SessionProgram::Claude(arguments) if arguments.contains(&"--mcp-config".to_string())));
        assert!(!tools_file.exists(), "Claude takes its tools config on the command line");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
