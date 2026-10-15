//! A call that carries a key runs once, however many times it's asked: a
//! caller whose connection dropped asks again and gets the first run's
//! outcome instead of a second run.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use legion_proto::{Caller, Outcome, NAME};
use tokio::sync::OnceCell;

/// How many finished calls are kept to answer a repeat.
const MAX_KEPT_CALLS: usize = 256;
/// A repeat comes within one call's retry, so a finished call is kept only briefly.
const MAX_KEPT_AGE: Duration = Duration::from_secs(15 * 60);

/// The caller is part of the key, so one session can't read another's outcome.
type CallKey = (Option<Caller>, String);

struct Call {
    outcome: Arc<OnceCell<Outcome>>,
    asked_at: Instant,
}

#[derive(Default)]
pub struct RequestReplays {
    calls: Mutex<HashMap<CallKey, Call>>,
}

/// Forgets finished calls that are old, then the oldest finished ones past the cap.
fn forget_stale(calls: &mut HashMap<CallKey, Call>, now: Instant) {
    calls.retain(|_, call| !call.outcome.initialized() || now.duration_since(call.asked_at) < MAX_KEPT_AGE);
    let excess = calls.len().saturating_sub(MAX_KEPT_CALLS);
    if excess == 0 {
        return;
    }
    let mut finished: Vec<(Instant, CallKey)> = calls.iter().filter(|(_, call)| call.outcome.initialized()).map(|(key, call)| (call.asked_at, key.clone())).collect();
    finished.sort_by_key(|(asked_at, _)| *asked_at);
    finished.into_iter().take(excess).for_each(|(_, key)| {
        calls.remove(&key);
    });
}

impl RequestReplays {
    /// Runs `work` for the first ask of this key; a repeat waits for that run
    /// and gets its outcome. The run goes on if the asker goes away.
    pub async fn run_once<Work>(&self, from: Option<Caller>, key: String, work: Work) -> Outcome
    where
        Work: Future<Output = Outcome> + Send + 'static,
    {
        let outcome = {
            let mut calls = self.calls.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let now = Instant::now();
            forget_stale(&mut calls, now);
            calls.entry((from, key)).or_insert_with(|| Call { outcome: Arc::default(), asked_at: now }).outcome.clone()
        };
        // In its own task so the first run isn't dropped with its asker, which would let a repeat run it again.
        let task = tokio::spawn(async move { outcome.get_or_init(|| work).await.clone() });
        task.await.unwrap_or_else(|error| Outcome::Error(format!("{NAME}d failed: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn counting_work(runs: &Arc<AtomicUsize>, delay: Duration) -> impl Future<Output = Outcome> + Send + 'static {
        let runs = runs.clone();
        async move {
            tokio::time::sleep(delay).await;
            Outcome::Error(format!("run {}", runs.fetch_add(1, Ordering::SeqCst) + 1))
        }
    }

    fn text(outcome: Outcome) -> String {
        match outcome {
            Outcome::Error(text) => text,
            Outcome::Ok(_) => panic!("expected the test's error outcome"),
        }
    }

    #[tokio::test]
    async fn a_repeat_after_the_run_gets_the_same_outcome() {
        let replays = RequestReplays::default();
        let runs = Arc::new(AtomicUsize::new(0));
        let first = replays.run_once(None, "k".into(), counting_work(&runs, Duration::ZERO)).await;
        let second = replays.run_once(None, "k".into(), counting_work(&runs, Duration::ZERO)).await;
        assert_eq!((text(first), text(second)), ("run 1".into(), "run 1".into()));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_repeat_while_it_runs_waits_even_if_the_first_asker_left() {
        let replays = Arc::new(RequestReplays::default());
        let runs = Arc::new(AtomicUsize::new(0));
        let first = tokio::spawn({
            let (replays, work) = (replays.clone(), counting_work(&runs, Duration::from_millis(100)));
            async move { replays.run_once(None, "k".into(), work).await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        first.abort();
        let repeat = replays.run_once(None, "k".into(), counting_work(&runs, Duration::ZERO)).await;
        assert_eq!(text(repeat), "run 1");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn another_caller_with_the_same_key_runs_its_own() {
        let replays = RequestReplays::default();
        let runs = Arc::new(AtomicUsize::new(0));
        let other = Caller { deployment: "d".into(), position: "p".into() };
        replays.run_once(None, "k".into(), counting_work(&runs, Duration::ZERO)).await;
        replays.run_once(Some(other), "k".into(), counting_work(&runs, Duration::ZERO)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn only_finished_calls_are_forgotten() {
        let finished = || Arc::new(OnceCell::new_with(Some(Outcome::Error(String::new()))));
        let now = Instant::now();
        let mut calls: HashMap<CallKey, Call> = HashMap::new();
        for index in 0..MAX_KEPT_CALLS + 3 {
            calls.insert((None, index.to_string()), Call { outcome: finished(), asked_at: now });
        }
        calls.insert((None, "running".into()), Call { outcome: Arc::default(), asked_at: now - MAX_KEPT_AGE });
        forget_stale(&mut calls, now);
        assert_eq!(calls.len(), MAX_KEPT_CALLS);
        assert!(calls.contains_key(&(None, "running".into())));
    }
}
