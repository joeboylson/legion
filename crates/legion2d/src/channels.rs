//! The channels this machine hosts and subscribes to, kept connected for as
//! long as legion2d runs. A subscriber checks in every 10 seconds; either
//! end that hears nothing for 25 calls the other down, and a subscriber
//! keeps trying to reconnect. Every change goes out as an event, so the app
//! shows each end coming and going.
//!
//! Over each connection the ends also say which deployments they have open,
//! and pass messages between deployments' commanders. The host keeps the
//! list of every deployment it can reach and passes messages on. A message
//! for a deployment that has closed comes back to its sender, and a
//! commander is told when a deployment it has talked to leaves the channels.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use legion2_proto::{ChannelDeployment, ChannelEnd, ChannelLogEntry, ChannelLogKind, ChannelSwitch, Channels, DeploymentSwitches, Event, HostedChannel, Subscription, LEGION};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    runtime::Handle,
    sync::{broadcast, mpsc},
    task::{JoinHandle, JoinSet},
    time::{interval, sleep, timeout},
};

use crate::{
    channel_log::{ChannelLog, LogLine},
    channel_routes::{route_at_host, route_from_here, Route},
    channel_settings::{read_channel_settings, write_channel_settings, ChannelSettings, HostedSettings, SubscriptionSettings},
    channel_wire::{answer_hello, decode, encode, WireMessage},
    constants::{CHANNEL_CHECK_INTERVAL, CHANNEL_LISTEN_ADDRESS, CHANNEL_RECONNECT_DELAY, CHANNEL_SILENCE_LIMIT},
    ids::new_id,
};

/// What the channels hand to the deployments here.
pub enum Inbound {
    /// A message for the commander of `to`, a deployment open here.
    Message { to: String, from: ChannelDeployment, text: String },
    /// A message from `from`, a deployment here, that didn't get through.
    Undelivered { from: String, to: String, reason: String },
    /// A deployment that `here` has talked to has left the channels.
    Gone { here: String, gone: ChannelDeployment },
}

/// Hands a message to a deployment here; an error when it can't (it closed).
pub type Deliver = Arc<dyn Fn(Inbound) -> Result<(), String> + Send + Sync>;

type Outbox = mpsc::UnboundedSender<WireMessage>;

struct SubscriberLive {
    end: ChannelEnd,
    deployments: Vec<ChannelDeployment>,
    outbox: Outbox,
}

#[derive(Default, Clone)]
struct SubscriptionState {
    is_up: bool,
    host_machine: Option<String>,
    problem: Option<String>,
    /// Every deployment the host can reach, as it last said.
    directory: Vec<ChannelDeployment>,
    outbox: Option<Outbox>,
}

/// What's known now about each end, shared with the tasks that keep them connected.
#[derive(Default)]
struct Live {
    settings: ChannelSettings,
    hosted_problem: Option<String>,
    /// This machine's open deployments.
    local: Vec<ChannelDeployment>,
    descriptions: HashMap<String, String>,
    /// By the subscriber's address.
    subscribers: HashMap<String, SubscriberLive>,
    /// By the address subscribed to.
    subscriptions: HashMap<String, SubscriptionState>,
    /// Every deployment reachable, as last worked out.
    directory: Vec<ChannelDeployment>,
    /// What this host last told its subscribers it can reach.
    hosted_view: Vec<ChannelDeployment>,
    /// For each deployment elsewhere, the deployments here that have talked to it.
    contacts: HashMap<String, HashSet<String>>,
}

/// One list from several, each deployment once, in key order.
fn merged(lists: impl IntoIterator<Item = Vec<ChannelDeployment>>) -> Vec<ChannelDeployment> {
    let mut by_key: HashMap<String, ChannelDeployment> = HashMap::new();
    lists.into_iter().flatten().for_each(|deployment| {
        by_key.insert(deployment.key.clone(), deployment);
    });
    let mut deployments: Vec<ChannelDeployment> = by_key.into_values().collect();
    deployments.sort_by(|first, second| first.key.cmp(&second.key));
    deployments
}

impl Live {
    /// What a host offers its subscribers: its own deployments and theirs.
    fn hosted_view(&self) -> Vec<ChannelDeployment> {
        if self.settings.hosted.is_none() {
            return Vec::new();
        }
        merged([self.local.clone()].into_iter().chain(self.subscribers.values().map(|subscriber| subscriber.deployments.clone())))
    }

    fn full_directory(&self) -> Vec<ChannelDeployment> {
        merged([self.local.clone(), self.hosted_view()].into_iter().chain(self.subscriptions.values().map(|state| state.directory.clone())))
    }

    fn local_keys(&self) -> Vec<String> {
        self.local.iter().map(|deployment| deployment.key.clone()).collect()
    }

