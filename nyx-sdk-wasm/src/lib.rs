use wasm_bindgen::prelude::*;
use nyx_crypto::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, PushSubscriptionOptionsInit, ServiceWorkerRegistration, PushSubscription, PushManager};
use js_sys::{Uint8Array, JSON, Object, Reflect};
use base64::engine::{general_purpose, Engine};

#[wasm_bindgen]
pub fn noise_handshake_demo() -> String {
    // Simple demo performing Noise_Nyx X25519 handshake in wasm.
    let (init_pub, init_sec) = initiator_generate();
    let (resp_pub, shared_resp) = responder_process(&init_pub);
    let shared_init = initiator_finalize(init_sec, &resp_pub);
    assert_eq!(shared_init.as_bytes(), shared_resp.as_bytes());
    let key = derive_session_key(&shared_init);
    hex::encode(key.0)
}

/// Register a Service Worker at `sw_path` and subscribe to WebPush with the given VAPID public key.
/// Returns the JSON serialized subscription (to be sent to Nyx gateway).
///
/// This function is `async` in JS; use like:
/// `nyx_register_push("/nyx_sw.js", vapid_key).then(sub => { ... });`
#[wasm_bindgen]
pub async fn nyx_register_push(sw_path: String, vapid_public_key: String) -> Result<JsValue, JsValue> {
    let win = window().ok_or("no window")?;
    let navigator = win.navigator();
    let sw_container = navigator.service_worker();

    // Register service worker if not already controlling.
    let reg_promise = sw_container.register(&sw_path);
    let reg_js = JsFuture::from(reg_promise).await?;
    let reg: ServiceWorkerRegistration = reg_js.dyn_into()?;

    let push: PushManager = reg.push_manager()?;

    // Convert base64 public key to Uint8Array (assumes urlsafe base64 without padding).
    let key_buf = general_purpose::URL_SAFE_NO_PAD.decode(vapid_public_key.as_bytes()).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let key_u8 = Uint8Array::from(&key_buf[..]);
    let key_js: JsValue = key_u8.into();

    let sub_opts = PushSubscriptionOptionsInit::new();
    sub_opts.set_user_visible_only(true);
    sub_opts.set_application_server_key(&key_js);

    let sub_promise = push.subscribe_with_options(&sub_opts).map_err(|e| e)?;
    let sub_js = JsFuture::from(sub_promise).await?;
    let sub: PushSubscription = sub_js.dyn_into()?;

    // Simplified serialization - just return the endpoint for now
    let js_obj = Object::new();
    Reflect::set(&js_obj, &"endpoint".into(), &sub.endpoint().into())?;
    
    let json_str = JSON::stringify(&js_obj)?;
    Ok(json_str.into())
} 