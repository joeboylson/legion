//! Posting a message to an OpenCode session's server, and finding which
//! session it holds. The HTTP itself is opencode_server's.

use std::future::Future;

use legion_proto::{COMMANDER, HUMAN};

use crate::{
    constants::{NOT_FOUND_STATUS, OPENCODE_AGENT, OPENCODE_SESSIONS_PATH},
    opencode_server::{get, patch, post, sessions_search_path, this_runs_session, OpenCodeServer},
};

/// Why a message couldn't be posted just now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The server is up but doesn't hold this run's conversation yet.
    NoSessionYet,
    /// The server no longer knows the conversation Legion had saved.
    SessionGone,
    Other(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::NoSessionYet => write!(formatter, "its server has no session of this run yet"),
            Failure::SessionGone => write!(formatter, "its server no longer has the session Legion saved"),
            Failure::Other(reason) => write!(formatter, "{reason}"),
        }
    }
}

/// The ID of this run's session on its server, once it has one.
pub async fn find_session_id(server: &OpenCodeServer) -> Result<String, Failure> {
    let body = get(server, &sessions_search_path(server)).await.map_err(|error| Failure::Other(format!("can't list its server's sessions: {error}")))?;
    let found = this_runs_session(&body, &server.tag).map_err(|error| Failure::Other(format!("can't read its server's session list: {error}")))?;
    found.ok_or(Failure::NoSessionYet)
}

/// How a message meets a session that is busy: behind its turn, or into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Queue,
    Steer,
}

impl Delivery {
    fn as_str(self) -> &'static str {
        match self {
            Delivery::Queue => "queue",
            Delivery::Steer => "steer",
        }
    }

    /// The commander and the admin steer a running turn, so a correction isn't stuck behind it; the rest queue.
    pub fn from_sender(sender: &str) -> Self {
        match sender == COMMANDER || sender == HUMAN {
            true => Delivery::Steer,
            false => Delivery::Queue,
        }
    }
}

/// Ends the session's current turn, if it is in one. Its parked messages wait for the next one.
pub async fn interrupt_turn(server: &OpenCodeServer, session_id: &str) -> Result<(), String> {
    let path = format!("{OPENCODE_SESSIONS_PATH}/{session_id}/interrupt");
    post(server, &path, "").await.map(|_| ()).map_err(|error| format!("its server refused the interrupt: {error}"))
}

/// After an interrupt, queued messages stay parked in the session's inbox
/// until the next message. Steering them makes them run now, so a handoff
/// leaves nothing waiting.
pub async fn steer_parked(server: &OpenCodeServer, session_id: &str) -> Result<(), String> {
    let inbox_path = format!("{OPENCODE_SESSIONS_PATH}/{session_id}/inbox");
    let listing = get(server, &inbox_path).await.map_err(|error| format!("can't read its inbox: {error}"))?;
    let listing: serde_json::Value = serde_json::from_str(&listing).map_err(|error| format!("can't read its inbox: not JSON: {error}"))?;
    let items = listing.get("data").and_then(serde_json::Value::as_array).ok_or("can't read its inbox: no data list")?;
    let body = serde_json::json!({ "delivery": Delivery::Steer.as_str() }).to_string();
    for id in items.iter().filter_map(|item| item.get("id").and_then(serde_json::Value::as_str)) {
        patch(server, &format!("{inbox_path}/{id}"), &body).await.map_err(|error| format!("its server refused to steer inbox item {id}: {error}"))?;
    }
    Ok(())
}

/// Posts one message, to queue behind the session's turn or to steer into it.
pub async fn post_message(server: &OpenCodeServer, session_id: &str, text: &str, delivery: Delivery) -> Result<(), Failure> {
    let path = format!("{OPENCODE_SESSIONS_PATH}/{session_id}/prompt");
    let body = serde_json::json!({ "text": text, "delivery": delivery.as_str(), "agent": OPENCODE_AGENT }).to_string();
    post(server, &path, &body).await.map(|_| ()).map_err(|error| match error.starts_with(NOT_FOUND_STATUS) {
        true => Failure::SessionGone,
        false => Failure::Other(format!("its server refused the message: {error}")),
    })
}

