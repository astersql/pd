// Copyright 2026 AsterSQL.

/// Name of the classic PD kernel selected by the default build.
pub const KERNEL_TYPE: &str = "Classic";

/// Reports whether the selected kernel is the next-generation kernel.
pub const fn is_next_gen() -> bool {
    false
}

#[cfg(test)]
mod kerneltype_test;
