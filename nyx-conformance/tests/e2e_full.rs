#![allow(clippy::unwrap_used)]
use nyx_mix::cmix::{CmixController, verify_batch};
use nyx_mix::adaptive::AdaptiveCoverGenerator;
use nyx_stream::ReorderBuffer;
use nyx_fec::RaptorQCodec;
use tokio::time::{timeout, Duration};
use rand::{Rng, thread_rng};

/// Combined E2E scenario exercising each major subsystem (cMix, LowPower, Multipath, RaptorQ).
#[tokio::test]
async fn e2e_full_stack() {
    // --- cMix Batch & VDF Proof -----------------------------------------
    let mut cmix = CmixController::new(4, 5); // small delay for test speed
    let tx = cmix.sender();
    // inject dummy packets
    for i in 0u8..4 { tx.send(vec![i]).await.unwrap(); }
    let batch = timeout(Duration::from_millis(50), cmix.recv())
        .await
        .expect("cMix timeout")
        .expect("controller closed");
    assert!(verify_batch(&batch, cmix.params(), 1_000));

    // --- Low Power Adaptive Cover ---------------------------------------
    let mut cover = AdaptiveCoverGenerator::new(10.0, 0.3);
    // Simulate entering Low Power mode
    cover.set_low_power(true);
    let lambda_lp = cover.current_lambda();
    // next_delay call just to update internal state
    let _ = cover.next_delay();
    assert!(lambda_lp < 10.0 * 0.5, "λ should be reduced in low-power mode");

    // --- Multipath Reorder Buffer ---------------------------------------
    let mut rb = ReorderBuffer::new(0u64);
    // push packets out-of-order: 1,0,2
    let mut delivered = rb.push(1, 1);
    assert!(delivered.is_empty());
    delivered.extend(rb.push(0, 0));
    delivered.extend(rb.push(2, 2));
    // expect 0,1,2 in order
    assert_eq!(delivered, vec![0,1,2]);

    // --- RaptorQ Encode/Decode ------------------------------------------
    let codec = RaptorQCodec::new(0.3); // 30% redundancy
    let mut data = vec![0u8; 4096];
    thread_rng().fill(&mut data[..]);
    let packets = codec.encode(&data);
    // simulate loss: drop first two packets
    let recovered = codec.decode(&packets[2..]).expect("decode");
    assert_eq!(recovered, data);
} 