    fn subscribers_reach(&self) -> Vec<(String, Vec<String>)> {
        self.subscribers.iter().map(|(address, subscriber)| (address.clone(), subscriber.deployments.iter().map(|deployment| deployment.key.clone()).collect())).collect()
    }

    fn hosts_reach(&self) -> Vec<(String, Vec<String>)> {
        self.subscriptions
            .iter()
            .filter(|(_, state)| state.is_up && state.outbox.is_some())
            .map(|(address, state)| (address.clone(), state.directory.iter().map(|deployment| deployment.key.clone()).collect()))
            .collect()
    }
}

struct Shared {
    live: Mutex<Live>,
    events: broadcast::Sender<Event>,
    /// This machine's name, as the other ends see it.
    machine: String,
    runtime: Option<Handle>,
    deliver: OnceLock<Deliver>,
    log: ChannelLog,
}

impl Shared {
    fn status(&self) -> Channels {
        let live = self.live.lock().unwrap();
        let hosted = live.settings.hosted.as_ref().map(|hosted| HostedChannel {
            port: hosted.port,
            key: hosted.key.clone(),
            subscribers: {
                let mut subscribers: Vec<ChannelEnd> = live.subscribers.values().map(|subscriber| subscriber.end.clone()).collect();
                subscribers.sort_by(|first, second| first.address.cmp(&second.address));
                subscribers
            },
            problem: live.hosted_problem.clone(),
        });
        let subscriptions = live
            .settings
            .subscriptions
            .iter()
            .map(|subscription| {
                let state = live.subscriptions.get(&subscription.address).cloned().unwrap_or_default();
                Subscription { address: subscription.address.clone(), is_up: state.is_up, host_machine: state.host_machine, problem: state.problem }
            })
            .collect();
        Channels { machine: self.machine.clone(), hosted, subscriptions, deployments: live.directory.clone(), switches: live.settings.switches.clone() }
    }

    /// Adds to the channel log, and tells everyone watching.
    fn note(&self, line: LogLine) {
        match self.log.record(line) {
            Ok(entry) => {
                let _ = self.events.send(Event::ChannelLogged { entry });
            }
            Err(error) => eprintln!("{LEGION}d: {error}"),
        }
    }

    /// Changes what's known, then passes on what follows from it: the
    /// host's list to its subscribers, a note for each deployment here whose
    /// contact has gone, the teams that came and went for the log, and an
    /// event for everyone watching.
    fn change(&self, update: impl FnOnce(&mut Live)) {
        let (gone_notes, team_lines) = {
            let mut live = self.live.lock().unwrap();
            update(&mut live);
            let hosted_view = live.hosted_view();
            if hosted_view != live.hosted_view {
                live.subscribers.values().for_each(|subscriber| {
                    let _ = subscriber.outbox.send(WireMessage::Deployments { deployments: hosted_view.clone() });
                });
                live.hosted_view = hosted_view;
            }
            let directory = live.full_directory();
            let gone: Vec<ChannelDeployment> = live.directory.iter().filter(|old| !directory.iter().any(|new| new.key == old.key)).cloned().collect();
            let appeared: Vec<ChannelDeployment> = directory.iter().filter(|new| !live.directory.iter().any(|old| old.key == new.key)).cloned().collect();
            live.directory = directory;
            let team_lines: Vec<LogLine> = appeared
                .iter()
                .map(|team| LogLine { from: Some(team.key.clone()), ..LogLine::new(ChannelLogKind::TeamAppeared, team_summary(team)).machine(team.machine.clone()) })
                .chain(gone.iter().map(|team| LogLine { from: Some(team.key.clone()), ..LogLine::new(ChannelLogKind::TeamGone, team_summary(team)).machine(team.machine.clone()) }))
                .collect();
            let gone_notes = gone
                .into_iter()
                .flat_map(|gone| {
                    let talked_to = live.contacts.remove(&gone.key).unwrap_or_default();
                    talked_to.into_iter().map(move |here| Inbound::Gone { here, gone: gone.clone() })
                })
                .collect::<Vec<_>>();
            (gone_notes, team_lines)
        };
        team_lines.into_iter().for_each(|line| self.note(line));
        gone_notes.into_iter().for_each(|note| self.hand_over(note));
        // No one watching is fine.
        let _ = self.events.send(Event::Channels { channels: self.status() });
    }

    /// Hands something to the deployments here, away from any lock or connection.
    fn hand_over(&self, inbound: Inbound) {
        if let Inbound::Undelivered { from, to, reason } = &inbound {
            self.note(LogLine::new(ChannelLogKind::Undelivered, reason.clone()).between(from.clone(), to.clone()));
        }
        if let (Some(runtime), Some(deliver)) = (&self.runtime, self.deliver.get()) {
            let deliver = deliver.clone();
            runtime.spawn_blocking(move || {
                let _ = deliver(inbound);
            });
        }
    }

