// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn classic_kernel_default() {
    assert_eq!(KERNEL_TYPE, "Classic");
    assert!(!is_next_gen());
}
