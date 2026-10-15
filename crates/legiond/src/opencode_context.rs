//! How full an OpenCode session's conversation is, read from its server: the
//! tokens its newest step used, over the context size the server plans around
//! for that model. The session's own `tokens` add up every step, so they say
//! how much was spent, not how much is in the conversation now.

use serde_json::Value;

use crate::{
    constants::{OPENCODE_LAST_STEP_QUERY, OPENCODE_MODELS_PATH, OPENCODE_SESSIONS_PATH},
    opencode_delivery::find_session_id,
    opencode_server::{get, OpenCodeServer},
};

/// The model a step ran on, and everything it sent and got back, which is
/// what the next step starts from.
#[derive(Debug, PartialEq, Eq)]
struct Step {
    provider: String,
    model: String,
    tokens: u64,
}

fn token_count(tokens: &Value, path: &[&str]) -> u64 {
    path.iter().try_fold(tokens, |at, key| at.get(key)).and_then(Value::as_u64).unwrap_or(0)
}

/// The newest step of a session, None while it has taken none.
fn last_step_in(body: &str) -> Result<Option<Step>, String> {
    let reply: Value = serde_json::from_str(body).map_err(|error| format!("not JSON: {error}"))?;
    let messages = reply.get("data").and_then(Value::as_array).ok_or("no data list")?;
    let Some(message) = messages.first() else { return Ok(None) };
    let text = |path: &[&str]| path.iter().try_fold(message, |at, key| at.get(key)).and_then(Value::as_str).map(str::to_string);
    let provider = text(&["model", "providerID"]).ok_or("no model provider")?;
    let model = text(&["model", "id"]).ok_or("no model")?;
    let tokens = message.get("tokens").ok_or("no tokens")?;
    let total = [&["input"][..], &["output"], &["reasoning"], &["cache", "read"], &["cache", "write"]].iter().map(|path| token_count(tokens, path)).sum();
    Ok(Some(Step { provider, model, tokens: total }))
}

fn context_limit_in(body: &str, provider: &str, model: &str) -> Result<u64, String> {
    let reply: Value = serde_json::from_str(body).map_err(|error| format!("not JSON: {error}"))?;
    let models = reply.get("data").and_then(Value::as_array).ok_or("no data list")?;
    models
        .iter()
        .find(|entry| entry.get("providerID").and_then(Value::as_str) == Some(provider) && entry.get("id").and_then(Value::as_str) == Some(model))
        .and_then(|entry| entry.pointer("/limit/context")?.as_u64())
        .filter(|limit| *limit > 0)
        .ok_or_else(|| format!("no context limit for {provider}/{model}"))
}

/// Whole percent, never past 100: a step can use more than the size it was told.
fn percent_of(tokens: u64, limit: u64) -> u8 {
    (tokens.saturating_mul(100) / limit).min(100) as u8
}

/// The percentage for a session, None while it has taken no step.
pub async fn context_percent(server: &OpenCodeServer, session_id: &str) -> Result<Option<u8>, String> {
    let steps = get(server, &format!("{OPENCODE_SESSIONS_PATH}/{session_id}/{OPENCODE_LAST_STEP_QUERY}")).await?;
    let Some(step) = last_step_in(&steps)? else { return Ok(None) };
    let models = get(server, OPENCODE_MODELS_PATH).await?;
    Ok(Some(percent_of(step.tokens, context_limit_in(&models, &step.provider, &step.model)?)))
}