    /// Delivers a message to a deployment here; why not, when it can't.
    async fn deliver_here(&self, to: &str, from: &ChannelDeployment, text: &str) -> Result<(), String> {
        let deliver = self.deliver.get().cloned().ok_or("this Legion isn't taking messages yet")?;
        let inbound = Inbound::Message { to: to.to_string(), from: from.clone(), text: text.to_string() };
        let result = tokio::task::spawn_blocking(move || deliver(inbound)).await.unwrap_or_else(|error| Err(error.to_string()));
        if result.is_ok() {
            self.remember_contact(&from.key, to);
            self.note(LogLine::new(ChannelLogKind::Delivered, text).machine(from.machine.clone()).between(from.key.clone(), to));
        }
        result
    }

    fn remember_contact(&self, elsewhere: &str, here: &str) {
        self.live.lock().unwrap().contacts.entry(elsewhere.to_string()).or_default().insert(here.to_string());
    }
}

#[derive(Default)]
struct Tasks {
    hosted: Option<JoinHandle<()>>,
    subscriptions: HashMap<String, JoinHandle<()>>,
}

pub struct ChannelManager {
    data_folder: PathBuf,
    shared: Arc<Shared>,
    tasks: Mutex<Tasks>,
}

impl ChannelManager {
    pub fn new(data_folder: PathBuf, events: broadcast::Sender<Event>) -> ChannelManager {
        let log = ChannelLog::open(&data_folder);
        let shared = Shared { live: Mutex::new(Live::default()), events, machine: machine_name(), runtime: Handle::try_current().ok(), deliver: OnceLock::new(), log };
        ChannelManager { data_folder, shared: Arc::new(shared), tasks: Mutex::new(Tasks::default()) }
    }

    /// How messages reach the deployments here. Set once, before any arrive.
    pub fn take_deliveries(&self, deliver: Deliver) {
        let _ = self.shared.deliver.set(deliver);
    }

    pub fn machine(&self) -> &str {
        &self.shared.machine
    }

    pub fn status(&self) -> Channels {
        self.shared.status()
    }

    /// The latest `limit` channel log entries, oldest first.
    pub fn log(&self, limit: u32) -> Vec<ChannelLogEntry> {
        self.shared.log.latest(limit as usize)
    }

    pub fn description(&self, key: &str) -> Option<String> {
        self.shared.live.lock().unwrap().descriptions.get(key).cloned()
    }

    pub fn switches(&self, deployment: &str) -> DeploymentSwitches {
        self.shared.live.lock().unwrap().settings.switches_for(deployment)
    }

    pub fn set_switches(&self, deployment: &str, send: Option<ChannelSwitch>, receive: Option<ChannelSwitch>) -> Result<Channels, String> {
        self.save(|settings| settings.with_switches(deployment, send, receive))?;
        Ok(self.status())
    }

    /// Whether `key` is an open deployment the channels can reach now.
    pub fn reaches(&self, key: &str) -> Option<ChannelDeployment> {
        self.shared.live.lock().unwrap().directory.iter().find(|deployment| deployment.key == key).cloned()
    }

    pub fn describe(&self, key: &str, text: &str) {
        self.shared.live.lock().unwrap().descriptions.insert(key.to_string(), text.trim().to_string());
    }

    /// This machine's open deployments changed: tell each host, and this channel's subscribers.
    pub fn set_local(&self, deployments: Vec<ChannelDeployment>) {
        self.shared.change(|live| {
            live.subscriptions.values().filter_map(|state| state.outbox.as_ref()).for_each(|outbox| {
                let _ = outbox.send(WireMessage::Deployments { deployments: deployments.clone() });
            });
            live.local = deployments;
        });
    }

    /// Sends a commander's message on towards the deployment `to`.
    pub fn send(&self, from: ChannelDeployment, to: &str, text: &str) -> Result<(), String> {
        let route = {
            let live = self.shared.live.lock().unwrap();
            route_from_here(to, &live.local_keys(), &live.subscribers_reach(), &live.hosts_reach())
        };
        let relay = WireMessage::Relay { from: from.clone(), to: to.to_string(), text: text.to_string() };
        let to_machine = self.shared.live.lock().unwrap().directory.iter().find(|team| team.key == to).map(|team| team.machine.clone());
        let sent_line = || {
            let line = LogLine::new(ChannelLogKind::Sent, text).between(from.key.clone(), to);
            match &to_machine {
                Some(machine) => line.machine(machine.clone()),
                None => line,
            }
        };
        let sent = {
            let live = self.shared.live.lock().unwrap();
            match &route {
                Route::Host(address) => live.subscriptions.get(address).and_then(|state| state.outbox.as_ref()).map(|outbox| outbox.send(relay).is_ok()),
                Route::Subscriber(address) => live.subscribers.get(address).map(|subscriber| subscriber.outbox.send(relay).is_ok()),
                Route::Here | Route::Nowhere => None,
            }
        };
        match (route, sent) {
            (Route::Nowhere, _) => Err(format!("no open deployment {to} on the channels; channel_deployments lists who's there")),
            (Route::Here, _) => {
                self.shared.note(sent_line());
                self.shared.hand_over(Inbound::Message { to: to.to_string(), from: from.clone(), text: text.to_string() });
                self.shared.remember_contact(to, &from.key);
                Ok(())
            }
            (_, Some(true)) => {
                self.shared.note(sent_line());
                self.shared.remember_contact(to, &from.key);
                Ok(())
            }
            _ => Err(format!("the channel to {to} just went down; try again shortly")),
        }
    }

