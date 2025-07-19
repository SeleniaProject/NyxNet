#![forbid(unsafe_code)]

//! Timing obfuscator queue.
//! Adds ±sigma randomized delay before releasing packets to mitigate timing analysis.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use tokio::{sync::mpsc, time::{sleep, Duration}};

/// Obfuscated packet with payload bytes.
#[derive(Debug)]
pub struct Packet(pub Vec<u8>);

/// TimingObfuscator parameters.
pub struct TimingConfig {
    /// Mean delay in milliseconds.
    pub mean_ms: f64,
    /// Standard deviation.
    pub sigma_ms: f64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self { mean_ms: 20.0, sigma_ms: 10.0 }
    }
}

/// Queue that releases packets after randomized delay.
pub struct TimingObfuscator {
    tx: mpsc::Sender<Packet>,
    rx: mpsc::Receiver<Packet>,
}

impl TimingObfuscator {
    pub fn new(config: TimingConfig) -> Self {
        let (int_tx, mut int_rx) = mpsc::channel::<Packet>(1024);
        let (out_tx, out_rx) = mpsc::channel::<Packet>(1024);
        // spawn worker
        tokio::spawn(async move {
            let mut rng = StdRng::from_entropy();
            while let Some(pkt) = int_rx.recv().await {
                // Sample delay: mean + N(0, sigma)
                let noise: f64 = rng.sample(rand_distr::Normal::new(0.0, config.sigma_ms).unwrap());
                let delay_ms = (config.mean_ms + noise).max(0.0);
                sleep(Duration::from_millis(delay_ms as u64)).await;
                if out_tx.send(pkt).await.is_err() {
                    break;
                }
            }
        });
        Self { tx: int_tx, rx: out_rx }
    }

    /// Enqueue packet for delayed release.
    pub async fn enqueue(&self, data: Vec<u8>) {
        let _ = self.tx.send(Packet(data)).await;
    }

    /// Receive next obfuscated packet.
    pub async fn recv(&mut self) -> Option<Packet> {
        self.rx.recv().await
    }
} 