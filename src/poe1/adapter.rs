//! PoE1-specific game-state adapter. Owns PoE1 pointers, specialized
//! snapshots and demand-driven reader caches, independently of PoE2.
use std::io;

use crate::area::AreaInfo;
use crate::game_state::{GameStateReader, LiveVitals};
use crate::process::Process;

use super::buffs::{EffectCache, EffectsSnapshot};
use super::flasks::{FlaskCache, FlaskSnapshot};
use super::movement::MotionCache;
use super::nearby::{NearbyCache, NearbySnapshot};
use super::Probe;

#[derive(Default)]
struct TickSnapshot {
    area: Option<AreaInfo>,
    flasks: Option<FlaskSnapshot>,
    effects: Option<EffectsSnapshot>,
    animation: Option<i32>,
}

pub struct Adapter {
    probe: Probe,
    tick: TickSnapshot,
    flask_cache: FlaskCache,
    effect_cache: EffectCache,
    motion_cache: MotionCache,
    nearby_cache: NearbyCache,
}

impl Adapter {
    pub fn probe(process: &Process) -> io::Result<Self> {
        Ok(Self {
            probe: super::probe(process)?,
            tick: TickSnapshot::default(),
            flask_cache: FlaskCache::default(),
            effect_cache: EffectCache::default(),
            motion_cache: MotionCache::default(),
            nearby_cache: NearbyCache::default(),
        })
    }

    pub fn begin_tick(&mut self) {
        self.tick = TickSnapshot::default();
    }

    pub fn player_metadata(&self) -> &str {
        &self.probe.metadata
    }

    pub fn verify_vitals(&self, process: &Process) -> io::Result<LiveVitals> {
        super::sample_vitals(process, &self.probe)
    }

    pub fn chat_active(&self, process: &Process) -> io::Result<bool> {
        super::chat::is_chat_active(process, &self.probe)
    }

    pub fn animation(&mut self, process: &Process) -> io::Result<i32> {
        if let Some(value) = self.tick.animation {
            return Ok(value);
        }
        let value = self.motion_cache.sample(process, &self.probe)?;
        self.tick.animation = Some(value);
        Ok(value)
    }

    pub fn verify_animation(&mut self, process: &Process) -> io::Result<i32> {
        self.motion_cache.sample(process, &self.probe)
    }

    pub fn nearby(&mut self, process: &Process) -> io::Result<NearbySnapshot> {
        self.nearby_cache.sample(process, &self.probe)
    }

    pub fn area(&mut self, process: &Process) -> io::Result<&AreaInfo> {
        if self.tick.area.is_none() {
            self.tick.area = Some(super::area::read(process, &self.probe)?);
        }
        Ok(self.tick.area.as_ref().expect("PoE1 area just sampled"))
    }

    pub fn flasks(&mut self, process: &Process) -> io::Result<&FlaskSnapshot> {
        if self.tick.flasks.is_none() {
            self.tick.flasks = Some(super::flasks::sample(
                process,
                &self.probe,
                &mut self.flask_cache,
            )?);
        }
        Ok(self.tick.flasks.as_ref().expect("PoE1 flasks just sampled"))
    }

    pub fn effects(&mut self, process: &Process) -> io::Result<&EffectsSnapshot> {
        if self.tick.effects.is_none() {
            self.tick.effects = Some(super::buffs::sample(
                process,
                &self.probe,
                &mut self.effect_cache,
            )?);
        }
        Ok(self
            .tick
            .effects
            .as_ref()
            .expect("PoE1 effects just sampled"))
    }
}

impl GameStateReader for Adapter {
    fn refresh(&mut self, process: &Process) -> io::Result<()> {
        // Invalidate before attempting reconnection; never reuse stale samples
        // or cached component pointers if the area or player changed.
        self.begin_tick();
        self.flask_cache.clear();
        self.effect_cache.clear();
        self.motion_cache.clear();
        self.nearby_cache.clear();
        self.probe = super::refresh(process, &self.probe)?;
        Ok(())
    }

    fn read_vitals(&self, process: &Process) -> io::Result<LiveVitals> {
        super::sample_vitals(process, &self.probe)
    }
}
