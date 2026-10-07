//! Keeping each deployment's strategist as legion.json says: started
//! while it's enabled and the commander runs, stopped when it's turned off,
//! and asked for a speed check on its timer whenever it's idle and there's
//! work to look at.

use std::sync::Arc;

use legion2_proto::{Activity, EntryKind, MissionStatus, NewEntry, COMMANDER, HUMAN, LEGION, STRATEGIST};

use crate::{
    constants::STRATEGIST_RESTART_GAP_MS,
    daemon::Daemon,
    prompts::strategist_check_request,
    session_commands::is_busy,
    setup::{read_settings, strategist_check_minutes},
    store::now_ms,
};

const MILLISECONDS_PER_MINUTE: i64 = 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategistAction {
    Start,
    Stop,
    Check,
    Wait,
}

/// One deployment's strategist, as the timer sees it.
pub struct StrategistView {
    /// How often it checks, or None when it's off.
    pub every_minutes: Option<u32>,
    pub is_running: bool,
    pub is_idle: bool,
    pub last_started_ms: Option<i64>,
    pub last_checked_ms: Option<i64>,
    /// Someone else is working, or a mission waits or is under way. With
    /// nothing going on there's nothing to speed up, so it isn't woken.
    pub has_work: bool,
}

/// Mission statuses with work still to do. Blocked and paused ones wait on
/// people, and done ones only on the commander's finish.
const ACTIVE_MISSION_STATUSES: &[MissionStatus] = &[MissionStatus::Waiting, MissionStatus::Started, MissionStatus::HandedOff];

pub fn has_work_to_look_at(other_sessions_busy: bool, mission_statuses: &[MissionStatus]) -> bool {
    other_sessions_busy || mission_statuses.iter().any(|status| ACTIVE_MISSION_STATUSES.contains(status))
}

pub fn strategist_action(view: &StrategistView, now: i64) -> StrategistAction {
    let Some(every_minutes) = view.every_minutes else {
        return if view.is_running { StrategistAction::Stop } else { StrategistAction::Wait };
    };
    if !view.is_running {
        let may_start = view.last_started_ms.is_none_or(|started| now - started >= STRATEGIST_RESTART_GAP_MS);
        return if may_start { StrategistAction::Start } else { StrategistAction::Wait };
    }
    // The first check comes one interval after it starts.
    let since_ms = view.last_checked_ms.or(view.last_started_ms).unwrap_or(now);
    let is_due = now - since_ms >= i64::from(every_minutes) * MILLISECONDS_PER_MINUTE;
    if view.is_idle && is_due && view.has_work {
        StrategistAction::Check
    } else {
        StrategistAction::Wait
    }
}

