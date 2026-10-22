// Copyright 2026 AsterSQL.

use super::*;

#[test]
#[cfg(not(feature = "nextgen"))]
fn classic_kernel_default() {
    assert_eq!(KERNEL_TYPE, "Classic");
    assert!(!is_next_gen());
}

#[test]
fn nextgen_kernel_feature() {
    if cfg!(feature = "nextgen") {
        assert_eq!(KERNEL_TYPE, "Next Generation");
        assert!(is_next_gen());
    } else {
        assert_eq!(KERNEL_TYPE, "Classic");
        assert!(!is_next_gen());
    }
}
