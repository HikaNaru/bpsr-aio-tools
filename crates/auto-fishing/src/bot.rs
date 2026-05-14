use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum FishingState {
    Idle,
    Casting,
    WaitingBite { cast_at: Instant },
    Reeling,
    Cooldown { until: Instant },
}

impl FishingState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle           => "Idle",
            Self::Casting        => "Casting...",
            Self::WaitingBite { .. } => "Waiting for bite",
            Self::Reeling            => "Reeling!",
            Self::Cooldown { .. }    => "Cooldown",
        }
    }
}
