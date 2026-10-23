// Copyright 2026 AsterSQL.
use crate::{CallbackTxn, Result};

impl CallbackTxn<'_> {
    /// Read a key and guard its modification revision at commit time.
    /// Missing keys use create revision zero; values stay out of the commit request.
    pub async fn load_with_revision(&mut self, key: &[u8]) -> Result<Vec<u8>> {
        self.load_value(key, true).await
    }
}
