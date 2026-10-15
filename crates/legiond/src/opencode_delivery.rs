//! Posting a message to an OpenCode session's server, and finding which
//! session it holds. The HTTP itself is opencode_server's.

use std::future::Future;

use crate::{
    constants::{NOT_FOUND_STATUS, OPENCODE_SESSIONS_PATH},
    opencode_server::{get, post, sessions_search_path, this_runs_session, OpenCodeServer},
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

/// Queues one message behind whatever the session is doing.
pub async fn post_message(server: &OpenCodeServer, session_id: &str, text: &str) -> Result<(), Failure> {
    let path = format!("{OPENCODE_SESSIONS_PATH}/{session_id}/prompt");
    let body = serde_json::json!({ "text": text, "delivery": "queue" }).to_string();
    post(server, &path, &body).await.map(|_| ()).map_err(|error| match error.starts_with(NOT_FOUND_STATUS) {
        true => Failure::SessionGone,
        false => Failure::Other(format!("its server refused the message: {error}")),
    })
}

/// Posts messages in order, telling `on_delivered` after each success. Stops
/// at the first failure, so none is passed over: that entry and the reason.
pub async fn deliver_in_order<Recorded: Future<Output = ()>>(server: &OpenCodeServer, session_id: &str, messages: &[(i64, String)], mut on_delivered: impl FnMut(i64) -> Recorded) -> Option<(i64, Failure)> {
    for (entry_id, text) in messages {
        match post_message(server, session_id, text).await {
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
    async fn a_posted_message_goes_to_the_prompt_path_with_auth_and_queue_delivery() {
        let (server, serving) = fake_server(vec![ok("{}")]);
        post_message(&server, "ses_1", "[legion] hi \"there\"").await.unwrap();
        let request = serving.join().unwrap().remove(0);
        assert!(request.starts_with("POST /api/session/ses_1/prompt HTTP/1.1\r\n"), "{request}");
        assert!(request.contains("authorization: Basic b3BlbmNvZGU6cHc=\r\n"), "{request}");
        let body: serde_json::Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body, serde_json::json!({ "text": "[legion] hi \"there\"", "delivery": "queue" }));
    }

    #[tokio::test]
    async fn messages_are_posted_in_order_and_each_success_is_reported() {
        let (server, serving) = fake_server(vec![ok("{}"), ok("{}")]);
        let mut delivered = Vec::new();
        let failure = deliver_in_order(&server, "ses_1", &[(4, "first".into()), (9, "second".into())], |id| {
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
        let failure = deliver_in_order(&server, "ses_1", &[(1, "a".into()), (2, "b".into()), (3, "c".into())], |id| {
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
        assert_eq!(post_message(&server, "ses_1", "hi").await, Err(Failure::SessionGone));
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn a_server_that_is_gone_fails_the_delivery() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let server = OpenCodeServer { port, password: "x".into(), tag: "tag-1".into() };
        let mut delivered = Vec::new();
        let failure = deliver_in_order(&server, "ses_1", &[(1, "a".into())], |id| {
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
