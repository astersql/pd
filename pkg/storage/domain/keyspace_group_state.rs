// Copyright 2026 AsterSQL.

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::KeyspaceGroupMember;

/// Split source ID, including the source group's own ID while splitting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SplitState {
    #[serde(rename = "split-source")]
    pub split_source: u32,
}

/// Ordered merging source IDs; None preserves a Go nil slice as JSON null.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MergeState {
    #[serde(rename = "merge-list")]
    pub merge_list: Option<Vec<u32>>,
}

/// Keyspace group value. The lookup table is local and is never persisted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct KeyspaceGroup {
    pub id: u32,
    #[serde(rename = "user-kind")]
    pub user_kind: String,
    #[serde(rename = "split-state", skip_serializing_if = "Option::is_none")]
    pub split_state: Option<SplitState>,
    #[serde(rename = "merge-state", skip_serializing_if = "Option::is_none")]
    pub merge_state: Option<MergeState>,
    pub members: Option<Vec<KeyspaceGroupMember>>,
    pub keyspaces: Option<Vec<u32>>,
    #[serde(skip)]
    pub keyspace_lookup_table: HashSet<u32>,
}

impl KeyspaceGroup {
    /// Whether a split state exists, even when the source ID is zero.
    pub fn is_splitting(&self) -> bool {
        self.split_state.is_some()
    }
    /// Whether this group is a splitting source.
    pub fn is_split_source(&self) -> bool {
        self.split_state
            .as_ref()
            .is_some_and(|state| state.split_source == self.id)
    }
    /// Whether this group is a splitting target.
    pub fn is_split_target(&self) -> bool {
        self.split_state
            .as_ref()
            .is_some_and(|state| state.split_source != self.id)
    }
    /// Split source ID, or zero when there is no split state.
    pub fn split_source(&self) -> u32 {
        self.split_state
            .as_ref()
            .map_or(0, |state| state.split_source)
    }
    /// Whether a merge state exists, including an empty source list.
    pub fn is_merging(&self) -> bool {
        self.merge_state.is_some()
    }
    /// Whether the merge list contains this group's ID.
    pub fn is_merge_source(&self) -> bool {
        self.merge_state
            .as_ref()
            .and_then(|state| state.merge_list.as_ref())
            .is_some_and(|ids| ids.contains(&self.id))
    }
    /// Whether this is a merging group absent from the source list.
    pub fn is_merge_target(&self) -> bool {
        self.is_merging() && !self.is_merge_source()
    }
}

/// Nil-safe Go state queries on an optional borrowed group.
pub trait OptionalKeyspaceGroupState {
    fn is_splitting(&self) -> bool;
    fn is_split_source(&self) -> bool;
    fn is_split_target(&self) -> bool;
    fn split_source(&self) -> u32;
    fn is_merging(&self) -> bool;
    fn is_merge_source(&self) -> bool;
    fn is_merge_target(&self) -> bool;
}

impl OptionalKeyspaceGroupState for Option<&KeyspaceGroup> {
    fn is_splitting(&self) -> bool {
        self.is_some_and(KeyspaceGroup::is_splitting)
    }
    fn is_split_source(&self) -> bool {
        self.is_some_and(KeyspaceGroup::is_split_source)
    }
    fn is_split_target(&self) -> bool {
        self.is_some_and(KeyspaceGroup::is_split_target)
    }
    fn split_source(&self) -> u32 {
        self.map_or(0, KeyspaceGroup::split_source)
    }
    fn is_merging(&self) -> bool {
        self.is_some_and(KeyspaceGroup::is_merging)
    }
    fn is_merge_source(&self) -> bool {
        self.is_some_and(KeyspaceGroup::is_merge_source)
    }
    fn is_merge_target(&self) -> bool {
        self.is_some_and(KeyspaceGroup::is_merge_target)
    }
}

impl<'de> Deserialize<'de> for SplitState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        de::DeserializeSeed::deserialize(Self::default(), deserializer)
    }
}

impl<'de> de::DeserializeSeed<'de> for SplitState {
    type Value = Self;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor(SplitState);
        impl<'de> de::Visitor<'de> for ValueVisitor {
            type Value = SplitState;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a SplitState object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(self.0)
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = self.0;
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("split-source") {
                        if let Some(field) = map.next_value::<Option<u32>>()? {
                            value.split_source = field;
                        }
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(value)
            }
        }
        deserializer.deserialize_any(ValueVisitor(self))
    }
}

impl<'de> Deserialize<'de> for MergeState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        de::DeserializeSeed::deserialize(Self::default(), deserializer)
    }
}

impl<'de> de::DeserializeSeed<'de> for MergeState {
    type Value = Self;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor(MergeState);
        impl<'de> de::Visitor<'de> for ValueVisitor {
            type Value = MergeState;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a MergeState object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(self.0)
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = self.0;
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("merge-list") {
                        value.merge_list = map.next_value::<Option<GoIds>>()?.map(|ids| ids.0);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(value)
            }
        }
        deserializer.deserialize_any(ValueVisitor(self))
    }
}

impl<'de> Deserialize<'de> for KeyspaceGroup {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> de::Visitor<'de> for ValueVisitor {
            type Value = KeyspaceGroup;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a KeyspaceGroup object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(KeyspaceGroup::default())
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = KeyspaceGroup::default();
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("id") {
                        if let Some(field) = map.next_value::<Option<u32>>()? {
                            value.id = field;
                        }
                    } else if key.eq_ignore_ascii_case("user-kind") {
                        if let Some(field) = map.next_value::<Option<String>>()? {
                            value.user_kind = field;
                        }
                    } else if key.eq_ignore_ascii_case("split-state") {
                        value.split_state =
                            map.next_value_seed(OptionalStateSeed(value.split_state.take()))?;
                    } else if key.eq_ignore_ascii_case("merge-state") {
                        value.merge_state =
                            map.next_value_seed(OptionalStateSeed(value.merge_state.take()))?;
                    } else if key.eq_ignore_ascii_case("members") {
                        value.members = map.next_value::<Option<Vec<KeyspaceGroupMember>>>()?;
                    } else if key.eq_ignore_ascii_case("keyspaces") {
                        value.keyspaces = map.next_value::<Option<GoIds>>()?.map(|ids| ids.0);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(value)
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

// Go reuses an existing pointed-to struct when another object updates the same
// JSON field; null clears the pointer. In particular, {} must not erase a source.
struct OptionalStateSeed<T>(Option<T>);
impl<'de, T> de::DeserializeSeed<'de> for OptionalStateSeed<T>
where
    T: Default + de::DeserializeSeed<'de, Value = T>,
{
    type Value = Option<T>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_option(self)
    }
}

impl<'de, T> de::Visitor<'de> for OptionalStateSeed<T>
where
    T: Default + de::DeserializeSeed<'de, Value = T>,
{
    type Value = Option<T>;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an optional state object")
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.unwrap_or_default().deserialize(d).map(Some)
    }
}

// Go accepts null array elements for numeric slices, leaving the zero value.
struct GoIds(Vec<u32>);
impl<'de> Deserialize<'de> for GoIds {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<Option<u32>>::deserialize(d)
            .map(|ids| Self(ids.into_iter().map(Option::unwrap_or_default).collect()))
    }
}
