// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn encryption_defaults_and_methods() {
    let mut c = EncryptionConfig::default();
    c.adjust().unwrap();
    assert_eq!(c.data_key_rotation_period.as_nanos(), 604800000000000);
    assert_eq!(c.master_key_meta().unwrap(), MasterKeyMeta::Plaintext);
    for (name, method) in [
        ("plaintext", EncryptionMethod::Plaintext),
        ("aes128-ctr", EncryptionMethod::Aes128Ctr),
        ("aes192-ctr", EncryptionMethod::Aes192Ctr),
        ("aes256-ctr", EncryptionMethod::Aes256Ctr),
    ] {
        c.data_encryption_method = name.into();
        assert_eq!(c.method().unwrap(), method);
        c.adjust().unwrap();
    }
    c.master_key.key_type = "kms".into();
    c.master_key.kms.kms_key_id = "id".into();
    assert!(
        matches!(c.master_key_meta().unwrap(),MasterKeyMeta::Kms{vendor:"AWS",key_id,..} if key_id=="id")
    );
    c.master_key.key_type = "file".into();
    c.master_key.file.file_path = "/key".into();
    assert_eq!(
        c.master_key_meta().unwrap(),
        MasterKeyMeta::File {
            path: "/key".into()
        }
    );
}
#[test]
fn encryption_errors_preserve_code_precedence_and_partial_defaults() {
    let mut c = EncryptionConfig::default();
    c.data_encryption_method = "AES256-CTR".into();
    c.data_key_rotation_period = Duration::from_nanos(-1);
    c.master_key.key_type = "bad".into();
    let e = c.adjust().unwrap_err();
    assert_eq!(e.code(), "PD:encryption:ErrEncryptionInvalidMethod");
    assert_eq!(
        e.to_string(),
        "[PD:encryption:ErrEncryptionInvalidMethod]unknown method"
    );
    c.data_encryption_method.clear();
    let e = c.adjust().unwrap_err();
    assert_eq!(
        e.to_string(),
        "[PD:encryption:ErrEncryptionInvalidConfig]negative data-key-rotation-period -1"
    );
    assert_eq!(c.data_encryption_method, "plaintext");
    c.data_key_rotation_period = Duration::default();
    assert!(
        c.adjust()
            .unwrap_err()
            .to_string()
            .ends_with("unrecognized encryption master key type: bad")
    );
    assert_eq!(c.data_key_rotation_period.as_nanos(), 604800000000000);
}
