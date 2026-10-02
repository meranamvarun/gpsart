use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

pub mod templates;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityProfile {
    Walking,
    Running,
    Cycling,
}

impl ActivityProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Walking => "walking",
            Self::Running => "running",
            Self::Cycling => "cycling",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metres(u32);

impl Metres {
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::ZeroDistance);
        }
        Ok(Self(value))
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlanningRequestId(Uuid);

impl PlanningRequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlanningRequestId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for PlanningRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DomainError {
    #[error("distance must be greater than zero metres")]
    ZeroDistance,
}
