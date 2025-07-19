use nyx_stream::congestion::CongestionCtrl;
use std::time::Duration;

#[test]
fn bbrv2_cwnd_gain_cycle() {
    let mut cc = CongestionCtrl::new();
    // First ACK should increase cwnd roughly by gain 1.25
    cc.on_send(1280);
    cc.on_ack(1280, Duration::from_millis(100)); // delivery_rate = 10 pkts/s
    let cwnd1 = cc.available_window() + (cc.inflight as f64 / 1280.0);
    // Second ACK applies 0.75 gain -> cwnd should not grow compared to previous significantly
    cc.on_send(1280);
    cc.on_ack(1280, Duration::from_millis(100));
    let cwnd2 = cc.available_window() + (cc.inflight as f64 / 1280.0);
    assert!(cwnd1 > 10.0);
    assert!(cwnd2 < cwnd1 * 1.1); // should not overshoot
} 