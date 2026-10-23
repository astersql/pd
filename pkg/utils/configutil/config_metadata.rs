// Copyright 2026 AsterSQL.

use std::{collections::BTreeSet, sync::Arc};

/// Field presence independent of configuration values and parser defaults.
///
/// The default represents absent metadata, like Go's nil TOML metadata.
#[derive(Clone, Debug, Default)]
pub struct ConfigMetaData {
    defined: Arc<BTreeSet<Vec<String>>>,
    path: Vec<String>,
    pub(super) undecoded: Arc<Vec<String>>,
}

impl ConfigMetaData {
    /// Builds metadata from hierarchical paths reachable through configuration
    /// tables. Each segment is literal (dots are not separators). Table prefixes
    /// are also defined; an empty path defines nothing. Include empty tables and
    /// scalar/array fields themselves, but not paths through scalar/array values.
    /// Values, including false, zero and empty strings, do not affect presence.
    pub fn from_defined_paths<I, P, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut defined = BTreeSet::new();
        for path in paths {
            let mut prefix = Vec::new();
            for segment in path {
                prefix.push(segment.into());
                defined.insert(prefix.clone());
            }
        }
        Self {
            defined: Arc::new(defined),
            path: Vec::new(),
            undecoded: Arc::default(),
        }
    }

    /// Reports whether a literal key exists below the current path.
    pub fn is_defined(&self, key: &str) -> bool {
        let mut path = self.path.clone();
        path.push(key.to_owned());
        self.defined.contains(&path)
    }

    /// Returns a view below additional literal path segments without changing
    /// this view or its siblings. An empty path returns an equivalent view.
    pub fn child<I, S>(&self, path: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut new_path = self.path.clone();
        new_path.extend(path.into_iter().map(Into::into));
        Self {
            defined: Arc::clone(&self.defined),
            path: new_path,
            undecoded: Arc::clone(&self.undecoded),
        }
    }
}
