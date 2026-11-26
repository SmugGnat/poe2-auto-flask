//! The UI owns the lifetime of a single Rust automation thread.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use poe2_auto_flask::backend;
use poe2_auto_flask::poe1::skills::SkillId;
use poe2_auto_flask::runtime::{BackendCommand, BackendControl, BackendStatus, Connection};
use poe2_auto_flask::telemetry::{Pool, Reader, Snapshot};
use slint::ComponentHandle;

use crate::{AppWindow, DetectedSkills, Editor};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct SkillRequest {
    active: bool,
    revision: u64,
}

#[derive(Clone, Default)]
struct DisplayState {
    backend: Option<BackendStatus>,
    telemetry: Option<Snapshot>,
    skill_request: SkillRequest,
}

fn detected_skills(present: Option<&BTreeSet<SkillId>>) -> DetectedSkills {
    let Some(present) = present else {
        return DetectedSkills::default();
    };
    DetectedSkills {
        known: true,
        bladefall: present.contains(&SkillId::BladefallOfTrarthus),
        blood_rage: present.contains(&SkillId::BloodRage),
        convocation: present.contains(&SkillId::Convocation),
        focus: present.contains(&SkillId::Focus),
        immortal_call: present.contains(&SkillId::ImmortalCall),
        molten_shell: present.contains(&SkillId::MoltenShell),
        spirit_offering: present.contains(&SkillId::SpiritOffering),
        steelskin: present.contains(&SkillId::Steelskin),
    }
}

fn pool_text(pool: Option<Pool>) -> slint::SharedString {
    pool.map(|pool| {
        format!(
            "{}/{}",
            grouped_number(pool.current),
            grouped_number(pool.maximum)
        )
        .into()
    })
    .unwrap_or_else(|| "—".into())
}

