// Copyright 2026 AsterSQL.
//! AWS is an explicit asynchronous boundary; callers own runtime and cancellation.
use crate::{MasterKey, MasterKeyFileConfig, MasterKeyMeta};
use astersql_pd_errs::{
    ErrorTemplate, TypedError,
    catalog::{ERR_ENCRYPTION_KMS, ERR_ENCRYPTION_NEW_MASTER_KEY},
};
use aws_sdk_kms::{Client, config::Region, error::ProvideErrorMetadata, primitives::Blob};
use zeroize::Zeroizing;

fn error(message: &'static str, args: impl IntoIterator<Item = String>) -> TypedError {
    ErrorTemplate::new(ERR_ENCRYPTION_KMS.code(), message).instantiate(args)
}
// Never format SDK responses or credential/provider errors: service messages can
// contain caller-controlled payloads. Preserve the PD operation and error class.
fn request_error(operation: &'static str, service_code: Option<&str>) -> TypedError {
    let category = match service_code {
        Some("AccessDeniedException") => "AccessDeniedException",
        Some("NotFoundException") => "NotFoundException",
        Some("DisabledException") => "DisabledException",
        Some("InvalidCiphertextException") => "InvalidCiphertextException",
        Some("KMSInvalidStateException") => "KMSInvalidStateException",
        Some("ThrottlingException") => "ThrottlingException",
        _ => "AWS KMS request failed",
    };
    ErrorTemplate::new(ERR_ENCRYPTION_KMS.code(), operation)
        .with_source([] as [String; 0], std::io::Error::other(category))
}
impl MasterKey {
    /// Load file/plaintext or AWS using the official SDK default credential chain.
    /// The persisted KMS endpoint field is ignored by Go; SDK environment/profile
    /// endpoint settings remain effective. Region always comes from this metadata.
    pub async fn from_config(
        config: Option<&MasterKeyMeta>,
        ciphertext_key: &[u8],
    ) -> Result<Self, TypedError> {
        let config = config.ok_or_else(|| {
            ErrorTemplate::new(
                ERR_ENCRYPTION_NEW_MASTER_KEY.code(),
                "master key config is empty",
            )
            .instantiate([] as [String; 0])
        })?;
        match config {
            MasterKeyMeta::Plaintext => Ok(Self::plaintext()),
            MasterKeyMeta::File { path } => Self::from_file(Some(&MasterKeyFileConfig {
                file_path: path.clone(),
            })),
            MasterKeyMeta::Kms {
                vendor,
                key_id,
                region,
                ..
            } => {
                if *vendor != "AWS" {
                    return Err(error("unsupported KMS vendor: %s", [vendor.to_string()]));
                }
                let http = aws_smithy_http_client::Builder::new()
                    .tls_provider(aws_smithy_http_client::tls::Provider::Rustls(
                        aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring,
                    ))
                    .build_https();
                let sdk = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(Region::new(region.clone()))
                    .http_client(http.clone())
                    .load()
                    .await;
                let mut config = aws_sdk_kms::config::Builder::from(&sdk);
                // Go explicitly selects Kubernetes web identity before environment
                // credentials whenever both role and token-file variables exist.
                if std::env::var("AWS_ROLE_ARN").is_ok_and(|v| !v.is_empty())
                    && std::env::var("AWS_WEB_IDENTITY_TOKEN_FILE").is_ok_and(|v| !v.is_empty())
                {
                    let provider = aws_config::web_identity_token::WebIdentityTokenCredentialsProvider::builder()
                        .configure(&aws_config::provider_config::ProviderConfig::without_region()
                            .with_region(Some(Region::new(region.clone()))).with_http_client(http))
                        .build();
                    config = config.credentials_provider(provider);
                }
                Self::from_kms_client(&Client::from_conf(config.build()), key_id, ciphertext_key)
                    .await
            }
        }
    }
    /// Host/test injection accepts the real official SDK client, not a KMS mock trait.
    pub async fn from_kms_client(
        client: &Client,
        key_id: &str,
        ciphertext_key: &[u8],
    ) -> Result<Self, TypedError> {
        let (key, ciphertext, operation) = if ciphertext_key.is_empty() {
            let mut output = client
                .generate_data_key()
                .key_id(key_id)
                .number_of_bytes(32)
                .send()
                .await
                .map_err(|e| {
                    request_error(
                        "fail to generate data key from AWS KMS",
                        e.as_service_error().and_then(|e| e.code()),
                    )
                })?;
            (
                Zeroizing::new(
                    output
                        .plaintext
                        .take()
                        .map(Blob::into_inner)
                        .unwrap_or_default(),
                ),
                output
                    .ciphertext_blob
                    .take()
                    .map(Blob::into_inner)
                    .unwrap_or_default(),
                "unexpected data key length generated from AWS KMS, expected %d vs actual %d",
            )
        } else {
            let mut output = client
                .decrypt()
                .key_id(key_id)
                .ciphertext_blob(Blob::new(ciphertext_key))
                .send()
                .await
                .map_err(|e| {
                    request_error(
                        "fail to decrypt data key from AWS KMS",
                        e.as_service_error().and_then(|e| e.code()),
                    )
                })?;
            (
                Zeroizing::new(
                    output
                        .plaintext
                        .take()
                        .map(Blob::into_inner)
                        .unwrap_or_default(),
                ),
                ciphertext_key.to_vec(),
                "unexpected data key length decrypted from AWS KMS, expected %d vs actual %d",
            )
        };
        if key.len() != 32 {
            return Err(error(operation, ["32".into(), key.len().to_string()]));
        }
        Ok(Self {
            key: Some(key),
            ciphertext_key: ciphertext,
        })
    }
}
