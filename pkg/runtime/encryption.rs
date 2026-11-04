// Copyright 2026 AsterSQL.
//! One node-owned dictionary/watch shared with all term persistence adapters.
use astersql_pd_encryption::{DictionaryStorage, EncryptionConfig, KeyRotation, KeyWatch};
use astersql_pd_storage::MetaStore;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) struct EncryptionResources {
    pub(crate) dictionary: Arc<DictionaryStorage>,
    pub(crate) rotation: Arc<KeyRotation>,
    pub(crate) config: EncryptionConfig,
    watch: tokio::sync::Mutex<Option<KeyWatch>>,
}
impl EncryptionResources {
    pub(crate) async fn start(
        store: Arc<dyn MetaStore>,
        mut config: EncryptionConfig,
        cancel: &CancellationToken,
    ) -> Result<Arc<Self>, crate::NodeError> {
        config.adjust()?;
        let dictionary = Arc::new(DictionaryStorage::new(
            store,
            Arc::new(astersql_pd_server::encryption_dictionary::ProtoKeyDictionaryCodec),
        ));
        let rotation = Arc::new(KeyRotation::new(dictionary.clone(), &config)?);
        let watch =
            KeyWatch::start(&tokio::runtime::Handle::current(), rotation.clone(), cancel).await?;
        Ok(Arc::new(Self {
            dictionary,
            rotation,
            config,
            watch: tokio::sync::Mutex::new(Some(watch)),
        }))
    }
    pub(crate) async fn shutdown(&self) -> Result<(), crate::NodeError> {
        self.rotation.set_leadership(None).await?;
        if let Some(watch) = self.watch.lock().await.take() {
            watch.shutdown().await?;
        }
        Ok(())
    }
}
