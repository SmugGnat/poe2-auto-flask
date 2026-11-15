//! A single coherent, demand-driven PoE1 decision-tick state.
//! Readers populate values once; flask and skill policies consume those
//! validated observations without reading or interpreting game memory.
use std::collections::BTreeMap;
use std::io;

use crate::game_state::LiveVitals;
use crate::process::Process;
use crate::session::GameSession;

use super::buffs::EffectsSnapshot;
use super::flasks::FlaskSnapshot;
use super::nearby::NearbySnapshot;
use super::skills::{SkillId, SkillReadiness};

pub struct TickState {
    pub vitals: LiveVitals,
    pub area_allowed: bool,
    chat: Option<bool>,
    effects: Option<EffectsSnapshot>,
    flasks: Option<FlaskSnapshot>,
    animation: Option<i32>,
    skills: Option<BTreeMap<SkillId, SkillReadiness>>,
    nearby: Option<NearbySnapshot>,
}

impl TickState {
    pub fn sample(
        session: &mut GameSession,
        process: &Process,
        allow_hideout: bool,
    ) -> io::Result<Self> {
        let vitals = session.vitals(process)?;
        let area_allowed = session.poe1_area(process)?.allows_auto_flask(allow_hideout);
        Ok(Self {
            vitals,
            area_allowed,
            chat: None,
            effects: None,
            flasks: None,
            animation: None,
            skills: None,
            nearby: None,
        })
    }

    pub fn chat(&mut self, session: &GameSession, process: &Process) -> io::Result<bool> {
        if let Some(chat) = self.chat {
            return Ok(chat);
        }
        let chat = session.poe1_chat_active(process)?;
        self.chat = Some(chat);
        Ok(chat)
    }

    pub fn effects<'a>(
        &'a mut self,
        session: &mut GameSession,
        process: &Process,
    ) -> io::Result<&'a EffectsSnapshot> {
        if self.effects.is_none() {
            self.effects = Some(session.poe1_effects(process)?.clone());
        }
        Ok(self.effects.as_ref().expect("effects were sampled"))
    }

    pub fn flasks<'a>(
        &'a mut self,
        session: &mut GameSession,
        process: &Process,
    ) -> io::Result<&'a FlaskSnapshot> {
        if self.flasks.is_none() {
            self.flasks = Some(session.poe1_flasks(process)?.clone());
        }
        Ok(self.flasks.as_ref().expect("flasks were sampled"))
    }

    pub fn animation(&mut self, session: &mut GameSession, process: &Process) -> io::Result<i32> {
        if let Some(value) = self.animation {
            return Ok(value);
        }
        let value = session.poe1_animation(process)?;
        self.animation = Some(value);
        Ok(value)
    }

    /// One bulk Actor snapshot for all rules passing cheap conditions.
    /// Additional requested skill IDs in the same tick are rejected rather
    /// than silently returning a partial or stale status.
    pub fn skills(
        &mut self,
        session: &mut GameSession,
        process: &Process,
        requested: &[SkillId],
    ) -> io::Result<&BTreeMap<SkillId, SkillReadiness>> {
        if requested.is_empty() {
            return Ok(self.skills.get_or_insert_with(BTreeMap::new));
        }
        if self.skills.as_ref().is_none_or(|value| value.is_empty()) {
            self.skills = Some(session.poe1_skills(process, requested)?);
        } else if self
            .skills
            .as_ref()
            .is_some_and(|value| requested.iter().any(|id| !value.contains_key(id)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "skills requested after Actor snapshot",
            ));
        }
        Ok(self.skills.as_ref().expect("skills sampled"))
    }

    /// Optional census already sampled within this tick. Never initiates a
    /// scan simply because a rule is being revalidated before dispatch.
    pub fn observed_nearby(&self) -> Option<&NearbySnapshot> {
        self.nearby.as_ref()
    }

    pub fn nearby(
        &mut self,
        session: &mut GameSession,
        process: &Process,
    ) -> io::Result<&NearbySnapshot> {
        if self.nearby.is_none() {
            self.nearby = Some(session.poe1_nearby(process)?);
        }
        Ok(self.nearby.as_ref().expect("nearby was sampled"))
    }
}

/// One final shared pre-dispatch observation. It is separate from the 100ms
/// decision snapshot because a window or game transition can invalidate
/// state while multiple action candidates are being considered.
pub struct Preflight {
    pub vitals: LiveVitals,
    pub chat: bool,
    pub effects: EffectsSnapshot,
    readiness: Option<BTreeMap<SkillId, SkillReadiness>>,
}

impl Preflight {
    pub fn sample(session: &mut GameSession, process: &Process) -> io::Result<Self> {
        Ok(Self {
            vitals: session.poe1_verify_vitals(process)?,
            chat: session.poe1_chat_active(process)?,
            effects: session.poe1_verify_effects(process)?,
            readiness: None,
        })
    }

    /// One fresh Actor skill scan for all selected skill action candidates.
    pub fn skills(
        &mut self,
        session: &mut GameSession,
        process: &Process,
        requested: &[SkillId],
    ) -> io::Result<&BTreeMap<SkillId, SkillReadiness>> {
        if requested.is_empty() {
            return Ok(self.readiness.get_or_insert_with(BTreeMap::new));
        }
        if self.readiness.as_ref().is_none_or(|value| value.is_empty()) {
            self.readiness = Some(session.poe1_verify_skills(process, requested)?);
        } else if self
            .readiness
            .as_ref()
            .is_some_and(|value| requested.iter().any(|id| !value.contains_key(id)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "skills requested after preflight scan",
            ));
        }
        Ok(self.readiness.as_ref().expect("preflight skills sampled"))
    }
}
