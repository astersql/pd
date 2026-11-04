// Copyright 2026 AsterSQL.
use crate::*;
fn unhex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn mcs_009_behavior() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/mcs-009.json")).unwrap();
    let key = unhex(f["key"].as_str().unwrap());
    let plain = unhex(f["plaintext"].as_str().unwrap());
    let nonce: [u8; 12] = unhex(f["iv"].as_str().unwrap()).try_into().unwrap();
    let expected = unhex(f["ciphertext"].as_str().unwrap());
    assert_eq!(
        aes_gcm_encrypt_with_iv(&key, &plain, &nonce).unwrap(),
        expected,
        "saved Go TestAesGcmCrypter ciphertext+tag must match byte-for-byte"
    );
    assert_eq!(aes_gcm_decrypt(&key, &expected, &nonce).unwrap(), plain);
    let mut bad = expected;
    bad[0] = bad[0].wrapping_add(1);
    let error = aes_gcm_decrypt(&key, &bad, &nonce).unwrap_err();
    assert_eq!(error.code(), "PD:encryption:ErrEncryptionGCMDecrypt");
}
#[test]
fn mcs_009_methods_and_entropy_outputs() {
    for (method, length) in [
        (EncryptionMethod::Aes128Ctr, 16),
        (EncryptionMethod::Aes192Ctr, 24),
        (EncryptionMethod::Aes256Ctr, 32),
    ] {
        assert_eq!(key_length(method).unwrap(), length);
        check_encryption_method_supported(method).unwrap();
        let (_, key) = new_data_key(method, 123).unwrap();
        assert_eq!(key.key.len(), length);
        assert_eq!(key.creation_time, 123);
        assert_eq!(key.method, method);
        assert!(!key.was_exposed);
    }
    for method in [
        EncryptionMethod::Plaintext,
        EncryptionMethod::Unknown(0),
        EncryptionMethod::Unknown(99),
    ] {
        assert_eq!(
            key_length(method).unwrap_err().code(),
            "PD:encryption:ErrEncryptionInvalidMethod"
        );
        assert!(new_data_key(method, 123).is_err());
    }
    assert_eq!(new_iv_ctr().unwrap().len(), 16);
    assert_eq!(new_iv_gcm().unwrap().len(), 12);
}

#[test]
fn mcs_009_gcm_modes_and_failure_precedence() {
    for size in [16, 24, 32] {
        let key = vec![7; size];
        for plain in [b"".as_slice(), b"domain key material\0\xff".as_slice()] {
            let (cipher, iv) = aes_gcm_encrypt(&key, plain).unwrap();
            assert_eq!(cipher.len(), plain.len() + 16);
            assert_eq!(aes_gcm_decrypt(&key, &cipher, &iv).unwrap(), plain);
            let mut wrong = iv;
            wrong[0] ^= 1;
            assert!(aes_gcm_decrypt(&key, &cipher, &wrong).is_err());
            let mut wrong = cipher.clone();
            *wrong.last_mut().unwrap() ^= 1;
            assert!(aes_gcm_decrypt(&key, &wrong, &iv).is_err());
            assert!(aes_gcm_decrypt(&key, &cipher[..15], &iv).is_err());
        }
    }
    let error = aes_gcm_decrypt(&[0; 31], b"", &[0; 11]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "[PD:encryption:ErrEncryptionGCMDecrypt]unexpected gcm iv length 11"
    );
    for size in [0, 1, 15, 17, 23, 25, 31, 33] {
        let key = vec![0; size];
        assert_eq!(
            aes_gcm_encrypt_with_iv(&key, b"", &[0; 12])
                .unwrap_err()
                .to_string(),
            format!(
                "[PD:encryption:ErrEncryptionGCMEncrypt]fail to create aes cipher: crypto/aes: invalid key size {size}"
            )
        );
        assert_eq!(
            aes_gcm_decrypt(&key, b"", &[0; 12]).unwrap_err().code(),
            "PD:encryption:ErrEncryptionGCMDecrypt"
        );
    }
    assert_eq!(EncryptionMethod::from_value(3), EncryptionMethod::Aes192Ctr);
    assert_eq!(
        EncryptionMethod::from_value(-1),
        EncryptionMethod::Unknown(-1)
    );
    assert_eq!(
        key_length(EncryptionMethod::Plaintext)
            .unwrap_err()
            .to_string(),
        "[PD:encryption:ErrEncryptionInvalidMethod]invalid encryption method%!(EXTRA string=PLAINTEXT)"
    );
    assert_eq!(
        key_length(EncryptionMethod::Unknown(99))
            .unwrap_err()
            .to_string(),
        "[PD:encryption:ErrEncryptionInvalidMethod]invalid encryption method%!(EXTRA int32=99)"
    );
}

#[test]
fn mcs_009_go_standard_library_vectors() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/mcs-009-go-stdlib.json")).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let key = unhex(vector["key"].as_str().unwrap());
        let iv: [u8; 12] = unhex(vector["iv"].as_str().unwrap()).try_into().unwrap();
        let plaintext = unhex(vector["plaintext"].as_str().unwrap());
        let ciphertext = unhex(vector["ciphertext"].as_str().unwrap());
        assert_eq!(
            aes_gcm_encrypt_with_iv(&key, &plaintext, &iv).unwrap(),
            ciphertext,
            "Go saved GCM vector with {}-bit key",
            key.len() * 8
        );
        assert_eq!(aes_gcm_decrypt(&key, &ciphertext, &iv).unwrap(), plaintext);
    }
}
