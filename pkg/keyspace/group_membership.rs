// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, KeyspaceGroupManager,
    group_catalog::{commit_error, put_index},
};
use astersql_pd_storage_domain::{Base, KeyspaceGroup, StorageEndpoint, UserKind};
use std::convert::Infallible;

/// Failures from validating or atomically moving persisted group membership.
#[derive(Debug, thiserror::Error)]
pub enum GroupMembershipError {
    #[error(transparent)]
    Catalog(#[from] GroupCatalogError),
    #[error("{0}")]
    Validation(String),
}
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
fn parse_group_id(input: &str) -> Result<u64, GroupMembershipError> {
    let invalid = |reason| {
        GroupMembershipError::Validation(format!(
            "strconv.ParseUint: parsing {}: {reason}",
            go_quote(input)
        ))
    };
    if input.is_empty() {
        return Err(invalid("invalid syntax"));
    }
    let mut value = 0_u64;
    for digit in input.bytes() {
        if !digit.is_ascii_digit() {
            return Err(invalid("invalid syntax"));
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(digit - b'0')))
            .ok_or_else(|| invalid("value out of range"))?;
    }
    Ok(value)
}
fn validate_kind(
    group: Option<KeyspaceGroup>,
    id: &str,
    kind: UserKind,
) -> Result<KeyspaceGroup, GroupMembershipError> {
    group
        .filter(|g| UserKind::from_name(&g.user_kind) == kind)
        .ok_or_else(|| {
            GroupMembershipError::Validation(format!(
                "keyspace group {id} not found in {kind} group"
            ))
        })
}
fn add(group: &mut KeyspaceGroup, id: u32) {
    let keys = group.keyspaces.get_or_insert_with(Vec::new);
    if !keys.contains(&id) {
        keys.push(id);
        keys.sort_unstable();
    }
}
fn remove(group: &mut KeyspaceGroup, id: u32) {
    if let Some(keys) = &mut group.keyspaces {
        keys.retain(|key| *key != id);
    }
}
impl KeyspaceGroupManager {
    /// Move a keyspace using current persisted groups, publishing only after commit.
    /// As in Go, equal group IDs add then remove the keyspace from that one group.
    pub async fn update_keyspace_group(
        &self,
        old_group_id: &str,
        new_group_id: &str,
        old_kind: UserKind,
        new_kind: UserKind,
        keyspace_id: u32,
    ) -> Result<(), GroupMembershipError> {
        let _membership = self.membership_mutex.lock().await;
        self.update_keyspace_group_locked(
            old_group_id,
            new_group_id,
            old_kind,
            new_kind,
            keyspace_id,
        )
        .await
    }
    pub(crate) async fn update_keyspace_group_locked(
        &self,
        old_group_id: &str,
        new_group_id: &str,
        old_kind: UserKind,
        new_kind: UserKind,
        keyspace_id: u32,
    ) -> Result<(), GroupMembershipError> {
        let old_id = parse_group_id(old_group_id)?;
        let new_id = parse_group_id(new_group_id)?;
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut old = validate_kind(
            endpoint
                .load_keyspace_group(&mut txn, &self.paths, old_id as u32)
                .await
                .map_err(GroupCatalogError::from)?,
            old_group_id,
            old_kind,
        )?;
        let mut new = if old_id == new_id {
            old.clone()
        } else {
            validate_kind(
                endpoint
                    .load_keyspace_group(&mut txn, &self.paths, new_id as u32)
                    .await
                    .map_err(GroupCatalogError::from)?,
                new_group_id,
                new_kind,
            )?
        };
        // The equal-ID branch also checks the requested destination kind.
        if UserKind::from_name(&new.user_kind) != new_kind {
            return Err(GroupMembershipError::Validation(format!(
                "keyspace group {new_group_id} not found in {new_kind} group"
            )));
        }
        for group in [&old, &new] {
            if group.is_splitting() {
                return Err(GroupCatalogError::InSplit(group.id).into());
            }
        }
        for group in [&old, &new] {
            if group.is_merging() {
                return Err(GroupMembershipError::Validation(format!(
                    "[PD:keyspace:ErrKeyspaceGroupInMerging]keyspace group {} is in merging state",
                    group.id
                )));
            }
        }
        if old_id == new_id {
            add(&mut old, keyspace_id);
            remove(&mut old, keyspace_id);
        } else {
            add(&mut new, keyspace_id);
            remove(&mut old, keyspace_id);
        }
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &old)
            .map_err(GroupCatalogError::from)?;
        if old_id != new_id {
            endpoint
                .save_keyspace_group(&mut txn, &self.paths, &new)
                .map_err(GroupCatalogError::from)?;
        }
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        put_index(&mut index, old);
        if old_id != new_id {
            put_index(&mut index, new);
        }
        Ok(())
    }
}