    /// Tells `sender`, a deployment anywhere on the channels, that its
    /// message to `here` didn't get through and why. It arrives as a notice,
    /// past the sender's switches.
    pub fn turn_back(&self, sender: &str, here: &str, reason: &str) -> Result<(), String> {
        let back = WireMessage::Undeliverable { from: sender.to_string(), to: here.to_string(), text: String::new(), reason: reason.to_string() };
        let live = self.shared.live.lock().unwrap();
        let route = route_from_here(sender, &live.local_keys(), &live.subscribers_reach(), &live.hosts_reach());
        let sent = match &route {
            Route::Host(address) => live.subscriptions.get(address).and_then(|state| state.outbox.as_ref()).is_some_and(|outbox| outbox.send(back).is_ok()),
            Route::Subscriber(address) => live.subscribers.get(address).is_some_and(|subscriber| subscriber.outbox.send(back).is_ok()),
            Route::Here | Route::Nowhere => false,
        };
        drop(live);
        match route {
            Route::Here => {
                self.shared.hand_over(Inbound::Undelivered { from: sender.to_string(), to: here.to_string(), reason: reason.to_string() });
                Ok(())
            }
            _ if sent => {
                self.shared.note(LogLine::new(ChannelLogKind::Undelivered, reason).between(sender.to_string(), here.to_string()));
                Ok(())
            }
            _ => Err(format!("{sender} can't be reached on the channels to tell it")),
        }
    }

    /// Starts the channels saved last time.
    pub fn restore(&self) -> Result<(), String> {
        let settings = read_channel_settings(&self.data_folder)?;
        self.shared.change(|live| live.settings = settings.clone());
        if let Some(hosted) = &settings.hosted {
            self.start_hosting(hosted.clone());
        }
        settings.subscriptions.iter().for_each(|subscription| self.start_subscription(subscription.clone()));
        Ok(())
    }

    pub fn open(&self, port: u16, key: Option<String>) -> Result<Channels, String> {
        let hosted = HostedSettings { port, key: key.filter(|key| !key.is_empty()).unwrap_or_else(new_key) };
        self.save(|settings| settings.with_hosted(Some(hosted.clone())))?;
        self.shared.note(LogLine::new(ChannelLogKind::ChannelOpened, format!("hosting a channel on port {port}")));
        self.start_hosting(hosted);
        Ok(self.status())
    }

    pub fn close(&self) -> Result<Channels, String> {
        if let Some(task) = self.tasks.lock().unwrap().hosted.take() {
            task.abort();
        }
        self.save(|settings| settings.with_hosted(None))?;
        self.shared.note(LogLine::new(ChannelLogKind::ChannelClosed, "stopped hosting the channel"));
        self.shared.change(|live| {
            live.subscribers.clear();
            live.hosted_problem = None;
        });
        Ok(self.status())
    }

    pub fn subscribe(&self, address: &str, key: &str) -> Result<Channels, String> {
        let subscription = SubscriptionSettings { address: address.trim().to_string(), key: key.to_string() };
        if !subscription.address.contains(':') {
            return Err(format!("{address} needs a port: host:port"));
        }
        self.save(|settings| settings.with_subscription(subscription.clone()))?;
        self.start_subscription(subscription);
        Ok(self.status())
    }

    pub fn unsubscribe(&self, address: &str) -> Result<Channels, String> {
        let is_subscribed = self.shared.live.lock().unwrap().settings.subscriptions.iter().any(|subscription| subscription.address == address);
        if !is_subscribed {
            return Err(format!("not subscribed to {address}"));
        }
        if let Some(task) = self.tasks.lock().unwrap().subscriptions.remove(address) {
            task.abort();
        }
        self.save(|settings| settings.without_subscription(address))?;
        self.shared.change(|live| {
            live.subscriptions.remove(address);
        });
        Ok(self.status())
    }

