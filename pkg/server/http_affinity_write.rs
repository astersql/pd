// Copyright 2026 AsterSQL.
//! Owned write envelope passed to the existing host affinity owner.
pub struct Request {
    pub method: String,
    pub body: axum::body::Body,
}
