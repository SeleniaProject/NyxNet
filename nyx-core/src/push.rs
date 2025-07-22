#![forbid(unsafe_code)]
//! Push gateway abstraction (FCM/APNS).

use async_trait::async_trait;
use tracing::info;

#[async_trait]
pub trait PushGateway: Send + Sync {
    async fn send_wakeup(&self, node_id: &str, payload: &[u8]) -> anyhow::Result<()>;
}

pub struct FcmGateway;
#[async_trait]
impl PushGateway for FcmGateway {
    async fn send_wakeup(&self, node_id: &str, _payload: &[u8]) -> anyhow::Result<()> {
        info!(node_id, "send FCM wakeup (stub)");
        Ok(())
    }
}

pub struct ApnsGateway;
#[async_trait]
impl PushGateway for ApnsGateway {
    async fn send_wakeup(&self, node_id: &str, _payload: &[u8]) -> anyhow::Result<()> {
        info!(node_id, "send APNS wakeup (stub)");
        Ok(())
    }
} 