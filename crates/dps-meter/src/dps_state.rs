use crate::encounter::Encounter;
use core::types::EntityId;
use game::GameEvent;
use std::time::{Duration, Instant};

const MAX_PAST_ENCOUNTERS: usize = 20;

pub struct DpsState {
    pub active:            Option<Encounter>,
    pub past:              Vec<Encounter>,
    pub encounter_timeout: Duration,
    last_combat:           Option<Instant>,
}

impl DpsState {
    pub fn new(encounter_timeout_secs: u32) -> Self {
        Self {
            active:            None,
            past:              Vec::new(),
            encounter_timeout: Duration::from_secs(encounter_timeout_secs as u64),
            last_combat:       None,
        }
    }

    pub fn apply_event<F>(&mut self, event: &GameEvent, name_resolver: F)
    where
        F: Fn(EntityId) -> String,
    {
        let GameEvent::Combat(combat) = event else {
            return;
        };

        let timed_out = self.last_combat
            .map(|t| t.elapsed() > self.encounter_timeout)
            .unwrap_or(false);

        if self.active.is_none() || timed_out {
            self.finish_active();
            self.active = Some(Encounter::new());
        }

        self.last_combat = Some(Instant::now());

        if let Some(enc) = &mut self.active {
            enc.apply(combat, |id| name_resolver(id));
        }
    }

    pub fn tick(&mut self) {
        if let Some(ref enc) = self.active {
            if let Some(last) = self.last_combat {
                if last.elapsed() > self.encounter_timeout && enc.is_active() {
                    self.finish_active();
                }
            }
        }
    }

    fn finish_active(&mut self) {
        if let Some(mut enc) = self.active.take() {
            enc.finish();
            if enc.total_damage > 0 {
                self.past.push(enc);
                if self.past.len() > MAX_PAST_ENCOUNTERS {
                    self.past.remove(0);
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.finish_active();
        self.last_combat = None;
    }
}
