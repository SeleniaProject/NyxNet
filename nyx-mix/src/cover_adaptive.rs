//! Adaptive cover traffic controller reacting to power state changes.
#![forbid(unsafe_code)]

use crate::cover::CoverGenerator;
use nyx_core::mobile::{subscribe_power_events, MobilePowerState};
use tokio_stream::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared controller reference.
pub type AdaptiveCover = Arc<RwLock<AdaptiveCoverController>>;

/// Controller that dynamically scales cover traffic rate (λ) based on device power state.
pub struct AdaptiveCoverController {
    generator: CoverGenerator,
    base_lambda: f64,
}

impl AdaptiveCoverController {
    /// Spawn controller task and return shared handle.
    pub fn spawn(base_lambda: f64) -> AdaptiveCover {
        let gen = CoverGenerator::new(base_lambda);
        let ctl = Arc::new(RwLock::new(Self { generator: gen, base_lambda }));
        let ctl_clone = ctl.clone();
        tokio::spawn(async move {
            let mut stream = subscribe_power_events();
            while let Some(state) = stream.next().await {
                let factor = match state {
                    MobilePowerState::Foreground => 1.0,
                    MobilePowerState::Charging => 0.8,
                    MobilePowerState::Discharging => 0.5,
                    MobilePowerState::ScreenOff => 0.3,
                };
                let mut guard = ctl_clone.write().await;
                guard.generator.lambda = guard.base_lambda * factor;
            }
        });
        ctl
    }

    /// Get current generator.
    pub async fn generator(&self) -> CoverGenerator { self.generator }
} 