impl Daemon {
    /// Runs on legion2d's regular tick, for each deployment whose commander is running.
    pub fn keep_strategists(self: &Arc<Self>) {
        let now = now_ms();
        let actions: Vec<(String, StrategistAction)> = {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            let commander_deployments: Vec<String> = state.sessions.values().filter(|session| session.position == COMMANDER).map(|session| session.deployment.clone()).collect();
            commander_deployments
                .into_iter()
                .filter_map(|deployment_id| {
                    let (folder_index, _) = state.find_open_deployment(&deployment_id).ok()?;
                    let folder = &state.folders[folder_index];
                    let settings = read_settings(&folder.path).inspect_err(|error| eprintln!("{LEGION}d: {error}")).ok()?;
                    let mission_statuses: Vec<MissionStatus> = folder.store.missions(&deployment_id).ok()?.into_iter().map(|mission| mission.status).collect();
                    let other_sessions_busy = state.sessions.values().any(|session| session.deployment == deployment_id && session.position != STRATEGIST && is_busy(session.activity));
                    let strategist_session = state.sessions.values().find(|session| session.deployment == deployment_id && session.position == STRATEGIST);
                    let view = StrategistView {
                        every_minutes: strategist_check_minutes(&settings),
                        is_running: strategist_session.is_some(),
                        is_idle: strategist_session.is_some_and(|session| session.activity == Activity::Idle),
                        last_started_ms: state.strategist_started_at.get(&deployment_id).copied(),
                        last_checked_ms: state.strategist_checked_at.get(&deployment_id).copied(),
                        has_work: has_work_to_look_at(other_sessions_busy, &mission_statuses),
                    };
                    let action = strategist_action(&view, now);
                    match action {
                        StrategistAction::Start => {
                            state.strategist_started_at.insert(deployment_id.clone(), now);
                            state.strategist_checked_at.remove(&deployment_id);
                        }
                        StrategistAction::Check => {
                            state.strategist_checked_at.insert(deployment_id.clone(), now);
                        }
                        StrategistAction::Stop | StrategistAction::Wait => {}
                    }
                    Some((deployment_id, action))
                })
                .collect()
        };
        actions.into_iter().for_each(|(deployment_id, action)| {
            let outcome = match action {
                StrategistAction::Start => self.start_session(&deployment_id, STRATEGIST, None, LEGION, None).map(|_| ()),
                StrategistAction::Stop => self.stop_session(&deployment_id, STRATEGIST).map(|_| ()),
                StrategistAction::Check => {
                    let request = NewEntry { kind: EntryKind::Message, mission: None, to: Some(STRATEGIST.into()), text: strategist_check_request(), answers: None };
                    self.post_entry(&deployment_id, LEGION, request).map(|_| ())
                }
                StrategistAction::Wait => Ok(()),
            };
            if let Err(error) = outcome {
                let text = format!("the strategist: {error}");
                self.post_legion_entry(&deployment_id, NewEntry { kind: EntryKind::Note, mission: None, to: Some(HUMAN.into()), text, answers: None });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = MILLISECONDS_PER_MINUTE;

    fn view(every_minutes: Option<u32>, is_running: bool, is_idle: bool) -> StrategistView {
        StrategistView { every_minutes, is_running, is_idle, last_started_ms: None, last_checked_ms: None, has_work: true }
    }

    #[test]
    fn it_isnt_woken_when_nothing_is_going_on() {
        let quiet = StrategistView { last_started_ms: Some(0), has_work: false, ..view(Some(1), true, true) };
        assert_eq!(strategist_action(&quiet, 10 * MINUTE), StrategistAction::Wait);
        assert_eq!(strategist_action(&StrategistView { has_work: true, ..quiet }, 10 * MINUTE), StrategistAction::Check);
    }

    #[test]
    fn work_is_someone_busy_or_a_mission_still_moving() {
        assert!(has_work_to_look_at(true, &[]));
        assert!(has_work_to_look_at(false, &[MissionStatus::Done, MissionStatus::HandedOff]));
        assert!(!has_work_to_look_at(false, &[MissionStatus::Done, MissionStatus::Blocked, MissionStatus::Paused]));
    }

    #[test]
    fn it_starts_when_enabled_and_stops_when_turned_off() {
        assert_eq!(strategist_action(&view(Some(1), false, false), 0), StrategistAction::Start);
        assert_eq!(strategist_action(&view(None, true, true), 0), StrategistAction::Stop);
        assert_eq!(strategist_action(&view(None, false, false), 0), StrategistAction::Wait);
    }

    #[test]
    fn one_that_keeps_ending_waits_before_starting_again() {
        let ended = StrategistView { last_started_ms: Some(0), ..view(Some(1), false, false) };
        assert_eq!(strategist_action(&ended, STRATEGIST_RESTART_GAP_MS - 1), StrategistAction::Wait);
        assert_eq!(strategist_action(&ended, STRATEGIST_RESTART_GAP_MS), StrategistAction::Start);
    }

    #[test]
    fn it_checks_on_its_timer_only_when_idle() {
        let started = StrategistView { last_started_ms: Some(0), ..view(Some(2), true, true) };
        assert_eq!(strategist_action(&started, 2 * MINUTE - 1), StrategistAction::Wait);
        assert_eq!(strategist_action(&started, 2 * MINUTE), StrategistAction::Check);
        let busy = StrategistView { is_idle: false, ..started };
        assert_eq!(strategist_action(&busy, 5 * MINUTE), StrategistAction::Wait);
        let checked = StrategistView { last_checked_ms: Some(2 * MINUTE), ..view(Some(2), true, true) };
        assert_eq!(strategist_action(&checked, 3 * MINUTE), StrategistAction::Wait);
        assert_eq!(strategist_action(&checked, 4 * MINUTE), StrategistAction::Check);
    }
}
