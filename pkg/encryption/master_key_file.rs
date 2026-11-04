// Copyright 2026 AsterSQL.
//! Explicit plaintext/file backends; private key bytes never enter Debug or errors.
use crate::{MasterKeyFileConfig, aes_gcm_decrypt, aes_gcm_encrypt};
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::ERR_ENCRYPTION_NEW_MASTER_KEY};
use std::{error::Error, fmt};
use zeroize::Zeroizing;

pub struct MasterKey {
    pub(crate) key: Option<Zeroizing<Vec<u8>>>,
    pub(crate) ciphertext_key: Vec<u8>,
}
impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MasterKey")
            .field("plaintext", &self.is_plaintext())
            .field("ciphertext_key_length", &self.ciphertext_key.len())
            .finish()
    }
}
fn error(message: &'static str, args: impl IntoIterator<Item = String>) -> TypedError {
    ErrorTemplate::new(ERR_ENCRYPTION_NEW_MASTER_KEY.code(), message).instantiate(args)
}
#[derive(Debug)]
struct FileCause(String);
impl fmt::Display for FileCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for FileCause {}

impl MasterKey {
    /// Plaintext master mode is an explicit no-op, including nonce validation.
    pub fn plaintext() -> Self {
        Self {
            key: None,
            ciphertext_key: vec![],
        }
    }
    /// Read a 256-bit hex master key from the caller-specified file.
    pub fn from_file(config: Option<&MasterKeyFileConfig>) -> Result<Self, TypedError> {
        let config = config.ok_or_else(|| error("missing master key file config", []))?;
        let path = &config.file_path;
        if path.is_empty() {
            return Err(error("missing master key file path", []));
        }
        let data = Zeroizing::new(std::fs::read(path).map_err(|e| {
            let operation = if e.kind() == std::io::ErrorKind::IsADirectory {
                "read"
            } else {
                "open"
            };
            let text = e.to_string();
            let description = text
                .split(" (os error ")
                .next()
                .unwrap_or(&text)
                .to_ascii_lowercase();
            let cause = format!("{operation} {path}: {description}");
            ErrorTemplate::new(
                ERR_ENCRYPTION_NEW_MASTER_KEY.code(),
                "fail to get encryption key from file %s%!(EXTRA string=%s)",
            )
            .with_source([path.clone(), cause.clone()], FileCause(cause))
        })?);
        let key = decode_hex(trim_space(&data)).map_err(|cause| {
            ErrorTemplate::new(
                ERR_ENCRYPTION_NEW_MASTER_KEY.code(),
                "failed to decode encryption key from file, the key must be in hex form",
            )
            .with_source([] as [String; 0], cause)
        })?;
        if key.len() != 32 {
            return Err(error(
                "unexpected key length from master key file, expected %d vs actual %d",
                ["32".into(), key.len().to_string()],
            ));
        }
        Ok(Self {
            key: Some(key),
            ciphertext_key: vec![],
        })
    }
    pub fn is_plaintext(&self) -> bool {
        self.key.is_none()
    }
    pub fn ciphertext_key(&self) -> &[u8] {
        &self.ciphertext_key
    }
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), TypedError> {
        match &self.key {
            None => Ok((plaintext.to_vec(), vec![])),
            Some(key) => {
                let (data, iv) = aes_gcm_encrypt(key, plaintext)?;
                Ok((data, iv.to_vec()))
            }
        }
    }
    pub fn decrypt(&self, ciphertext: &[u8], iv: &[u8]) -> Result<Vec<u8>, TypedError> {
        match &self.key {
            None => Ok(ciphertext.to_vec()),
            Some(key) => aes_gcm_decrypt(key, ciphertext, iv),
        }
    }
}
// Decode only the first/last scalar while retaining invalid UTF-8 bytes for hex's
// own byte-oriented error, just as Go strings.TrimSpace does.
fn trim_space(mut data: &[u8]) -> &[u8] {
    loop {
        let Some(first) = data.first().copied() else {
            break;
        };
        let width = match first {
            0..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => break,
        };
        let Some(prefix) = data.get(..width) else {
            break;
        };
        let Ok(text) = std::str::from_utf8(prefix) else {
            break;
        };
        if !text.chars().next().unwrap().is_whitespace() {
            break;
        }
        data = &data[width..];
    }
    loop {
        let mut width = None;
        for n in 1..=4.min(data.len()) {
            if let Ok(text) = std::str::from_utf8(&data[data.len() - n..]) {
                let mut chars = text.chars();
                if chars.next().is_some_and(char::is_whitespace) && chars.next().is_none() {
                    width = Some(n);
                    break;
                }
            }
        }
        let Some(n) = width else { break };
        data = &data[..data.len() - n];
    }
    data
}
fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
fn invalid_byte(byte: u8) -> FileCause {
    let mut text = format!("encoding/hex: invalid byte: U+{byte:04X}");
    if (0x20..=0x7e).contains(&byte) || (byte >= 0xa1 && byte != 0xad) {
        let escaped = match byte {
            b'\'' => "\\'".into(),
            b'\\' => "\\\\".into(),
            _ => char::from(byte).to_string(),
        };
        text.push_str(&format!(" '{escaped}'"));
    }
    FileCause(text)
}
fn decode_hex(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, FileCause> {
    let mut decoded = Zeroizing::new(Vec::with_capacity(data.len() / 2));
    for pair in data.chunks_exact(2) {
        let first = nibble(pair[0]).ok_or_else(|| invalid_byte(pair[0]))?;
        let second = nibble(pair[1]).ok_or_else(|| invalid_byte(pair[1]))?;
        decoded.push(first << 4 | second);
    }
    if data.len() % 2 != 0 {
        let byte = *data.last().unwrap();
        nibble(byte).ok_or_else(|| invalid_byte(byte))?;
        return Err(FileCause("encoding/hex: odd length hex string".into()));
    }
    Ok(decoded)
}
