use nyx_fec::{NyxFec, DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE};

#[test]
fn rs_fec_encode_verify() {
    let codec = NyxFec::new();
    let mut shards: Vec<Vec<u8>> = (0..DATA_SHARDS)
        .map(|i| vec![i as u8; SHARD_SIZE])
        .collect();
    shards.extend((0..PARITY_SHARDS).map(|_| vec![0u8; SHARD_SIZE]));

    let mut mut_refs: Vec<&mut [u8]> = shards.iter_mut().map(|v| v.as_mut_slice()).collect();
    codec.encode(&mut mut_refs).unwrap();
    let verify_vec: Vec<&[u8]> = mut_refs.iter().map(|s| &**s).collect();
    assert!(codec.verify(&verify_vec).unwrap());
} 