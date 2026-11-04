// Copyright 2026 AsterSQL.
use crate::{DataKey, DictionaryStorage};
use astersql_pd_core::RegionMeta;
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::*};
use std::{borrow::Cow, error::Error, fmt};
#[derive(Debug)]
pub enum RegionCipherError {
    Typed(TypedError),
    KeyLength(usize),
    IvLength(usize),
}
impl From<TypedError> for RegionCipherError {
    fn from(e: TypedError) -> Self {
        Self::Typed(e)
    }
}
impl fmt::Display for RegionCipherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Typed(e) => e.fmt(f),
            Self::KeyLength(n) => write!(
                f,
                "fail to create aes cipher: crypto/aes: invalid key size {n}"
            ),
            Self::IvLength(_) => f.write_str("cipher.NewCTR: IV length must equal block size"),
        }
    }
}
impl Error for RegionCipherError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Typed(e) => Some(e),
            _ => None,
        }
    }
}
impl DictionaryStorage {
    pub fn get_current_key(&self) -> Result<Option<(u64, DataKey)>, TypedError> {
        let snapshot = self.snapshot();
        let Some(keys) = snapshot.keys else {
            return Ok(None);
        };
        if keys.current_key_id == 0 {
            return Ok(None);
        }
        let id = keys.current_key_id;
        let key = keys.keys.get(&id).ok_or_else(|| {
            let message = if keys.keys.is_empty() {
                "empty key list, currentKeyID = %d"
            } else {
                "currentKeyID = %d"
            };
            ErrorTemplate::new(ERR_ENCRYPTION_CURRENT_KEY_NOT_FOUND.code(), message)
                .instantiate([id.to_string()])
        })?;
        Ok(Some((id, key.clone())))
    }
    pub async fn get_key(&self, id: u64) -> Result<DataKey, TypedError> {
        if let Some(key) = self
            .snapshot()
            .keys
            .as_ref()
            .and_then(|keys| keys.keys.get(&id))
            .cloned()
        {
            return Ok(key);
        }
        let operation = self.begin_operation().await;
        if let Some(key) = self
            .snapshot()
            .keys
            .as_ref()
            .and_then(|keys| keys.keys.get(&id))
            .cloned()
        {
            return Ok(key);
        }
        let loaded = operation.load().await?;
        loaded
            .keys
            .as_ref()
            .and_then(|keys| keys.keys.get(&id))
            .cloned()
            .ok_or_else(|| {
                ErrorTemplate::new(ERR_ENCRYPTION_KEY_NOT_FOUND.code(), "keyId = %d")
                    .instantiate([id.to_string()])
            })
    }
}
/// Return the same borrowed region when disabled; encrypt only an owned clone.
pub fn encrypt_region<'a>(
    region: Option<&'a RegionMeta>,
    manager: Option<&DictionaryStorage>,
) -> Result<Cow<'a, RegionMeta>, RegionCipherError> {
    let region = region.ok_or_else(|| {
        ErrorTemplate::new(
            ERR_ENCRYPTION_ENCRYPT_REGION.code(),
            "trying to encrypt nil region",
        )
        .instantiate([] as [String; 0])
    })?;
    if region.encryption_meta.is_some() {
        return Err(ErrorTemplate::new(
            ERR_ENCRYPTION_ENCRYPT_REGION.code(),
            "region already encrypted, region id = %d",
        )
        .instantiate([region.id.to_string()])
        .into());
    }
    let Some(manager) = manager else {
        return Ok(Cow::Borrowed(region));
    };
    let Some((key_id, key)) = manager.get_current_key()? else {
        return Ok(Cow::Borrowed(region));
    };
    crate::check_encryption_method_supported(key.method)?;
    let iv = crate::new_iv_ctr()?;
    let mut out = region.clone();
    process_region_keys(&mut out, &key, &iv)?;
    out.encryption_meta = Some(astersql_pd_core::RegionEncryptionMeta {
        key_id,
        iv: iv.to_vec(),
    });
    Ok(Cow::Owned(out))
}
/// Decrypt in place only after validating key and IV, then clear metadata.
pub async fn decrypt_region(
    region: Option<&mut RegionMeta>,
    manager: Option<&DictionaryStorage>,
) -> Result<(), RegionCipherError> {
    let region = region.ok_or_else(|| {
        ErrorTemplate::new(
            ERR_ENCRYPTION_DECRYPT_REGION.code(),
            "trying to decrypt nil region",
        )
        .instantiate([] as [String; 0])
    })?;
    let Some(meta) = region.encryption_meta.clone() else {
        return Ok(());
    };
    let manager = manager.ok_or_else(|| {
        ErrorTemplate::new(
            ERR_ENCRYPTION_DECRYPT_REGION.code(),
            "unable to decrypt region without encryption keys",
        )
        .instantiate([] as [String; 0])
    })?;
    let key = manager.get_key(meta.key_id).await?;
    crate::check_encryption_method_supported(key.method)?;
    process_region_keys(region, &key, &meta.iv)?;
    region.encryption_meta = None;
    Ok(())
}
pub(crate) fn process_region_keys(
    region: &mut RegionMeta,
    key: &DataKey,
    iv: &[u8],
) -> Result<(), RegionCipherError> {
    use aes::cipher::KeyInit;
    let length = key.key.len();
    if ![16, 24, 32].contains(&length) {
        return Err(RegionCipherError::KeyLength(length));
    }
    let iv: [u8; 16] = iv
        .try_into()
        .map_err(|_| RegionCipherError::IvLength(iv.len()))?;
    match length {
        16 => apply(
            &aes::Aes128::new_from_slice(&key.key).expect("validated AES128 length"),
            region,
            iv,
        ),
        24 => apply(
            &aes::Aes192::new_from_slice(&key.key).expect("validated AES192 length"),
            region,
            iv,
        ),
        32 => apply(
            &aes::Aes256::new_from_slice(&key.key).expect("validated AES256 length"),
            region,
            iv,
        ),
        _ => unreachable!("validated AES key length"),
    }
    Ok(())
}
fn apply<C: aes::cipher::BlockEncrypt>(cipher: &C, region: &mut RegionMeta, iv: [u8; 16]) {
    use zeroize::Zeroize;
    let mut counter = u128::from_be_bytes(iv);
    let mut stream = aes::cipher::Block::<C>::default();
    let mut used = 16;
    // One continuous stream spans both bounds, including partial-block splits.
    // Go CTR wraps the full big-endian counter instead of rejecting u128 overflow.
    for byte in region
        .key_range
        .start_key
        .iter_mut()
        .chain(region.key_range.end_key.iter_mut())
    {
        if used == 16 {
            stream.copy_from_slice(&counter.to_be_bytes());
            cipher.encrypt_block(&mut stream);
            counter = counter.wrapping_add(1);
            used = 0;
        }
        *byte ^= stream[used];
        used += 1;
    }
    stream.as_mut_slice().zeroize();
}
