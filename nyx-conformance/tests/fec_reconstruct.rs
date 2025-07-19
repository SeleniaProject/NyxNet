use nyx_fec::{NyxFec, DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE};

#[test]
fn rs_fec_reconstruct_two_losses() {
    let codec = NyxFec::new();
    let mut shards: Vec<Vec<u8>> = (0..DATA_SHARDS)
        .map(|i| vec![i as u8; SHARD_SIZE])
        .collect();
    shards.extend((0..PARITY_SHARDS).map(|_| vec![0u8; SHARD_SIZE]));

    let mut mut_refs: Vec<&mut [u8]> = shards.iter_mut().map(|v| v.as_mut_slice()).collect();
    codec.encode(&mut mut_refs).unwrap();
    let mut present: Vec<bool> = vec![true; DATA_SHARDS + PARITY_SHARDS];
    // Simulate loss of two data shards (index 2, 7)
    mut_refs[2].fill(0);
    mut_refs[7].fill(0);
    present[2] = false;
    present[7] = false;

    codec.reconstruct(&mut mut_refs, &mut present).unwrap();
    assert!(present.iter().all(|&b| b));
} 