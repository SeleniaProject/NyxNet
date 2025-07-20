use nyx_crypto::kdf::{hkdf_expand, KdfLabel};

#[test]
fn hkdf_label_separation() {
    let ikm = b"test-ikm";
    let session = hkdf_expand(ikm, KdfLabel::Session, 32);
    let rekey = hkdf_expand(ikm, KdfLabel::Rekey, 32);
    let export = hkdf_expand(ikm, KdfLabel::Export, 32);
    assert_ne!(session, rekey);
    assert_ne!(session, export);
    assert_ne!(rekey, export);

    // Custom label should yield distinct output and be reproducible
    let custom1 = hkdf_expand(ikm, KdfLabel::Custom(b"my-label"), 32);
    let custom2 = hkdf_expand(ikm, KdfLabel::Custom(b"my-label"), 32);
    assert_eq!(custom1, custom2);
    assert_ne!(custom1, session);
} 