    fn save(&self, change: impl FnOnce(&ChannelSettings) -> ChannelSettings) -> Result<(), String> {
        let settings = change(&self.shared.live.lock().unwrap().settings);
        write_channel_settings(&self.data_folder, &settings)?;
        self.shared.change(|live| live.settings = settings);
        Ok(())
    }

    fn start_hosting(&self, hosted: HostedSettings) {
        let Some(runtime) = &self.shared.runtime else { return };
        let task = runtime.spawn(host(self.shared.clone(), hosted));
        if let Some(previous) = self.tasks.lock().unwrap().hosted.replace(task) {
            previous.abort();
        }
        // The old channel's subscribers were cut off with it.
        self.shared.change(|live| live.subscribers.clear());
    }

    fn start_subscription(&self, subscription: SubscriptionSettings) {
        let Some(runtime) = &self.shared.runtime else { return };
        let address = subscription.address.clone();
        let task = runtime.spawn(keep_subscribed(self.shared.clone(), subscription));
        if let Some(previous) = self.tasks.lock().unwrap().subscriptions.insert(address, task) {
            previous.abort();
        }
    }
}

/// How a team reads in the log.
fn team_summary(team: &ChannelDeployment) -> String {
    format!("{} ({} on {}, pipeline {})", team.name, team.folder, team.machine, team.pipeline)
}

/// Long enough not to be guessed; it only has to be passed on by hand.
fn new_key() -> String {
    format!("{}{}", new_id(), new_id())
}

fn machine_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown machine".into())
}

async fn host(shared: Arc<Shared>, hosted: HostedSettings) {
    let listener = match TcpListener::bind((CHANNEL_LISTEN_ADDRESS, hosted.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            shared.change(|live| live.hosted_problem = Some(format!("can't listen on port {}: {error}", hosted.port)));
            return;
        }
    };
    shared.change(|live| live.hosted_problem = None);
    println!("{LEGION}d: hosting a channel on port {}", hosted.port);
    // Dropped with this task when the channel closes, which cuts off every subscriber.
    let mut subscribers = JoinSet::new();
    loop {
        while subscribers.try_join_next().is_some() {}
        if let Ok((stream, peer)) = listener.accept().await {
            subscribers.spawn(serve_subscriber(shared.clone(), stream, peer.to_string(), hosted.key.clone()));
        }
    }
}

type Reader = Lines<BufReader<OwnedReadHalf>>;

async fn read_message(lines: &mut Reader) -> Result<WireMessage, String> {
    match timeout(CHANNEL_SILENCE_LIMIT, lines.next_line()).await {
        Err(_) => Err(format!("nothing heard in {} seconds", CHANNEL_SILENCE_LIMIT.as_secs())),
        Ok(Err(error)) => Err(error.to_string()),
        Ok(Ok(None)) => Err("the other end closed the channel".into()),
        Ok(Ok(Some(line))) => decode(&line),
    }
}

async fn write_message(writing: &mut OwnedWriteHalf, message: &WireMessage) -> Result<(), String> {
    writing.write_all(encode(message).as_bytes()).await.map_err(|error| error.to_string())
}

fn not_open(to: &str) -> String {
    format!("{to} isn't open on the channels: its deployment closed, or its Legion went away")
}

