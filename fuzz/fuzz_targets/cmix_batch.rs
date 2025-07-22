#![no_main]

use libfuzzer_sys::fuzz_target;
use nyx_mix::cmix::{CmixController, verify_batch};
use tokio::{runtime::Runtime, time::{timeout, Duration}};

// Fuzz target: feed arbitrary packets into a cMix controller and verify that
// the batch processing + proof verification never panics.
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        // Zero-length input is uninteresting for this target.
        return;
    }

    // Create a lightweight current-thread Tokio runtime for async operations.
    let rt = Runtime::new().expect("failed to create Tokio runtime");
    rt.block_on(async {
        // Use tiny batch/latency to keep fuzz iterations fast.
        let mut ctrl = CmixController::new(4, 1);
        // Try to send the raw input as a single packet. Errors are ignored –
        // we only care about catching panics or UB inside Nyx code.
        let _ = ctrl.sender().send(data.to_vec()).await;

        // Drain at most one batch with a small timeout to avoid prolonged
        // execution. If a batch arrives, attempt full cryptographic
        // verification. Any internal panic will be surfaced to libFuzzer.
        if let Ok(Some(batch)) = timeout(Duration::from_millis(5), ctrl.recv()).await {
            let _ = verify_batch(&batch, ctrl.params(), None);
        }
    });
}); 