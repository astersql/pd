// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn base_keyspace_ids() {
    let _: u32 = START_KEYSPACE_ID;
    let _: u32 = DEFAULT_KEYSPACE_ID;
    let _: u32 = MAX_VALID_KEYSPACE_ID;
    let _: u32 = VALID_KEYSPACE_ID_MASK;
    let _: u32 = NULL_KEYSPACE_ID;
    let _: u32 = DEFAULT_KEYSPACE_GROUP_ID;

    assert_eq!(START_KEYSPACE_ID, 0);
    assert_eq!(DEFAULT_KEYSPACE_ID, 0);
    assert_eq!(MAX_VALID_KEYSPACE_ID, 0x00ff_ffff);
    assert_eq!(VALID_KEYSPACE_ID_MASK, 0x00ff_ffff);
    assert_eq!(NULL_KEYSPACE_ID, u32::MAX);
    assert_eq!(DEFAULT_KEYSPACE_GROUP_ID, 0);
}

#[test]
fn reserved_keyspaces_and_labels() {
    let _: u64 = RESERVED_KEYSPACE_ID_COUNT;
    let _: u64 = RESERVED_KEYSPACE_ID_START;
    let _: u32 = SYSTEM_KEYSPACE_ID;

    assert_eq!(DEFAULT_KEYSPACE_NAME, "DEFAULT");
    assert_eq!(REGION_LABEL_ID_PREFIX, "keyspaces/");
    assert_eq!(REGION_LABEL_KEY, "id");
    assert_eq!(RESERVED_KEYSPACE_ID_COUNT, 1024);
    assert_eq!(RESERVED_KEYSPACE_ID_START, 0x00ff_ffff_u64 - 1024);
    assert_eq!(SYSTEM_KEYSPACE_ID, 0x00ff_fffe);
    assert_eq!(SYSTEM_KEYSPACE_NAME, "SYSTEM");
}