fn grouped_number(value: i32) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut text = String::with_capacity(digits.len() + 4);
    if value < 0 {
        text.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

pub struct BackendHost {
    commands: SyncSender<BackendCommand>,
    completion: mpsc::Receiver<()>,
    worker: Option<JoinHandle<()>>,
    bridge: Option<JoinHandle<()>>,
}

impl BackendHost {
    pub fn start(
        options: backend::Options,
        window: &AppWindow,
        editor: &Rc<RefCell<Editor>>,
    ) -> io::Result<Self> {
        let (control, events, commands) = BackendControl::supervised();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("automation".into())
            .spawn(move || {
                let _ = backend::run(options, control);
                let _ = done_tx.send(());
            })?;
        let weak_ui = window.as_weak();
        let latest = Arc::new(Mutex::new(DisplayState::default()));
        let visible = Arc::new(AtomicBool::new(true));
        let visibility = visible.clone();
        let wake = commands.clone();
        let visibility_state = latest.clone();
        window.on_presentation_changed(move |active| {
            visibility.store(active, Ordering::Release);
            // Never show a pre-minimize sample as if it were live.
            if !active {
                visibility_state.lock().unwrap().telemetry = None;
            }
            let _ = wake.try_send(BackendCommand::RequestStatus);
        });
        let skill_state = latest.clone();
        let skill_wake = commands.clone();
        let skill_ui = window.as_weak();
        window.on_skill_detection_changed(move |active| {
            {
                let mut state = skill_state.lock().unwrap();
                state.skill_request.active = active;
                state.skill_request.revision = state.skill_request.revision.wrapping_add(1);
                if let Some(sample) = &mut state.telemetry {
                    sample.present_skills = None;
                }
            }
            if active {
                if let Some(ui) = skill_ui.upgrade() {
                    ui.set_detected_skills(DetectedSkills::default());
                }
                let _ = skill_wake.try_send(BackendCommand::RequestStatus);
            }
        });
        let state = latest.clone();
        let ui_editor = editor.clone();
        let ui_weak = window.as_weak();
        window.on_refresh_backend(move || {
            if let Some(ui) = ui_weak.upgrade() {
                if !ui.get_presentation_active() {
                    return;
                }
                let state = state.lock().unwrap().clone();
                let mut editor = ui_editor.borrow_mut();
                let game_focused = state.backend.as_ref().is_some_and(|s| s.focused)
                    && !editor.backend_status.as_ref().is_some_and(|s| s.focused);
                editor.backend_status = state.backend;
                editor.update_backend_state(&ui);
                drop(editor);
                let sample = state.telemetry.as_ref();
                ui.set_detected_skills(detected_skills(
                    sample.and_then(|s| s.present_skills.as_ref()),
                ));
                ui.set_live_life(pool_text(sample.map(|s| s.life)));
                ui.set_live_mana(pool_text(sample.and_then(|s| s.mana)));
                ui.set_live_shield(pool_text(sample.and_then(|s| s.shield)));
                ui.set_live_character_name(
                    sample
                        .and_then(|s| s.character_name.as_deref())
                        .unwrap_or("")
                        .into(),
                );
                ui.set_live_area(
                    sample
                        .map(|s| s.area.as_str())
                        .unwrap_or("Waiting for game state")
                        .into(),
                );
                ui.set_live_area_detail(slint::StyledText::from_plain_text(&ui.get_live_area()));
                if game_focused {
                    ui.invoke_game_window_focused();
                }
            }
        });
        let select = commands.clone();
        window.on_choose_running_game(move |index| {
            let game = if index == 0 {
                poe2_auto_flask::game::Game::Poe1
            } else {
                poe2_auto_flask::game::Game::Poe2
            };
            let _ = select.try_send(BackendCommand::SelectGame(game));
        });
        let under_wine = poe2_auto_flask::platform::running_under_wine();
        let update_pending = Arc::new(AtomicBool::new(false));
        let bridge =
            match thread::Builder::new()
                .name("backend-status".into())
                .spawn(move || {
                    let mut status: Option<BackendStatus> = None;
                    let mut reader = None;
                    let mut target = None;
                    let mut next_sample = Instant::now();
                    let mut last_skill_request = SkillRequest::default();
                    loop {
                        let active = visible.load(Ordering::Acquire);
                        let sample_due = active && target.is_some();
                        let event = if sample_due {
                            events
                                .recv_timeout(next_sample.saturating_duration_since(Instant::now()))
                        } else {
                            events
                                .recv()
                                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                        };
                        let mut changed = false;
                        match event {
                            Ok(state) => {
                                let next_target = if state.connection == Connection::Connected {
                                    state.game.zip(state.process_id)
                                } else {
                                    None
                                };
                                let mut display = latest.lock().unwrap();
                                changed = display.backend.as_ref() != Some(&state);
                                display.backend = Some(state.clone());
                                if next_target != target {
                                    target = next_target;
                                    reader = None;
                                    display.telemetry = None;
                                    next_sample = Instant::now();
                                }
                                status = Some(state);
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                        // This bridge owns its own read-only session. The automation
                        // thread never waits for a presentation read or render.
                        let skill_request = latest.lock().unwrap().skill_request;
                        if skill_request != last_skill_request {
                            last_skill_request = skill_request;
                            if skill_request.active {
                                next_sample = Instant::now();
                            }
                        }
                        if visible.load(Ordering::Acquire) && Instant::now() >= next_sample {
                            if let Some((game, pid)) = target {
                                if reader.is_none() {
                                    reader = Reader::open(game, pid).ok();
                                }
                                let mut sample = reader
                                    .as_mut()
                                    .and_then(|r| r.sample(skill_request.active).ok());
                                let retry = if sample.is_some() {
                                    500
                                } else if reader.is_some() {
                                    1500
                                } else {
                                    5000
                                };
                                next_sample = Instant::now() + Duration::from_millis(retry);
                                let mut display = latest.lock().unwrap();
                                // A scan started before leaving or reopening Skills
                                // cannot supply the result for the new view.
                                if skill_request != display.skill_request {
                                    if let Some(sample) = &mut sample {
                                        sample.present_skills = None;
                                    }
                                }
                                let sample = if visible.load(Ordering::Acquire) {
                                    sample
                                } else {
                                    None
                                };
                                changed |= display.telemetry != sample;
                                display.telemetry = sample;
                            }
                        }
                        let finished = under_wine
                            && status
                                .as_ref()
                                .is_some_and(|s| s.connection == Connection::Exited);
                        if !(finished || changed && visible.load(Ordering::Acquire)) {
                            continue;
                        }
                        if update_pending.swap(true, Ordering::AcqRel) {
                            continue;
                        }
                        let weak_ui = weak_ui.clone();
                        let ui_latest = latest.clone();
                        let ui_pending = update_pending.clone();
                        if slint::invoke_from_event_loop(move || {
                            ui_pending.store(false, Ordering::Release);
                            if let Some(ui) = weak_ui.upgrade() {
                                if ui.get_presentation_active() {
                                    ui.invoke_refresh_backend();
                                }
                                let finished =
                                    ui_latest.lock().unwrap().backend.as_ref().is_some_and(
                                        |state| {
                                            state.connection
                                                == poe2_auto_flask::runtime::Connection::Exited
                                        },
                                    );
                                if finished && under_wine {
                                    let _ = slint::quit_event_loop();
                                }
                            }
                        })
                        .is_err()
                        {
                            break;
                        }
                    }
                }) {
                Ok(bridge) => bridge,
                Err(error) => {
                    let _ = commands.send(BackendCommand::Shutdown);
                    let _ = worker.join();
                    return Err(error);
                }
            };
        Ok(Self {
            commands,
            completion: done_rx,
            worker: Some(worker),
            bridge: Some(bridge),
        })
    }

    pub fn shutdown(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        // Restore requests may briefly fill the queue. Explicit Quit must
        // still deliver shutdown before waiting for the worker.
        let _ = self.commands.send(BackendCommand::Shutdown);
        if self.completion.recv_timeout(Duration::from_secs(2)).is_ok() {
            let _ = worker.join();
            if let Some(bridge) = self.bridge.take() {
                let _ = bridge.join();
            }
        }
    }
}

impl Drop for BackendHost {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readouts_group_large_values_and_keep_unknown_distinct_from_zero() {
        for (value, expected) in [
            (0, "0"),
            (999, "999"),
            (1000, "1,000"),
            (4347, "4,347"),
            (10_000_000, "10,000,000"),
            (i32::MIN, "-2,147,483,648"),
        ] {
            assert_eq!(grouped_number(value), expected);
        }
        assert_eq!(pool_text(None).as_str(), "—");
        assert_eq!(
            pool_text(Some(Pool {
                current: 0,
                maximum: 0
            }))
            .as_str(),
            "0/0"
        );
        assert_eq!(
            pool_text(Some(Pool {
                current: 4364,
                maximum: 4364
            }))
            .as_str(),
            "4,364/4,364"
        );
    }

    #[test]
    fn explicit_quit_is_delivered_when_restore_requests_fill_the_queue() {
        let (commands, receiver) = mpsc::sync_channel(8);
        for _ in 0..8 {
            commands.send(BackendCommand::RequestStatus).unwrap();
        }
        let (done_tx, completion) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        let worker = thread::spawn(move || {
            // Model a worker finishing a read before it drains UI commands.
            thread::sleep(Duration::from_millis(10));
            while let Ok(command) = receiver.recv() {
                if matches!(command, BackendCommand::Shutdown) {
                    worker_stopped.store(true, Ordering::Release);
                    let _ = done_tx.send(());
                    break;
                }
            }
        });
        let mut host = BackendHost {
            commands,
            completion,
            worker: Some(worker),
            bridge: None,
        };
        host.shutdown();
        assert!(stopped.load(Ordering::Acquire));
    }
}
