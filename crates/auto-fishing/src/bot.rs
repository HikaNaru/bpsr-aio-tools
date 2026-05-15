use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum FishingState {
    CheckingState,
    EnteringFishingMode,
    SelectingRod,
    Idle,
    Casting,
    WaitingBite { cast_at: Instant },
    Reeling { started_at: Instant },
    FishCaught,
    Cooldown { until: Instant },
}

impl FishingState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CheckingState          => "Checking...",
            Self::EnteringFishingMode    => "Entering fishing mode",
            Self::SelectingRod           => "Selecting rod",
            Self::Idle                   => "Idle",
            Self::Casting                => "Casting...",
            Self::WaitingBite { .. }     => "Waiting for bite",
            Self::Reeling { .. }         => "Reeling!",
            Self::FishCaught             => "Fish caught!",
            Self::Cooldown { .. }        => "Cooldown",
        }
    }
}
