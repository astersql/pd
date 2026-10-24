// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod kind;
pub use kind::*;

#[cfg(test)]
mod kind_test;

mod topn;
pub use topn::{TopN, TopNItem};

#[cfg(test)]
mod topn_test;

mod topn_ttl;
pub use topn_ttl::TtlTopN;

#[cfg(test)]
mod topn_ttl_test;
