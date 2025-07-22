use nyx_mix::{cmix::{CmixController, verify_batch}, vdf};
use tokio::time::Duration;

#[tokio::test]
async fn cmix_batch_verification() {
    let mut controller = CmixController::new(5, 20); // small batch & short delay for test
    let tx = controller.sender();
    // inject dummy packets
    for i in 0u8..3 {
        tx.send(vec![i]).await.unwrap();
    }
    // receive batch within 100ms
    let batch = tokio::time::timeout(Duration::from_millis(200), controller.recv())
        .await
        .expect("batch timeout")
        .expect("controller closed");
    assert!(verify_batch(&batch, controller.params(), None));
} 