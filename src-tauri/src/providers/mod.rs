//! Provider abstraction. The rest of the app only ever sees `dyn UsageProvider`.

pub mod command_code;

use crate::usage::models::{ProviderError, ProviderId, UsageSnapshot};
use async_trait::async_trait;

#[async_trait]
pub trait UsageProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn name(&self) -> &'static str;

    /// Fetches a fresh snapshot. Implementations must never include
    /// credentials in any error string or log line.
    async fn get_usage(&self) -> Result<UsageSnapshot, ProviderError>;
}

/// All providers compiled into this build, in display order.
pub fn all(http: reqwest::Client) -> Vec<Box<dyn UsageProvider>> {
    vec![Box::new(command_code::CommandCodeProvider::new(http))]
}
