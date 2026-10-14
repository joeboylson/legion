//! Where a message between deployments goes next. A Legion knows the
//! deployments open here, each subscriber's (while it hosts), and each host's
//! list (for each channel it subscribes to).

/// Deployment keys reachable through one connection, by its address.
pub type Reachable<'a> = &'a [(String, Vec<String>)];

#[derive(Debug, PartialEq)]
pub enum Route {
    Here,
    /// Over a subscriber's connection to this host.
    Subscriber(String),
    /// Over this Legion's subscription to a host.
    Host(String),
    Nowhere,
}

fn find_in(reachable: Reachable, key: &str) -> Option<String> {
    reachable.iter().find(|(_, keys)| keys.iter().any(|reached| reached == key)).map(|(address, _)| address.clone())
}

/// A message a commander here sends. It goes over a channel whenever one
/// reaches the deployment, even one open here (a Legion subscribed to
/// itself), so it takes the same path as between machines.
pub fn route_from_here(to: &str, here: &[String], subscribers: Reachable, hosts: Reachable) -> Route {
    if let Some(address) = find_in(hosts, to) {
        return Route::Host(address);
    }
    if let Some(address) = find_in(subscribers, to) {
        return Route::Subscriber(address);
    }
    if here.iter().any(|key| key == to) {
        return Route::Here;
    }
    Route::Nowhere
}

/// A message arriving at the host from a subscriber: delivered here, or
/// passed on to the subscriber whose deployment it is.
pub fn route_at_host(to: &str, here: &[String], subscribers: Reachable) -> Route {
    if here.iter().any(|key| key == to) {
        return Route::Here;
    }
    find_in(subscribers, to).map(Route::Subscriber).unwrap_or(Route::Nowhere)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|key| key.to_string()).collect()
    }

    fn reachable(address: &str, list: &[&str]) -> Vec<(String, Vec<String>)> {
        vec![(address.to_string(), keys(list))]
    }

    #[test]
    fn a_message_from_here_goes_over_a_channel_before_staying_here() {
        let here = keys(&["m/a", "m/b"]);
        assert_eq!(route_from_here("m/b", &here, &[], &reachable("127.0.0.1:4620", &["m/a", "m/b"])), Route::Host("127.0.0.1:4620".into()));
        assert_eq!(route_from_here("m/b", &here, &[], &[]), Route::Here);
    }

    #[test]
    fn a_message_from_here_reaches_a_subscriber_while_hosting() {
        assert_eq!(route_from_here("n/c", &keys(&["m/a"]), &reachable("10.0.0.2:5000", &["n/c"]), &[]), Route::Subscriber("10.0.0.2:5000".into()));
    }

    #[test]
    fn the_host_delivers_its_own_and_passes_on_the_rest() {
        let subscribers = reachable("10.0.0.2:5000", &["n/c"]);
        assert_eq!(route_at_host("m/a", &keys(&["m/a"]), &subscribers), Route::Here);
        assert_eq!(route_at_host("n/c", &keys(&["m/a"]), &subscribers), Route::Subscriber("10.0.0.2:5000".into()));
    }

    #[test]
    fn a_closed_or_unknown_deployment_is_nowhere() {
        assert_eq!(route_from_here("x/z", &keys(&["m/a"]), &[], &[]), Route::Nowhere);
        assert_eq!(route_at_host("x/z", &keys(&["m/a"]), &[]), Route::Nowhere);
    }
}
