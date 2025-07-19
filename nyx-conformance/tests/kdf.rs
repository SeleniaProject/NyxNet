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
} 