/// Posts messages in order, telling `on_delivered` after each success. Stops
/// at the first failure, so none is passed over: that entry and the reason.
pub async fn deliver_in_order<Recorded: Future<Output = ()>>(server: &OpenCodeServer, session_id: &str, messages: &[(i64, String, Delivery)], mut on_delivered: impl FnMut(i64) -> Recorded) -> Option<(i64, Failure)> {
    for (entry_id, text, delivery) in messages {
        match post_message(server, session_id, text, *delivery).await {
            Ok(()) => on_delivered(*entry_id).await,
            Err(reason) => return Some((*entry_id, reason)),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;
    use crate::opencode_testing::{fake_server, http, ok};

    #[tokio::test]
    async fn a_posted_message_goes_to_the_prompt_path_with_auth_queue_delivery_and_the_legion_agent() {
        let (server, serving) = fake_server(vec![ok("{}")]);
        post_message(&server, "ses_1", "[legion] hi \"there\"", Delivery::Queue).await.unwrap();
        let request = serving.join().unwrap().remove(0);
        assert!(request.starts_with("POST /api/session/ses_1/prompt HTTP/1.1\r\n"), "{request}");
        assert!(request.contains("authorization: Basic b3BlbmNvZGU6cHc=\r\n"), "{request}");
        let body: serde_json::Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body, serde_json::json!({ "text": "[legion] hi \"there\"", "delivery": "queue", "agent": "legion" }));
    }

    #[test]
    fn only_the_commander_and_the_admin_steer() {
        assert_eq!(["commander", "admin"].map(Delivery::from_sender), [Delivery::Steer; 2]);
        assert_eq!(["builder", "builder-2", "checker", "legion"].map(Delivery::from_sender), [Delivery::Queue; 4]);
    }

    #[tokio::test]
    async fn a_steered_message_says_steer() {
        let (server, serving) = fake_server(vec![ok("{}")]);
        post_message(&server, "ses_1", "stop", Delivery::Steer).await.unwrap();
        let request = serving.join().unwrap().remove(0);
        assert!(request.contains(r#""delivery":"steer""#), "{request}");
    }

    #[tokio::test]
    async fn an_interrupt_posts_to_the_sessions_interrupt_path() {
        let (server, serving) = fake_server(vec![ok(r#"{"interrupted":true}"#)]);
        interrupt_turn(&server, "ses_1").await.unwrap();
        let request = serving.join().unwrap().remove(0);
        assert!(request.starts_with("POST /api/session/ses_1/interrupt HTTP/1.1\r\n"), "{request}");
        let (server, serving) = fake_server(vec![Some(http("500 Internal Server Error", ""))]);
        assert!(interrupt_turn(&server, "ses_1").await.unwrap_err().contains("500"));
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn every_parked_message_is_steered_after_an_interrupt() {
        let inbox = r#"{"data":[{"id":"msg_a","delivery":"queue"},{"id":"msg_b","delivery":"queue"}]}"#;
        let (server, serving) = fake_server(vec![ok(inbox), ok("{}"), ok("{}")]);
        steer_parked(&server, "ses_1").await.unwrap();
        let sent = serving.join().unwrap();
        assert!(sent[0].starts_with("GET /api/session/ses_1/inbox HTTP/1.1\r\n"), "{}", sent[0]);
        assert!(sent[1].starts_with("PATCH /api/session/ses_1/inbox/msg_a HTTP/1.1\r\n") && sent[1].ends_with(r#"{"delivery":"steer"}"#), "{}", sent[1]);
        assert!(sent[2].starts_with("PATCH /api/session/ses_1/inbox/msg_b "), "{}", sent[2]);
    }

    #[tokio::test]
    async fn an_empty_inbox_needs_no_patch() {
        let (server, serving) = fake_server(vec![ok(r#"{"data":[]}"#)]);
        steer_parked(&server, "ses_1").await.unwrap();
        assert_eq!(serving.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn messages_are_posted_in_order_and_each_success_is_reported() {
        let (server, serving) = fake_server(vec![ok("{}"), ok("{}")]);
        let mut delivered = Vec::new();
        let failure = deliver_in_order(&server, "ses_1", &[(4, "first".into(), Delivery::Queue), (9, "second".into(), Delivery::Queue)], |id| {
            delivered.push(id);
            async {}
        })
        .await;
        assert_eq!((failure, delivered), (None, vec![4, 9]));
        let sent = serving.join().unwrap();
        assert!(sent[0].contains("\"first\"") && sent[1].contains("\"second\""));
    }

    #[tokio::test]
    async fn a_server_error_stops_delivery_and_gives_the_reason() {
        let (server, serving) = fake_server(vec![ok("{}"), Some(http("500 Internal Server Error", "boom"))]);
        let mut delivered = Vec::new();
        let failure = deliver_in_order(&server, "ses_1", &[(1, "a".into(), Delivery::Queue), (2, "b".into(), Delivery::Queue), (3, "c".into(), Delivery::Queue)], |id| {
            delivered.push(id);
            async {}
        })
        .await;
        serving.join().unwrap();
        assert_eq!(delivered, vec![1]);
        assert_eq!(failure, Some((2, Failure::Other("its server refused the message: 500 Internal Server Error".to_string()))));
    }

    #[tokio::test]
    async fn a_saved_session_the_server_no_longer_has_is_gone_not_refused() {
        let (server, serving) = fake_server(vec![Some(http("404 Not Found", ""))]);
        assert_eq!(post_message(&server, "ses_1", "hi", Delivery::Queue).await, Err(Failure::SessionGone));
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn a_server_that_is_gone_fails_the_delivery() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let server = OpenCodeServer { port, password: "x".into(), tag: "tag-1".into() };
        let mut delivered = Vec::new();
        let failure = deliver_in_order(&server, "ses_1", &[(1, "a".into(), Delivery::Queue)], |id| {
            delivered.push(id);
            async {}
        })
        .await;
        assert!(delivered.is_empty());
        let (entry_id, reason) = failure.unwrap();
        assert_eq!(entry_id, 1);
        assert!(reason.to_string().starts_with("its server refused the message: "), "{reason}");
    }

    #[tokio::test]
    async fn the_session_id_is_the_one_titled_with_the_servers_tag() {
        let listing = r#"{"data":[
            {"id":"ses_older","title":"tag-0","time":{"created":500},"location":{"directory":"/work"}},
            {"id":"ses_ours","title":"tag-1","time":{"created":2000},"location":{"directory":"/work"}},
            {"id":"ses_newer","title":"tag-2","time":{"created":9000},"location":{"directory":"/work"}}],"cursor":{}}"#;
        let (server, serving) = fake_server(vec![ok(listing)]);
        assert_eq!(find_session_id(&server).await.unwrap(), "ses_ours");
        assert!(serving.join().unwrap()[0].starts_with("GET /api/session?search=tag-1 HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn no_session_with_the_tag_yet_is_a_failure_to_retry() {
        let (server, serving) = fake_server(vec![ok(r#"{"data":[{"id":"ses_other","title":"tag-2"}]}"#)]);
        assert_eq!(find_session_id(&server).await.unwrap_err(), Failure::NoSessionYet);
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn two_sessions_with_the_tag_are_not_guessed_between() {
        let (server, serving) = fake_server(vec![ok(r#"{"data":[{"id":"a","title":"tag-1"},{"id":"b","title":"tag-1"}]}"#)]);
        let error = find_session_id(&server).await.unwrap_err().to_string();
        assert!(error.contains("more than one OpenCode session is titled tag-1"), "{error}");
        serving.join().unwrap();
    }
}
