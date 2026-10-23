// Copyright 2026 AsterSQL.
use crate::*;

pub(crate) fn request(key: &[u8], end: &[u8], limit: usize) -> Read {
    let mut read = Read::range(key, end, Consistency::Linearizable);
    read.limit = limit;
    read
}