async fn serve_subscriber(shared: Arc<Shared>, stream: TcpStream, address: String, key: String) {
    let (reading, mut writing) = stream.into_split();
    let mut lines = BufReader::new(reading).lines();
    let Ok(hello) = read_message(&mut lines).await else { return };
    let answer = answer_hello(&hello, &key, &shared.machine);
    if write_message(&mut writing, &answer).await.is_err() {
        return;
    }
    let WireMessage::Hello { machine, .. } = hello else { return };
    if !matches!(answer, WireMessage::Welcome { .. }) {
        return;
    }
    let (outbox, mut queued) = mpsc::unbounded_channel();
    let _ = outbox.send(WireMessage::Deployments { deployments: shared.live.lock().unwrap().hosted_view() });
    shared.note(LogLine::new(ChannelLogKind::SubscriberJoined, format!("subscribed from {address}")).machine(machine.clone()));
    shared.change(|live| {
        live.subscribers.insert(address.clone(), SubscriberLive { end: ChannelEnd { machine: machine.clone(), address: address.clone() }, deployments: Vec::new(), outbox });
    });
    // It checks in and the host answers, until it goes quiet or leaves.
    loop {
        let message = tokio::select! {
            queued_message = queued.recv() => match queued_message {
                Some(message) => {
                    if write_message(&mut writing, &message).await.is_err() { break }
                    continue;
                }
                None => break,
            },
            read = read_message(&mut lines) => match read { Ok(message) => message, Err(_) => break },
        };
        match message {
            WireMessage::Ping => {
                if write_message(&mut writing, &WireMessage::Pong).await.is_err() {
                    break;
                }
            }
            WireMessage::Deployments { deployments } => shared.change(|live| {
                if let Some(subscriber) = live.subscribers.get_mut(&address) {
                    subscriber.deployments = deployments;
                }
            }),
            WireMessage::Relay { from, to, text } => {
                let route = {
                    let live = shared.live.lock().unwrap();
                    route_at_host(&to, &live.local_keys(), &live.subscribers_reach())
                };
                let problem = match route {
                    Route::Here => shared.deliver_here(&to, &from, &text).await.err(),
                    Route::Subscriber(other) => {
                        shared.note(LogLine::new(ChannelLogKind::PassedOn, text.clone()).machine(from.machine.clone()).between(from.key.clone(), to.clone()));
                        let relay = WireMessage::Relay { from: from.clone(), to: to.clone(), text };
                        let passed_on = shared.live.lock().unwrap().subscribers.get(&other).is_some_and(|subscriber| subscriber.outbox.send(relay).is_ok());
                        (!passed_on).then(|| not_open(&to))
                    }
                    Route::Host(_) | Route::Nowhere => Some(not_open(&to)),
                };
                if let Some(reason) = problem {
                    let back = WireMessage::Undeliverable { from: from.key, to, text: String::new(), reason };
                    if write_message(&mut writing, &back).await.is_err() {
                        break;
                    }
                }
            }
            WireMessage::Undeliverable { from, to, text, reason } => {
                let route = {
                    let live = shared.live.lock().unwrap();
                    route_at_host(&from, &live.local_keys(), &live.subscribers_reach())
                };
                match route {
                    Route::Here => shared.hand_over(Inbound::Undelivered { from, to, reason }),
                    Route::Subscriber(other) => {
                        let back = WireMessage::Undeliverable { from, to, text, reason };
                        if let Some(subscriber) = shared.live.lock().unwrap().subscribers.get(&other) {
                            let _ = subscriber.outbox.send(back);
                        }
                    }
                    Route::Host(_) | Route::Nowhere => {}
                }
            }
            WireMessage::Hello { .. } | WireMessage::Welcome { .. } | WireMessage::Refused { .. } | WireMessage::Pong => {}
        }
    }
    shared.note(LogLine::new(ChannelLogKind::SubscriberLeft, format!("dropped off from {address}")).machine(machine));
    shared.change(|live| {
        live.subscribers.remove(&address);
    });
}

async fn keep_subscribed(shared: Arc<Shared>, subscription: SubscriptionSettings) {
    loop {
        let problem = subscribe_once(&shared, &subscription).await;
        let was_up = shared.live.lock().unwrap().subscriptions.get(&subscription.address).map(|state| (state.is_up, state.host_machine.clone()));
        if let Some((true, host_machine)) = was_up {
            let line = LogLine::new(ChannelLogKind::SubscriptionDown, format!("{}: {problem}", subscription.address));
            shared.note(match host_machine {
                Some(machine) => line.machine(machine),
                None => line,
            });
        }
        shared.change(|live| {
            let host_machine = live.subscriptions.get(&subscription.address).and_then(|state| state.host_machine.clone());
            live.subscriptions.insert(subscription.address.clone(), SubscriptionState { is_up: false, host_machine, problem: Some(problem), ..Default::default() });
        });
        sleep(CHANNEL_RECONNECT_DELAY).await;
    }
}

