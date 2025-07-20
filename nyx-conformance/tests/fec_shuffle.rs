use nyx_fec::{NyxFec, DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE};

#[test]
fn fec_shuffle_verify() {
    let codec = NyxFec::new();
    let mut shards: Vec<Vec<u8>> = (0..DATA_SHARDS).map(|i| vec![i as u8; SHARD_SIZE]).collect();
    shards.extend((0..PARITY_SHARDS).map(|_| vec![0u8; SHARD_SIZE]));
    let mut refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
    codec.encode(&mut refs).unwrap();
    // shuffle
    refs.swap(0, DATA_SHARDS);
    let verify_vec: Vec<&[u8]> = refs.iter().map(|r| &**r).collect();
    assert!(codec.verify(&verify_vec).unwrap());
} 