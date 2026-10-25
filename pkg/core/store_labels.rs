// Copyright 2026 AsterSQL.
//! Store labels retain ordering and duplicates; engine lookup uses exact keys.
use crate::{StoreInfo, StoreMeta};
use std::fmt;

pub const ENGINE_KEY: &str = "engine";
pub const ENGINE_ROLE_KEY: &str = "engine_role";
pub const ENGINE_TIKV: &str = "tikv";
pub const ENGINE_TIFLASH: &str = "tiflash";
pub const ENGINE_TIFLASH_COMPUTE: &str = "tiflash_compute";

/// An owned domain label, independent of protocol storage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreLabel {
    pub key: String,
    pub value: String,
}

// Go EqualFold uses Unicode simple folding, not full folding (ß != ss).
/// Compare strings using Go-compatible Unicode simple folding.
pub fn equal_fold(a: &str, b: &str) -> bool {
    fn fold(c: char) -> char {
        // Uppercase then lowercase closes simple-fold classes such as Greek
        // symbol variants and micro sign. Never expand a character to a string.
        if matches!(c, 'İ' | 'ı') {
            return c;
        }
        let mut upper = c.to_uppercase();
        let first = upper.next().unwrap();
        let base = if upper.next().is_none() { first } else { c };
        let mut lower = base.to_lowercase();
        let first = lower.next().unwrap();
        if lower.next().is_none() { first } else { base }
    }
    a.chars().map(fold).eq(b.chars().map(fold))
}

/// Merge in order, updating only the first matching key and filtering empty
/// values after all updates. Inputs remain owned by their callers.
pub fn merge_labels(origin: &[StoreLabel], labels: &[StoreLabel]) -> Vec<StoreLabel> {
    let mut result = origin.to_vec();
    for label in labels {
        if let Some(old) = result
            .iter_mut()
            .find(|old| equal_fold(&old.key, &label.key))
        {
            old.value.clone_from(&label.value);
        } else {
            result.push(label.clone());
        }
    }
    result.retain(|label| !label.value.is_empty());
    result
}

fn exact_label_value<'a>(store: Option<&'a StoreMeta>, key: &str) -> &'a str {
    store
        .and_then(|s| s.labels.iter().find(|label| label.key == key))
        .map_or("", |l| l.value.as_str())
}

/// Test exact key and value equality; absent keys have the empty value.
pub fn is_store_contain_label(store: Option<&StoreMeta>, key: &str, value: &str) -> bool {
    exact_label_value(store, key) == value
}

/// An unrecognized engine label, preserving its original value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidStoreEngine {
    pub value: String,
}
// Match Go %q escapes rather than Rust's brace-style Unicode escapes.
fn go_quote(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{b}' => out.push_str("\\v"),
            c if c.is_control() || c.escape_debug().to_string().starts_with("\\u{") => {
                use std::fmt::Write;
                let n = c as u32;
                if n < 128 {
                    write!(out, "\\x{n:02x}").unwrap();
                } else if n <= 0xffff {
                    write!(out, "\\u{n:04x}").unwrap();
                } else {
                    write!(out, "\\U{n:08x}").unwrap();
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
impl fmt::Display for InvalidStoreEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid store engine label value: {}, valid values are \"tikv\", \"tiflash\", \"tiflash_compute\" or empty",
            go_quote(&self.value)
        )
    }
}
impl std::error::Error for InvalidStoreEngine {}

/// Accept empty, TiKV, TiFlash write/classic and TiFlash compute labels.
pub fn validate_store_engine_key(store: Option<&StoreMeta>) -> Result<(), InvalidStoreEngine> {
    match exact_label_value(store, ENGINE_KEY) {
        "" | ENGINE_TIKV | ENGINE_TIFLASH | ENGINE_TIFLASH_COMPUTE => Ok(()),
        value => Err(InvalidStoreEngine {
            value: value.into(),
        }),
    }
}

impl StoreInfo {
    pub fn labels(&self) -> &[StoreLabel] {
        self.meta().map_or(&[], |meta| meta.labels.as_slice())
    }

    /// Replace labels. Like SetStoreLabels, absent metadata is a programmer error.
    pub fn set_labels(&mut self, labels: Vec<StoreLabel>) {
        std::sync::Arc::make_mut(self.meta.as_mut().expect("store metadata must be present"))
            .labels = labels;
    }

    /// Return the first case-insensitive key match, or empty for missing keys.
    pub fn label_value(&self, key: &str) -> &str {
        self.labels()
            .iter()
            .find(|l| equal_fold(&l.key, key))
            .map_or("", |l| l.value.as_str())
    }

    pub fn is_tikv(&self) -> bool {
        matches!(exact_label_value(self.meta(), ENGINE_KEY), "" | ENGINE_TIKV)
    }

    pub fn is_tiflash_write(&self) -> bool {
        is_store_contain_label(self.meta(), ENGINE_KEY, ENGINE_TIFLASH)
    }

    pub fn is_tiflash_compute(&self) -> bool {
        is_store_contain_label(self.meta(), ENGINE_KEY, ENGINE_TIFLASH_COMPUTE)
    }

    /// Go's metrics classification groups every non-TiKV value under TiFlash,
    /// including compute and invalid labels. Validate separately at boundaries.
    pub fn engine(&self) -> &'static str {
        if self.is_tikv() {
            ENGINE_TIKV
        } else {
            ENGINE_TIFLASH
        }
    }
}
