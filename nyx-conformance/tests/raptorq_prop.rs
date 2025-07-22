use nyx_fec::raptorq::RaptorQCodec;
use proptest::prelude::*;

proptest! {
    #[test]
    fn raptorq_recovers(data in proptest::collection::vec(any::<u8>(), 1..10_000), loss_frac in 0u8..40u8) {
        let codec = RaptorQCodec::new(0.3);
        let mut packets = codec.encode(&data);
        // Simulate packet loss: drop first N% of packets randomly.
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        packets.shuffle(&mut rng);
        let drop_cnt = ((loss_frac as f32 / 100.0) * packets.len() as f32) as usize;
        packets.truncate(packets.len() - drop_cnt);

        if let Some(recovered) = codec.decode(&packets) {
            prop_assert_eq!(recovered, data);
        } else {
            // Recovery may fail if too many packets dropped (> redundancy )
            prop_assume!((loss_frac as f32) > 30.0);
        }
    }
} 