//! Owned status values and bounded, nonblocking communication with the UI.
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::Duration;

use crate::config::{Config, ConfigManager};
use crate::game::Game;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Connection {
    #[default]
    Starting,
    Waiting,
    ChoosingGame,
    Validating,
    Connected,
    Exited,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppliedSettings {
    pub profile_id: Option<String>,
    pub config: Config,
    pub automation_key: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendStatus {
    pub connection: Connection,
    pub game: Option<Game>,
    pub process_id: Option<u32>,
    pub enabled: bool,
    pub focused: bool,
    pub reason: String,
    pub config_error: Option<String>,
    pub applied: Option<AppliedSettings>,
}

pub enum BackendCommand {
    Shutdown,
    SelectGame(Game),
    RequestStatus,
}

pub struct BackendControl {
    status: BackendStatus,
    events: Option<SyncSender<BackendStatus>>,
    commands: Option<Receiver<BackendCommand>>,
    pending: bool,
    stopping: bool,
    selected_game: Option<Game>,
}

impl BackendControl {
    pub fn standalone() -> Self {
        Self {
            status: BackendStatus::default(),
            events: None,
            commands: None,
            pending: false,
            stopping: false,
            selected_game: None,
        }
    }

    pub fn supervised() -> (Self, Receiver<BackendStatus>, SyncSender<BackendCommand>) {
        let (events_tx, events_rx) = mpsc::sync_channel(8);
        let (commands_tx, commands_rx) = mpsc::sync_channel(8);
        let control = Self {
            events: Some(events_tx),
            commands: Some(commands_rx),
            ..Self::standalone()
        };
        (control, events_rx, commands_tx)
    }

    pub fn is_supervised(&self) -> bool {
        self.events.is_some()
    }

    fn update(&mut self, change: impl FnOnce(&mut BackendStatus)) {
        let previous = self.status.clone();
        change(&mut self.status);
        self.pending |= previous != self.status;
        self.publish();
    }

    fn publish(&mut self) {
        if !self.pending {
            return;
        }
        if let Some(sender) = &self.events {
            match sender.try_send(self.status.clone()) {
                Ok(()) => self.pending = false,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => self.stopping = true,
            }
        } else {
            self.pending = false;
        }
    }

    pub fn connection(
        &mut self,
        connection: Connection,
        game: Option<Game>,
        reason: impl Into<String>,
    ) {
        let reason = reason.into();
        self.update(|state| {
            state.connection = connection;
            state.game = game;
            state.reason = reason;
            if connection != Connection::Connected {
                state.enabled = false;
                state.focused = false;
            }
            if matches!(
                connection,
                Connection::Waiting | Connection::Exited | Connection::Failed
            ) {
                state.process_id = None;
            }
        });
    }

    pub fn attached(&mut self, pid: u32) {
        self.update(|state| state.process_id = Some(pid));
    }

    pub fn automation(&mut self, enabled: bool, focused: bool, reason: &str) {
        if (
            self.status.enabled,
            self.status.focused,
            self.status.reason.as_str(),
        ) == (enabled, focused, reason)
        {
            return;
        }
        self.update(|state| {
            state.enabled = enabled;
            state.focused = focused;
            state.reason = reason.to_owned();
        });
    }

    pub fn applied(&mut self, manager: &ConfigManager) {
        self.update(|state| {
            state.applied = Some(AppliedSettings {
                profile_id: manager.active_profile_id().map(str::to_owned),
                config: manager.current().clone(),
                automation_key: manager.automation_key().label(),
            });
            state.config_error = None;
        });
    }

    pub fn config_error(&mut self, error: String) {
        self.update(|state| state.config_error = Some(error));
    }

    fn command(&mut self, command: BackendCommand) {
        match command {
            BackendCommand::Shutdown => self.stopping = true,
            BackendCommand::SelectGame(game) => self.selected_game = Some(game),
            BackendCommand::RequestStatus => self.pending = true,
        }
    }

    pub fn is_stopping(&mut self) -> bool {
        self.publish();
        loop {
            let command = self.commands.as_ref().map(Receiver::try_recv);
            match command {
                Some(Ok(command)) => self.command(command),
                Some(Err(TryRecvError::Disconnected)) => {
                    self.stopping = true;
                    break;
                }
                _ => break,
            }
        }
        self.stopping
    }

    pub fn wait(&mut self, duration: Duration) {
        if let Some(receiver) = &self.commands {
            match receiver.recv_timeout(duration) {
                Ok(command) => self.command(command),
                Err(mpsc::RecvTimeoutError::Disconnected) => self.stopping = true,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        } else {
            std::thread::sleep(duration);
        }
    }

    pub fn take_selected_game(&mut self) -> Option<Game> {
        self.selected_game.take()
    }

    /// Called after the engine has stopped. Deliver the final state even when
    /// the bounded channel was full during an automation tick.
    pub fn finish(&mut self, result: &std::io::Result<()>) {
        self.status.enabled = false;
        self.status.focused = false;
        self.status.process_id = None;
        if let Err(error) = result {
            self.status.connection = Connection::Failed;
            self.status.reason = error.to_string();
        } else {
            self.status.connection = Connection::Exited;
            self.status.reason = "Game disconnected".to_owned();
        }
        if let Some(sender) = &self.events {
            let _ = sender.send(self.status.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_status_channel_keeps_latest_state_and_shutdown_remains_available() {
        let (mut control, events, commands) = BackendControl::supervised();
        for i in 0..20 {
            control.connection(
                Connection::Waiting,
                Some(Game::Poe1),
                format!("Waiting {i}"),
            );
        }
        assert_eq!(events.try_iter().count(), 8);
        assert!(!control.is_stopping());
        assert_eq!(events.recv().unwrap().reason, "Waiting 19");
        commands.send(BackendCommand::Shutdown).unwrap();
        assert!(control.is_stopping());
    }

    #[test]
    fn losing_the_ui_stops_the_backend() {
        let (mut control, events, commands) = BackendControl::supervised();
        drop(events);
        drop(commands);
        assert!(control.is_stopping());
    }

    #[test]
    fn restored_ui_can_request_unchanged_status_and_disconnect_clears_pid() {
        let (mut control, events, commands) = BackendControl::supervised();
        control.attached(42);
        control.connection(Connection::Connected, Some(Game::Poe1), "DISABLED");
        let connected = events.try_iter().last().unwrap();
        assert_eq!(connected.process_id, Some(42));
        commands.send(BackendCommand::RequestStatus).unwrap();
        assert!(!control.is_stopping());
        assert!(!control.is_stopping());
        assert_eq!(events.recv().unwrap(), connected);
        control.connection(Connection::Waiting, Some(Game::Poe1), "Waiting");
        assert_eq!(events.recv().unwrap().process_id, None);
    }
}
