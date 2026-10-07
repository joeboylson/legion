//! The app in a browser: legion2d serves the built app and its WebSocket on
//! a local web address, so a browser tab can stay open while legion2d or
//! the app restart. Only this machine can reach it, and only pages it served
//! itself may connect: any website open in the same browser could otherwise
//! drive legion2d.

use std::{net::SocketAddr, path::Path, sync::Arc};

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

use crate::{constants::WEBSOCKET_ROUTE, daemon::Daemon, server::upgrade_to_websocket};

pub const WEB_HOST: [u8; 4] = [127, 0, 0, 1];
const LOCAL_HOST_NAMES: &[&str] = &["127.0.0.1", "localhost"];
/// The page is rebuilt while the app is worked on; a reload should always get the newest.
const NO_CACHE: &str = "no-cache";

pub fn web_address(port: u16) -> SocketAddr {
    SocketAddr::from((WEB_HOST, port))
}

/// A request is local when it's addressed to this machine on legion2d's own
/// port (which stops a website renamed to point here), and, if a page sent
/// it, that page came from the same address.
pub fn is_local_request(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let local_hosts: Vec<String> = LOCAL_HOST_NAMES.iter().map(|name| format!("{name}:{port}")).collect();
    let is_local_host = host.is_some_and(|host| local_hosts.iter().any(|local| local == host));
    let is_local_origin = origin.is_none_or(|origin| local_hosts.iter().any(|local| origin == format!("http://{local}")));
    is_local_host && is_local_origin
}

async fn refuse_other_sites(State(port): State<u16>, request: Request, next: Next) -> Response {
    // Worked out before the await: a borrow of the request held across it would keep the handler from moving between threads.
    let is_local = {
        let header_text = |name: header::HeaderName| request.headers().get(name).and_then(|value| value.to_str().ok());
        is_local_request(header_text(header::HOST), header_text(header::ORIGIN), port)
    };
    if !is_local {
        return (StatusCode::FORBIDDEN, "only pages legion2d serves itself may use it").into_response();
    }
    next.run(request).await
}

pub fn web_router(daemon: Arc<Daemon>, app_folder: &Path, port: u16) -> Router {
    let pages = ServeDir::new(app_folder);
    Router::new()
        .route(WEBSOCKET_ROUTE, get(upgrade_to_websocket))
        .with_state(daemon)
        .fallback_service(pages)
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static(NO_CACHE)))
        .layer(middleware::from_fn_with_state(port, refuse_other_sites))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_on_its_port_is_local() {
        assert!(is_local_request(Some("127.0.0.1:4610"), None, 4610));
        assert!(is_local_request(Some("localhost:4610"), Some("http://localhost:4610"), 4610));
        assert!(is_local_request(Some("127.0.0.1:4610"), Some("http://127.0.0.1:4610"), 4610));
    }

    #[test]
    fn other_sites_and_other_names_are_refused() {
        assert!(!is_local_request(Some("127.0.0.1:4610"), Some("https://example.com"), 4610));
        assert!(!is_local_request(Some("evil.example:4610"), None, 4610));
        assert!(!is_local_request(Some("127.0.0.1:9999"), None, 4610));
        assert!(!is_local_request(None, None, 4610));
        assert!(!is_local_request(Some("127.0.0.1:4610"), Some("http://127.0.0.1:4610.evil.example"), 4610));
    }
}
