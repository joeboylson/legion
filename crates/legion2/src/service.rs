//! Running legion2d as a launchd service on macOS: it starts at login, comes
//! back if it crashes, and writes its output to a log file.

use std::{path::PathBuf, process::Command};

use legion2_proto::NAME;

pub const LOG_FILE_NAME: &str = "legion2d.log";
const LAUNCH_AGENTS_FOLDER: &str = "Library/LaunchAgents";
const LAUNCHCTL: &str = "launchctl";
/// launchd finishes taking an old service down after bootout returns; a
/// bootstrap straight after can fail until it has.
const BOOTSTRAP_TRIES: u32 = 10;
const BOOTSTRAP_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

pub fn service_label() -> String {
    format!("com.{NAME}.{NAME}d")
}

/// What the service runs, and with what.
pub struct ServicePlan {
    pub program: PathBuf,
    pub claude_command: String,
    pub addon_folder: Option<String>,
    /// launchd starts programs with a bare PATH; sessions need the one the
    /// service was installed from to find git, node and claude.
    pub path_variable: String,
    pub log_file: PathBuf,
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn plist_string(text: &str) -> String {
    format!("<string>{}</string>", escape_xml(text))
}

pub fn program_arguments(plan: &ServicePlan) -> Vec<String> {
    let program = plan.program.to_string_lossy().into_owned();
    let claude = ["--claude".to_string(), plan.claude_command.clone()];
    let addon = plan.addon_folder.iter().flat_map(|folder| ["--addon".to_string(), folder.clone()]);
    std::iter::once(program).chain(claude).chain(addon).collect()
}

/// The launch agent. KeepAlive brings legion2d back only when it exits with
/// an error, so stopping it on purpose (SIGTERM, a clean exit) keeps it down.
pub fn launch_agent_plist(plan: &ServicePlan) -> String {
    let arguments = program_arguments(plan).iter().map(|argument| format!("        {}", plist_string(argument))).collect::<Vec<_>>().join("\n");
    let log_file = plist_string(&plan.log_file.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    {label}
    <key>ProgramArguments</key>
    <array>
{arguments}
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        {path}
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    {log_file}
    <key>StandardErrorPath</key>
    {log_file}
</dict>
</plist>
"#,
        label = plist_string(&service_label()),
        path = plist_string(&plan.path_variable),
    )
}

fn run(program: &str, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(program).args(arguments).output().map_err(|error| format!("can't run {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{program} {}: {}", arguments.join(" "), String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// launchd's name for this user's login session.
fn user_domain() -> Result<String, String> {
    Ok(format!("gui/{}", run("id", &["-u"])?))
}

fn plist_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME isn't set")?;
    Ok(PathBuf::from(home).join(LAUNCH_AGENTS_FOLDER).join(format!("{}.plist", service_label())))
}

pub fn install(claude_command: String, addon_folder: Option<String>) -> Result<String, String> {
    let program = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|folder| folder.join(format!("{NAME}d"))))
        .filter(|path| path.exists())
        .ok_or_else(|| format!("can't find {NAME}d next to {NAME}"))?;
    let data_folder = legion2_proto::data_dir().ok_or("HOME isn't set")?;
    std::fs::create_dir_all(&data_folder).map_err(|error| format!("can't create {}: {error}", data_folder.display()))?;
    let plan = ServicePlan {
        program,
        claude_command,
        addon_folder,
        path_variable: std::env::var("PATH").unwrap_or_default(),
        log_file: data_folder.join(LOG_FILE_NAME),
    };
    let path = plist_path()?;
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| format!("can't create {}: {error}", folder.display()))?;
    }
    std::fs::write(&path, launch_agent_plist(&plan)).map_err(|error| format!("can't write {}: {error}", path.display()))?;
    let domain = user_domain()?;
    // Not loaded yet is fine: this only replaces an older install.
    let _ = run(LAUNCHCTL, &["bootout", &format!("{domain}/{}", service_label())]);
    let plist = path.to_string_lossy().into_owned();
    (1..=BOOTSTRAP_TRIES)
        .map(|attempt| {
            let started = run(LAUNCHCTL, &["bootstrap", &domain, &plist]);
            if started.is_err() && attempt < BOOTSTRAP_TRIES {
                std::thread::sleep(BOOTSTRAP_RETRY_DELAY);
            }
            started
        })
        .find(Result::is_ok)
        .unwrap_or_else(|| run(LAUNCHCTL, &["bootstrap", &domain, &plist]))?;
    Ok(format!(
        "installed {}; {NAME}d starts now and at every login, and writes to {}. If one was already running by hand, stop it: launchd keeps trying until it can start.",
        path.display(),
        plan.log_file.display()
    ))
}

pub fn remove() -> Result<String, String> {
    let path = plist_path()?;
    if !path.exists() {
        return Err(format!("no service installed ({} doesn't exist)", path.display()));
    }
    let domain = user_domain()?;
    run(LAUNCHCTL, &["bootout", &format!("{domain}/{}", service_label())])?;
    std::fs::remove_file(&path).map_err(|error| format!("can't remove {}: {error}", path.display()))?;
    Ok(format!("stopped {NAME}d and removed {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn plan(addon_folder: Option<&str>) -> ServicePlan {
        ServicePlan {
            program: PathBuf::from("/bin/legion2d"),
            claude_command: "/bin/claude".into(),
            addon_folder: addon_folder.map(String::from),
            path_variable: "/usr/bin:/bin".into(),
            log_file: Path::new("/data").join(LOG_FILE_NAME),
        }
    }

    #[test]
    fn the_service_runs_legion2d_with_claude_and_any_addon() {
        assert_eq!(program_arguments(&plan(None)), ["/bin/legion2d", "--claude", "/bin/claude"]);
        assert_eq!(program_arguments(&plan(Some("/addon"))), ["/bin/legion2d", "--claude", "/bin/claude", "--addon", "/addon"]);
    }

    #[test]
    fn the_plist_keeps_the_path_logs_and_comes_back_only_after_a_crash() {
        let plist = launch_agent_plist(&plan(None));
        assert!(plist.contains("<string>com.legion2.legion2d</string>"));
        assert!(plist.contains("<key>PATH</key>\n        <string>/usr/bin:/bin</string>"));
        assert!(plist.contains("<key>SuccessfulExit</key>\n        <false/>"));
        assert_eq!(plist.matches("<string>/data/legion2d.log</string>").count(), 2);
    }

    #[test]
    fn paths_are_escaped_for_xml() {
        assert_eq!(plist_string("a&b<c>"), "<string>a&amp;b&lt;c&gt;</string>");
    }
}