/// What the server says about a session's conversation: its ID, if it has
/// one yet, and how full it is, if that can be told.
pub async fn ask_conversation(server: &OpenCodeServer, known: Option<String>) -> (Option<String>, Option<u8>) {
    let session_id = match known {
        Some(session_id) => Some(session_id),
        None => find_session_id(server).await.ok(),
    };
    let Some(id) = &session_id else { return (None, None) };
    let percent = context_percent(server, id).await.ok().flatten();
    (session_id, percent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opencode_testing::{fake_server, http, ok};

    const STEP: &str = r#"{"data":[{"type":"assistant","model":{"id":"qwen","providerID":"spark-big"},"tokens":{"input":100000,"output":500,"reasoning":0,"cache":{"read":10000,"write":4200}}}]}"#;
    const MODELS: &str = r#"{"data":[{"id":"qwen","providerID":"other","limit":{"context":1000}},{"id":"qwen","providerID":"spark-big","limit":{"context":131072,"output":8192}}]}"#;

    #[test]
    fn a_step_counts_everything_it_sent_and_got_back() {
        assert_eq!(last_step_in(STEP), Ok(Some(Step { provider: "spark-big".into(), model: "qwen".into(), tokens: 114_700 })));
    }

    #[test]
    fn no_step_yet_is_not_an_error() {
        assert_eq!(last_step_in(r#"{"data":[]}"#), Ok(None));
    }

    #[test]
    fn a_step_without_what_it_needs_is_an_error() {
        assert!(last_step_in("nope").is_err());
        assert!(last_step_in(r#"{"data":[{"model":{"id":"q","providerID":"p"}}]}"#).is_err());
        assert!(last_step_in(r#"{"data":[{"tokens":{}}]}"#).is_err());
    }

    #[test]
    fn the_limit_is_the_one_of_this_providers_model() {
        assert_eq!(context_limit_in(MODELS, "spark-big", "qwen"), Ok(131_072));
        assert!(context_limit_in(MODELS, "spark-big", "other").is_err());
        assert!(context_limit_in(r#"{"data":[{"id":"q","providerID":"p","limit":{"context":0}}]}"#, "p", "q").is_err());
    }

    #[test]
    fn the_percentage_rounds_down_and_stops_at_100() {
        assert_eq!(percent_of(114_700, 131_072), 87);
        assert_eq!(percent_of(0, 131_072), 0);
        assert_eq!(percent_of(200_000, 131_072), 100);
    }

    #[tokio::test]
    async fn the_percentage_comes_from_the_newest_step_and_its_models_limit() {
        let (server, serving) = fake_server(vec![ok(STEP), ok(MODELS)]);
        assert_eq!(context_percent(&server, "ses_1").await, Ok(Some(87)));
        let requests = serving.join().unwrap();
        assert!(requests[0].starts_with("GET /api/session/ses_1/message?type=assistant&order=desc&limit=1 HTTP/1.1"), "{}", requests[0]);
        assert!(requests[1].starts_with("GET /api/model HTTP/1.1"), "{}", requests[1]);
    }

    #[tokio::test]
    async fn a_session_with_no_step_isnt_asked_about_models() {
        let (server, serving) = fake_server(vec![ok(r#"{"data":[]}"#)]);
        assert_eq!(context_percent(&server, "ses_1").await, Ok(None));
        assert_eq!(serving.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_refusing_server_is_an_error() {
        let (server, serving) = fake_server(vec![Some(http("500 Internal Server Error", "boom"))]);
        assert!(context_percent(&server, "ses_1").await.is_err());
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn a_known_session_goes_straight_to_its_steps() {
        let (server, serving) = fake_server(vec![ok(STEP), ok(MODELS)]);
        assert_eq!(ask_conversation(&server, Some("ses_1".into())).await, (Some("ses_1".into()), Some(87)));
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn an_unknown_session_is_looked_up_by_its_title_first() {
        let list = r#"{"data":[{"id":"ses_9","title":"tag-1"}]}"#;
        let (server, serving) = fake_server(vec![ok(list), ok(STEP), ok(MODELS)]);
        assert_eq!(ask_conversation(&server, None).await, (Some("ses_9".into()), Some(87)));
        let requests = serving.join().unwrap();
        assert!(requests[1].starts_with("GET /api/session/ses_9/message?"), "{}", requests[1]);
    }

    #[tokio::test]
    async fn no_conversation_yet_says_nothing() {
        let (server, serving) = fake_server(vec![ok(r#"{"data":[]}"#)]);
        assert_eq!(ask_conversation(&server, None).await, (None, None));
        serving.join().unwrap();
    }
}
