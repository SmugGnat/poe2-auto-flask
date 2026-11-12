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
    skills: BTreeMap<SkillId, SkillReadiness>,
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
            skills: BTreeMap::new(),
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

    pub fn animation(
        &mut self,
        session: &mut GameSession,
        process: &Process,
    ) -> io::Result<i32> {
        if let Some(value) = self.animation {
            return Ok(value);
        }
        let value = session.poe1_animation(process)?;
        self.animation = Some(value);
        Ok(value)
    }

    pub fn skill(
        &mut self,
        session: &mut GameSession,
        process: &Process,
        id: SkillId,
    ) -> io::Result<SkillReadiness> {
        if let Some(value) = self.skills.get(&id) {
            return Ok(*value);
        }
        let value = session.poe1_skill_readiness(process, id)?;
        self.skills.insert(id, value);
        Ok(value)
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
    readiness: BTreeMap<SkillId, SkillReadiness>,
}

impl Preflight {
    pub fn sample(session: &mut GameSession, process: &Process) -> io::Result<Self> {
        Ok(Self {
            vitals: session.poe1_verify_vitals(process)?,
            chat: session.poe1_chat_active(process)?,
            effects: session.poe1_verify_effects(process)?,
            readiness: BTreeMap::new(),
        })
    }

    pub fn skill(
        &mut self,
        session: &mut GameSession,
        process: &Process,
        id: SkillId,
    ) -> io::Result<SkillReadiness> {
        if let Some(value) = self.readiness.get(&id) {
            return Ok(*value);
        }
        let value = session.poe1_verify_skill_readiness(process, id)?;
        self.readiness.insert(id, value);
        Ok(value)
    }
}
