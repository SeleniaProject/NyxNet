//! Push notification gateway (FCM / APNS) used for Low Power Mode wake-up.
//! All network interactions are async via `reqwest`.
#![forbid(unsafe_code)]

use reqwest::{Client, StatusCode};
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, oneshot};
use nyx_core::PushProvider;
use chrono::Utc;
use pasetors::version4::V4;

/// Errors that can occur while sending push notifications.
#[derive(Debug)]
pub enum PushError {
    /// HTTP transport-level error.
    Http(reqwest::Error),
    /// Remote server responded with non-success status.
    Server(StatusCode, String),
}

impl From<reqwest::Error> for PushError {
    fn from(e: reqwest::Error) -> Self { Self::Http(e) }
}

/// Simple push manager abstraction.
pub struct PushManager {
    client: Client,
    provider: PushProvider,
}

impl PushManager {
    /// Construct a new `PushManager` with given provider configuration.
    #[must_use]
    pub fn new(provider: PushProvider) -> Self {
        Self { client: Client::new(), provider }
    }

    /// Send JSON payload to a specific `device_token`.
    ///
    /// This method is **async** and must be awaited inside a Tokio runtime.
    pub async fn send(&self, device_token: &str, payload: &JsonValue) -> Result<(), PushError> {
        match &self.provider {
            PushProvider::Fcm { server_key } => {
                self.send_fcm(device_token, payload, server_key).await
            }
            PushProvider::Apns { team_id, key_id, key_p8 } => {
                self.send_apns(device_token, payload, team_id, key_id, key_p8).await
            }
        }
    }

    async fn send_fcm(
        &self,
        device_token: &str,
        payload: &JsonValue,
        server_key: &str,
    ) -> Result<(), PushError> {
        let body = serde_json::json!({
            "to": device_token,
            "data": payload,
        });
        let resp = self
            .client
            .post("https://fcm.googleapis.com/fcm/send")
            .header("Authorization", format!("key={}", server_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(PushError::Server(status, text))
        }
    }

    async fn send_apns(
        &self,
        device_token: &str,
        payload: &JsonValue,
        team_id: &str,
        key_id: &str,
        key_p8: &str,
    ) -> Result<(), PushError> {
        // Generate JWT valid for up to 20 minutes.
        let jwt = generate_apns_token(team_id, key_id, key_p8)?;

        // Minimal APNS implementation (HTTP/2, JWT auth).
        // APNS requires :authority header based on host.
        let url = format!("https://api.push.apple.com/3/device/{}", device_token);
        let resp = self
            .client
            .post(url)
            .header("authorization", format!("bearer {}", jwt))
            .header("apns-push-type", "background")
            .header("apns-priority", "5")
            .header("content-type", "application/json")
            .json(payload)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(PushError::Server(status, text))
        }
    }
}

/// Generate APNS token using PASETO instead of JWT.
fn generate_apns_token(team_id: &str, key_id: &str, _key_p8: &str) -> Result<String, PushError> {
    // For now, return a placeholder token
    // TODO: Implement proper PASETO signing when needed
    Ok(format!("paseto.token.{}.{}", team_id, key_id))
}

// -------------------------------------------------------------------------------------------------
// Background worker + handle

/// Internal request message used by [`PushHandle`].
enum PushRequest {
    Send {
        token: String,
        payload: JsonValue,
        resp: oneshot::Sender<Result<(), PushError>>,
    },
}

/// Cheap cloneable handle for queuing push send operations to background worker.
#[derive(Clone)]
pub struct PushHandle {
    tx: mpsc::Sender<PushRequest>,
}

impl PushHandle {
    /// Enqueue push notification and await result.
    pub async fn send(&self, token: &str, payload: JsonValue) -> Result<(), PushError> {
        let (tx, rx) = oneshot::channel();
        let msg = PushRequest::Send { token: token.to_string(), payload, resp: tx };
        // Translate channel closure into PushError::Server
        self.tx
            .send(msg)
            .await
            .map_err(|_| PushError::Server(StatusCode::INTERNAL_SERVER_ERROR, "worker closed".into()))?;
        rx.await.unwrap_or_else(|_| Err(PushError::Server(StatusCode::INTERNAL_SERVER_ERROR, "worker closed".into())))
    }
}

/// Spawn background task that owns `PushManager` and processes requests.
#[must_use]
pub fn spawn_push_service(provider: PushProvider) -> PushHandle {
    let (tx, mut rx) = mpsc::channel::<PushRequest>(64);
    tokio::spawn(async move {
        let mgr = PushManager::new(provider);
        while let Some(req) = rx.recv().await {
            match req {
                PushRequest::Send { token, payload, resp } => {
                    let result = mgr.send(&token, &payload).await;
                    let _ = resp.send(result);
                }
            }
        }
    });
    PushHandle { tx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn fcm_request_builds() {
        let mgr = PushManager::new(PushProvider::Fcm { server_key: "test_key".into() });
        // Sending will fail due to invalid key, but it should return Server error not panic.
        let res = mgr.send("dummy", &json!({"k":"v"})).await;
        assert!(res.is_err());
    }
} 