/// Connects, says hello, then checks in and passes messages until something
/// goes wrong: what went wrong.
async fn subscribe_once(shared: &Shared, subscription: &SubscriptionSettings) -> String {
    let stream = match timeout(CHANNEL_SILENCE_LIMIT, TcpStream::connect(&subscription.address)).await {
        Err(_) => return format!("no answer from {}", subscription.address),
        Ok(Err(error)) => return format!("can't reach {}: {error}", subscription.address),
        Ok(Ok(stream)) => stream,
    };
    let (reading, mut writing) = stream.into_split();
    let mut lines = BufReader::new(reading).lines();
    let hello = WireMessage::Hello { key: subscription.key.clone(), machine: shared.machine.clone() };
    if let Err(problem) = write_message(&mut writing, &hello).await {
        return problem;
    }
    let host_machine = match read_message(&mut lines).await {
        Ok(WireMessage::Welcome { machine }) => machine,
        Ok(WireMessage::Refused { reason }) => return format!("turned away: {reason}"),
        Ok(other) => return format!("unexpected answer: {other:?}"),
        Err(problem) => return problem,
    };
    let (outbox, mut queued) = mpsc::unbounded_channel();
    let _ = outbox.send(WireMessage::Deployments { deployments: shared.live.lock().unwrap().local.clone() });
    shared.note(LogLine::new(ChannelLogKind::SubscriptionUp, format!("subscribed to {}", subscription.address)).machine(host_machine.clone()));
    shared.change(|live| {
        live.subscriptions.insert(
            subscription.address.clone(),
            SubscriptionState { is_up: true, host_machine: Some(host_machine), problem: None, directory: Vec::new(), outbox: Some(outbox) },
        );
    });
    let mut check_ins = interval(CHANNEL_CHECK_INTERVAL);
    loop {
        let message = tokio::select! {
            _ = check_ins.tick() => {
                if let Err(problem) = write_message(&mut writing, &WireMessage::Ping).await { return problem }
                continue;
            }
            queued_message = queued.recv() => match queued_message {
                Some(message) => {
                    if let Err(problem) = write_message(&mut writing, &message).await { return problem }
                    continue;
                }
                None => return "the channel was replaced".into(),
            },
            read = read_message(&mut lines) => match read { Ok(message) => message, Err(problem) => return problem },
        };
        match message {
            WireMessage::Deployments { deployments } => shared.change(|live| {
                if let Some(state) = live.subscriptions.get_mut(&subscription.address) {
                    state.directory = deployments;
                }
            }),
            // The host only passes on messages for deployments here.
            WireMessage::Relay { from, to, text } => {
                if let Err(reason) = shared.deliver_here(&to, &from, &text).await {
                    let back = WireMessage::Undeliverable { from: from.key, to, text: String::new(), reason };
                    if let Err(problem) = write_message(&mut writing, &back).await {
                        return problem;
                    }
                }
            }
            WireMessage::Undeliverable { from, to, reason, .. } => shared.hand_over(Inbound::Undelivered { from, to, reason }),
            WireMessage::Pong | WireMessage::Ping | WireMessage::Hello { .. } | WireMessage::Welcome { .. } | WireMessage::Refused { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_for(manager: &ChannelManager, is_ready: impl Fn(&Channels) -> bool) -> Channels {
        for _ in 0..100 {
            let status = manager.status();
            if is_ready(&status) {
                return status;
            }
            sleep(std::time::Duration::from_millis(50)).await;
        }
        manager.status()
    }

    fn manager() -> (ChannelManager, PathBuf) {
        let folder = std::env::temp_dir().join(format!("legion2-channel-test-{}", new_id()));
        std::fs::create_dir_all(&folder).unwrap();
        let (events, _) = broadcast::channel(64);
        (ChannelManager::new(folder.clone(), events), folder)
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn deployment(key: &str) -> ChannelDeployment {
        ChannelDeployment { key: key.into(), machine: "m".into(), name: key.into(), folder: "f".into(), pipeline: "p".into(), operators: vec![], description: None }
    }

    /// Records what each Legion's deployments are handed, as text.
    fn recorder(manager: &ChannelManager, open: &[&str]) -> Arc<Mutex<Vec<String>>> {
        let received = Arc::new(Mutex::new(Vec::new()));
        let recording = received.clone();
        let open: Vec<String> = open.iter().map(|key| key.to_string()).collect();
        manager.take_deliveries(Arc::new(move |inbound| {
            let line = match inbound {
                Inbound::Message { to, from, text } if open.contains(&to) => format!("{to} got {text:?} from {}", from.key),
                Inbound::Message { to, .. } => return Err(not_open(&to)),
                Inbound::Undelivered { from, to, .. } => format!("{from} couldn't reach {to}"),
                Inbound::Gone { here, gone } => format!("{here} saw {} go", gone.key),
            };
            recording.lock().unwrap().push(line);
            Ok(())
        }));
        received
    }

    async fn wait_for_line(received: &Arc<Mutex<Vec<String>>>, line: &str) -> bool {
        for _ in 0..100 {
            if received.lock().unwrap().iter().any(|got| got == line) {
                return true;
            }
            sleep(std::time::Duration::from_millis(50)).await;
        }
        false
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscriber_with_the_key_is_let_in_and_both_ends_see_it() {
        let (hosting, hosting_folder) = manager();
        let (subscribing, subscribing_folder) = manager();
        let port = free_port();
        let key = hosting.open(port, None).unwrap().hosted.unwrap().key;
        subscribing.subscribe(&format!("127.0.0.1:{port}"), &key).unwrap();

        let subscribed = wait_for(&subscribing, |status| status.subscriptions.first().is_some_and(|subscription| subscription.is_up)).await;
        assert!(subscribed.subscriptions[0].host_machine.is_some());
        let hosted = wait_for(&hosting, |status| status.hosted.as_ref().is_some_and(|hosted| hosted.subscribers.len() == 1)).await;
        assert_eq!(hosted.hosted.unwrap().subscribers.len(), 1);

        // Closing the channel cuts the subscriber off, and it says so.
        hosting.close().unwrap();
        let cut_off = wait_for(&subscribing, |status| !status.subscriptions[0].is_up).await;
        assert!(cut_off.subscriptions[0].problem.is_some());
        [hosting_folder, subscribing_folder].iter().for_each(|folder| std::fs::remove_dir_all(folder).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_wrong_key_is_turned_away() {
        let (hosting, hosting_folder) = manager();
        let (subscribing, subscribing_folder) = manager();
        let port = free_port();
        hosting.open(port, Some("right".into())).unwrap();
        subscribing.subscribe(&format!("127.0.0.1:{port}"), "wrong").unwrap();

        let refused = wait_for(&subscribing, |status| status.subscriptions[0].problem.is_some()).await;
        assert!(!refused.subscriptions[0].is_up);
        assert!(refused.subscriptions[0].problem.as_deref().unwrap_or_default().contains("wrong key"));
        assert!(hosting.status().hosted.unwrap().subscribers.is_empty());
        [hosting_folder, subscribing_folder].iter().for_each(|folder| std::fs::remove_dir_all(folder).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deployments_message_each_other_and_hear_when_one_closes() {
        let (hosting, hosting_folder) = manager();
        let (subscribing, subscribing_folder) = manager();
        let host_received = recorder(&hosting, &["h/a"]);
        let subscriber_received = recorder(&subscribing, &["s/b"]);
        hosting.set_local(vec![deployment("h/a")]);
        subscribing.set_local(vec![deployment("s/b")]);
        let port = free_port();
        let key = hosting.open(port, None).unwrap().hosted.unwrap().key;
        subscribing.subscribe(&format!("127.0.0.1:{port}"), &key).unwrap();

        // Both ends come to know both deployments.
        let both = |status: &Channels| status.deployments.len() == 2;
        assert!(both(&wait_for(&hosting, both).await));
        assert!(both(&wait_for(&subscribing, both).await));

        // Each way, through the host.
        subscribing.send(deployment("s/b"), "h/a", "hello").unwrap();
        assert!(wait_for_line(&host_received, "h/a got \"hello\" from s/b").await);
        hosting.send(deployment("h/a"), "s/b", "hi back").unwrap();
        assert!(wait_for_line(&subscriber_received, "s/b got \"hi back\" from h/a").await);

        // The host's deployment closes: the subscriber's commander hears it
        // went, and a message to it doesn't get through.
        hosting.set_local(Vec::new());
        assert!(wait_for_line(&subscriber_received, "s/b saw h/a go").await);
        assert!(subscribing.send(deployment("s/b"), "h/a", "still there?").is_err());
        [hosting_folder, subscribing_folder].iter().for_each(|folder| std::fs::remove_dir_all(folder).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_for_a_deployment_that_just_closed_comes_back() {
        let (hosting, hosting_folder) = manager();
        let (subscribing, subscribing_folder) = manager();
        // The host still lists h/a, but its deployment has closed.
        let _host_received = recorder(&hosting, &[]);
        let subscriber_received = recorder(&subscribing, &["s/b"]);
        hosting.set_local(vec![deployment("h/a")]);
        subscribing.set_local(vec![deployment("s/b")]);
        let port = free_port();
        let key = hosting.open(port, None).unwrap().hosted.unwrap().key;
        subscribing.subscribe(&format!("127.0.0.1:{port}"), &key).unwrap();
        wait_for(&subscribing, |status| status.deployments.len() == 2).await;

        subscribing.send(deployment("s/b"), "h/a", "hello").unwrap();
        assert!(wait_for_line(&subscriber_received, "s/b couldn't reach h/a").await);
        [hosting_folder, subscribing_folder].iter().for_each(|folder| std::fs::remove_dir_all(folder).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_turned_back_reaches_its_sender_as_a_notice() {
        let (hosting, hosting_folder) = manager();
        let (subscribing, subscribing_folder) = manager();
        let host_received = recorder(&hosting, &["h/a"]);
        let _subscriber_received = recorder(&subscribing, &["s/b"]);
        hosting.set_local(vec![deployment("h/a")]);
        subscribing.set_local(vec![deployment("s/b")]);
        let port = free_port();
        let key = hosting.open(port, None).unwrap().hosted.unwrap().key;
        subscribing.subscribe(&format!("127.0.0.1:{port}"), &key).unwrap();
        wait_for(&subscribing, |status| status.deployments.len() == 2).await;

        subscribing.turn_back("h/a", "s/b", "the admin there said no").unwrap();
        assert!(wait_for_line(&host_received, "h/a couldn't reach s/b").await);
        assert!(subscribing.turn_back("x/gone", "s/b", "no").is_err());
        [hosting_folder, subscribing_folder].iter().for_each(|folder| std::fs::remove_dir_all(folder).unwrap());
    }

    #[test]
    fn an_address_needs_a_port() {
        let (subscribing, folder) = manager();
        assert!(subscribing.subscribe("laptop", "k").is_err());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
