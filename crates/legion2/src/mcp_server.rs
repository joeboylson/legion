//! `legion2 mcp`: Legion's tools inside Claude, as an MCP server on stdin
//! and stdout. Every session legion2d starts runs one; each tool call
//! becomes a legion2d command in that session's run.

use legion2_proto::{
    tools::{command_for_tool_call, tool_listing, tools_for_position},
    ENV_POSITION, ENV_RUN, NAME,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    client::Client,
    reply_format::reply_text,
};

const JSON_RPC_VERSION: &str = "2.0";
/// Used when the client doesn't say which protocol version it speaks.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
const METHOD_NOT_FOUND: i64 = -32601;
const PARSE_ERROR: i64 = -32700;

/// What to do with one message from Claude.
#[derive(Debug)]
pub enum Handling {
    Reply(Value),
    /// A tool call to send to legion2d; the reply goes back under `id`.
    CallTool { id: Value, command: legion2_proto::Command },
    /// Notifications need no answer.
    Nothing,
}

fn result_message(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": JSON_RPC_VERSION, "id": id, "result": result })
}

fn error_message(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": JSON_RPC_VERSION, "id": id, "error": { "code": code, "message": message } })
}

/// A tool's outcome as Claude reads it: text, flagged when it's an error.
pub fn tool_result(id: &Value, text: &str, is_error: bool) -> Value {
    result_message(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

pub fn handle_message(message: &Value, run: &str, position: &str) -> Handling {
    let Some(id) = message.get("id").cloned() else { return Handling::Nothing };
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => {
            let protocol_version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(DEFAULT_PROTOCOL_VERSION);
            let server_info = json!({ "name": NAME, "version": env!("CARGO_PKG_VERSION") });
            Handling::Reply(result_message(&id, json!({ "protocolVersion": protocol_version, "capabilities": { "tools": {} }, "serverInfo": server_info })))
        }
        "ping" => Handling::Reply(result_message(&id, json!({}))),
        "tools/list" => {
            let tools: Vec<Value> = tools_for_position(position).into_iter().map(tool_listing).collect();
            Handling::Reply(result_message(&id, json!({ "tools": tools })))
        }
        "tools/call" => {
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match command_for_tool_call(tool_name, &arguments, run, position) {
                Ok(command) => Handling::CallTool { id, command },
                Err(problem) => Handling::Reply(tool_result(&id, &problem, true)),
            }
        }
        unknown => Handling::Reply(error_message(&id, METHOD_NOT_FOUND, &format!("no method {unknown}"))),
    }
}

async fn write_message(stdout: &mut tokio::io::Stdout, message: &Value) -> Result<(), String> {
    let line = message.to_string() + "\n";
    stdout.write_all(line.as_bytes()).await.map_err(|error| error.to_string())?;
    stdout.flush().await.map_err(|error| error.to_string())
}

/// Serves until Claude closes stdin.
pub async fn serve_tools() -> Result<(), String> {
    let run = std::env::var(ENV_RUN).map_err(|_| format!("{NAME} mcp runs inside a session legion2d started ({ENV_RUN} isn't set)"))?;
    let position = std::env::var(ENV_POSITION).map_err(|_| format!("{ENV_POSITION} isn't set"))?;
    let mut client = Client::connect().await?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
        let handling = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle_message(&message, &run, &position),
            Err(error) => Handling::Reply(error_message(&Value::Null, PARSE_ERROR, &error.to_string())),
        };
        let response = match handling {
            Handling::Nothing => continue,
            Handling::Reply(response) => response,
            Handling::CallTool { id, command } => match client.ask(command).await {
                Ok(reply) => tool_result(&id, &reply_text(&reply), false),
                Err(problem) => tool_result(&id, &problem, true),
            },
        };
        write_message(&mut stdout, &response).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    fn reply(handling: Handling) -> Value {
        match handling {
            Handling::Reply(response) => response,
            other => panic!("expected a reply, got {other:?}"),
        }
    }

    #[test]
    fn initialize_echoes_the_clients_protocol_version() {
        let response = reply(handle_message(&request("initialize", json!({ "protocolVersion": "2025-03-26" })), "r", "builder"));
        assert_eq!(response["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(response["result"]["serverInfo"]["name"], NAME);
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notifications_get_no_answer() {
        let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(matches!(handle_message(&notification, "r", "builder"), Handling::Nothing));
    }

    #[test]
    fn each_role_lists_its_own_tools() {
        let operator_list = reply(handle_message(&request("tools/list", json!({})), "r", "builder"));
        let commander_list = reply(handle_message(&request("tools/list", json!({})), "r", "commander"));
        let count = |list: &Value| list["result"]["tools"].as_array().unwrap().len();
        assert!(count(&operator_list) < count(&commander_list));
    }

    #[test]
    fn a_good_call_goes_to_legion() {
        let call = handle_message(&request("tools/call", json!({ "name": "missions", "arguments": {} })), "r", "builder");
        assert!(matches!(call, Handling::CallTool { command: legion2_proto::Command::MissionList { .. }, .. }));
    }

    #[test]
    fn a_bad_call_comes_back_as_a_tool_error() {
        let response = reply(handle_message(&request("tools/call", json!({ "name": "start", "arguments": {} })), "r", "builder"));
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn unknown_methods_are_errors() {
        let response = reply(handle_message(&request("resources/list", json!({})), "r", "builder"));
        assert_eq!(response["error"]["code"], METHOD_NOT_FOUND);
    }
}
