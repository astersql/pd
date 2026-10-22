// Copyright 2026 AsterSQL.

/// Name of the PD kernel selected by the build configuration.
#[cfg(not(feature = "nextgen"))]
pub const KERNEL_TYPE: &str = "Classic";

/// Name of the PD kernel selected by the build configuration.
#[cfg(feature = "nextgen")]
pub const KERNEL_TYPE: &str = "Next Generation";

/// Reports whether the selected kernel is the next-generation kernel.
#[cfg(not(feature = "nextgen"))]
pub const fn is_next_gen() -> bool {
    false
}

/// Reports whether the selected kernel is the next-generation kernel.
#[cfg(feature = "nextgen")]
pub const fn is_next_gen() -> bool {
    true
}

#[cfg(test)]
mod kerneltype_test;
