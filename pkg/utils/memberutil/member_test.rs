// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn stable_sha256_prefix_and_byte_order() {
    assert_eq!(generate_unique_id(""), 0x141cfc9842c4b0e3);
    assert_eq!(generate_unique_id("abc"), 0xeacf018fbf1678ba);
    assert_ne!(generate_unique_id("node"), generate_unique_id("Node"));
}
