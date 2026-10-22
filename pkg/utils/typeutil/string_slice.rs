// Copyright 2026 AsterSQL.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A list of strings encoded in JSON as one comma-separated string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StringSlice(Vec<String>);

impl StringSlice {
    /// Returns the strings in this value.
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// Consumes this value and returns its strings.
    pub fn into_inner(self) -> Vec<String> {
        self.0
    }
}

impl<T, const N: usize> From<[T; N]> for StringSlice
where
    T: Into<String>,
{
    fn from(values: [T; N]) -> Self {
        Self(values.into_iter().map(Into::into).collect())
    }
}

impl From<Vec<String>> for StringSlice {
    fn from(values: Vec<String>) -> Self {
        Self(values)
    }
}

impl Serialize for StringSlice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.join(","))
    }
}

impl<'de> Deserialize<'de> for StringSlice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            return Ok(Self::default());
        }
        Ok(Self(value.split(',').map(str::to_owned).collect()))
    }
}
