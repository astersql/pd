// Copyright 2026 AsterSQL.
//! Data-key primitives. Keys remain domain values, independent of protobuf and KMS.
use crate::EncryptionMethod;
use aes_gcm::{
    Aes128Gcm, Aes256Gcm, AesGcm, KeyInit, Nonce,
    aead::{Aead, consts::U12},
};
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::*};
use rand::{CryptoRng, RngCore, rngs::OsRng};
use std::{error::Error, fmt};
type Aes192Gcm = AesGcm<aes::Aes192, U12>;

/// Owned secret material and its metadata; Debug deliberately excludes the key bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct DataKey {
    pub key: Vec<u8>,
    pub method: EncryptionMethod,
    pub creation_time: u64,
    pub was_exposed: bool,
}
impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataKey")
            .field("key_length", &self.key.len())
            .field("method", &self.method)
            .field("creation_time", &self.creation_time)
            .field("was_exposed", &self.was_exposed)
            .finish()
    }
}
impl EncryptionMethod {
    /// Decode stable method IDs, retaining unknown future values without a protocol dependency.
    pub const fn from_value(value: i32) -> Self {
        match value {
            1 => Self::Plaintext,
            2 => Self::Aes128Ctr,
            3 => Self::Aes192Ctr,
            4 => Self::Aes256Ctr,
            value => Self::Unknown(value),
        }
    }
    pub const fn value(self) -> i32 {
        match self {
            Self::Plaintext => 1,
            Self::Aes128Ctr => 2,
            Self::Aes192Ctr => 3,
            Self::Aes256Ctr => 4,
            Self::Unknown(value) => value,
        }
    }
}
fn invalid_method(method: EncryptionMethod) -> TypedError {
    // GenWithStackByArgs retains its extra argument even though the Go template
    // has no format verb. Preserve Go's caller-visible fmt output.
    match method.value() {
        0 => ErrorTemplate::new(
            ERR_ENCRYPTION_INVALID_METHOD.code(),
            "invalid encryption method%!(EXTRA string=%s)",
        )
        .instantiate(["UNKNOWN"]),
        1 => ErrorTemplate::new(
            ERR_ENCRYPTION_INVALID_METHOD.code(),
            "invalid encryption method%!(EXTRA string=%s)",
        )
        .instantiate(["PLAINTEXT"]),
        value => ErrorTemplate::new(
            ERR_ENCRYPTION_INVALID_METHOD.code(),
            "invalid encryption method%!(EXTRA int32=%d)",
        )
        .instantiate([value.to_string()]),
    }
}
/// Length of a supported data-encryption key, in bytes. Plaintext is not a data key.
pub fn key_length(method: EncryptionMethod) -> Result<usize, TypedError> {
    match method.value() {
        2 => Ok(16),
        3 => Ok(24),
        4 => Ok(32),
        _ => Err(invalid_method(method)),
    }
}
pub fn check_encryption_method_supported(method: EncryptionMethod) -> Result<(), TypedError> {
    key_length(method).map(|_| ())
}
fn random_iv<const N: usize>() -> Result<[u8; N], TypedError> {
    let mut iv = [0; N];
    OsRng
        .try_fill_bytes(&mut iv)
        .map_err(|e| ERR_ENCRYPTION_GENERATE_IV.with_source([] as [String; 0], e))?;
    Ok(iv)
}
/// Generate a 128-bit CTR IV with the OS cryptographic random source.
pub fn new_iv_ctr() -> Result<[u8; 16], TypedError> {
    random_iv()
}
/// Generate a 96-bit GCM nonce with the OS cryptographic random source.
pub fn new_iv_gcm() -> Result<[u8; 12], TypedError> {
    random_iv()
}
/// Generate an independent big-endian random ID and method-sized secret key.
pub fn new_data_key(
    method: EncryptionMethod,
    creation_time: u64,
) -> Result<(u64, DataKey), TypedError> {
    new_data_key_with_rng(method, creation_time, &mut OsRng)
}
/// The host may provide a cryptographic entropy source; the default remains OsRng.
pub fn new_data_key_with_rng<R: RngCore + CryptoRng + ?Sized>(
    method: EncryptionMethod,
    creation_time: u64,
    random: &mut R,
) -> Result<(u64, DataKey), TypedError> {
    let length = key_length(method)?;
    let mut id = [0; 8];
    random.try_fill_bytes(&mut id).map_err(|e| {
        ErrorTemplate::new(
            ERR_ENCRYPTION_NEW_DATA_KEY.code(),
            "fail to generate data key id",
        )
        .with_source([] as [String; 0], e)
    })?;
    let mut key = vec![0; length];
    random.try_fill_bytes(&mut key).map_err(|e| {
        ErrorTemplate::new(
            ERR_ENCRYPTION_NEW_DATA_KEY.code(),
            "fail to generate data key",
        )
        .with_source([] as [String; 0], e)
    })?;
    Ok((
        u64::from_be_bytes(id),
        DataKey {
            key,
            method: EncryptionMethod::from_value(method.value()),
            creation_time,
            was_exposed: false,
        },
    ))
}
#[derive(Debug)]
struct CipherCause(String);
impl fmt::Display for CipherCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CipherCause {}
fn invalid_key(code: &'static str, length: usize) -> TypedError {
    ErrorTemplate::new(code, "fail to create aes cipher").with_source(
        [] as [String; 0],
        CipherCause(format!("crypto/aes: invalid key size {length}")),
    )
}
fn auth_error() -> TypedError {
    ErrorTemplate::new(ERR_ENCRYPTION_GCM_DECRYPT.code(), "authentication fail").with_source(
        [] as [String; 0],
        CipherCause("cipher: message authentication failed".into()),
    )
}
/// Deterministic GCM primitive for an explicit 96-bit nonce; callers own nonce uniqueness.
/// Ciphertext includes the 128-bit authentication tag, with no associated data.
pub fn aes_gcm_encrypt_with_iv(
    key: &[u8],
    plaintext: &[u8],
    iv: &[u8; 12],
) -> Result<Vec<u8>, TypedError> {
    macro_rules! encrypt {
        ($cipher:ty) => {{
            let cipher = <$cipher>::new_from_slice(key)
                .map_err(|_| invalid_key(ERR_ENCRYPTION_GCM_ENCRYPT.code(), key.len()))?;
            cipher
                .encrypt(Nonce::from_slice(iv), plaintext)
                .map_err(|_| {
                    ErrorTemplate::new(ERR_ENCRYPTION_GCM_ENCRYPT.code(), "message too large")
                        .instantiate([] as [String; 0])
                })
        }};
    }
    match key.len() {
        16 => encrypt!(Aes128Gcm),
        24 => encrypt!(Aes192Gcm),
        32 => encrypt!(Aes256Gcm),
        length => Err(invalid_key(ERR_ENCRYPTION_GCM_ENCRYPT.code(), length)),
    }
}
/// Encrypt using a newly generated nonce, returning ciphertext/tag and nonce separately.
pub fn aes_gcm_encrypt(key: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 12]), TypedError> {
    let iv = new_iv_gcm()?;
    Ok((aes_gcm_encrypt_with_iv(key, plaintext, &iv)?, iv))
}
/// Validate the nonce before the key, then authenticate before returning any plaintext.
pub fn aes_gcm_decrypt(key: &[u8], ciphertext: &[u8], iv: &[u8]) -> Result<Vec<u8>, TypedError> {
    if iv.len() != 12 {
        return Err(ErrorTemplate::new(
            ERR_ENCRYPTION_GCM_DECRYPT.code(),
            "unexpected gcm iv length %d",
        )
        .instantiate([iv.len().to_string()]));
    }
    macro_rules! decrypt {
        ($cipher:ty) => {{
            let cipher = <$cipher>::new_from_slice(key)
                .map_err(|_| invalid_key(ERR_ENCRYPTION_GCM_DECRYPT.code(), key.len()))?;
            cipher
                .decrypt(Nonce::from_slice(iv), ciphertext)
                .map_err(|_| auth_error())
        }};
    }
    match key.len() {
        16 => decrypt!(Aes128Gcm),
        24 => decrypt!(Aes192Gcm),
        32 => decrypt!(Aes256Gcm),
        length => Err(invalid_key(ERR_ENCRYPTION_GCM_DECRYPT.code(), length)),
    }
}
