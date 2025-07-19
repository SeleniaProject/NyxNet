use nyx_stream::congestion::CongestionCtrl;
use std::time::Duration;

#[test]
fn cwnd_tracks_bandwidth() {
    let mut cc = CongestionCtrl::new();
    // Simulate 10 ACKs at 100Mbps (~9765 packets/s)
    for _ in 0..10 {
        cc.on_send(1280);
        cc.on_ack(1280, Duration::from_micros(104)); // 100Mbps
    }
    assert!(cc.cwnd > 100.0);
} 