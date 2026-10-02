use async_trait::async_trait;
use gpsart_domain::{ActivityProfile, Metres};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteQuery {
    pub activity: ActivityProfile,
    pub target_distance: Metres,
}

#[async_trait]
pub trait RouteProvider: Send + Sync {
    type Output: Send;
    async fn route(&self, query: &RouteQuery) -> Result<Self::Output, RoutingError>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RoutingError {
    #[error("routing provider is unavailable")]
    Unavailable,
    #[error("routing capability is unsupported")]
    Unsupported,
}
