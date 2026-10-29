// Copyright 2026 AsterSQL.
//! Host-side refill and error mapping for the shared classic Oracle.
use astersql_pd_member::{Leadership, MemberError};
use astersql_pd_tso::{Oracle, Timestamp, TsoError};
pub(crate) fn generation_error(reason: &str) -> String {
    format!("[PD:tso:ErrGenerateTimestamp]generate timestamp failed, {reason}")
}
pub(crate) async fn allocate(
    oracle: &Oracle,
    leader: &Leadership,
    count: u32,
) -> Result<Timestamp, String> {
    leader.validate().await.map_err(|error| match error {
        MemberError::NotLeader => generation_error("requested pd is not leader of cluster"),
        error => generation_error(&error.to_string()),
    })?;
    if count == 0 {
        return Err(generation_error("tso count should be positive"));
    }
    if count >= astersql_pd_tso::MAX_LOGICAL {
        return Err(generation_error(
            "generate tso maximum number of retries exceeded",
        ));
    }
    for _ in 0..10 {
        match oracle.allocate(count).await {
            Ok(timestamp) => return Ok(timestamp),
            Err(TsoError::WindowExhausted) => {
                // The canonical oracle extends its fenced durable window. No
                // private timer or worker is introduced by the request path.
                oracle.update().await.map_err(map_error)?;
            }
            Err(error) => return Err(map_error(error)),
        }
    }
    Err(generation_error(
        "generate tso maximum number of retries exceeded",
    ))
}
fn map_error(error: TsoError) -> String {
    match error {
        TsoError::Member(MemberError::NotLeader) => {
            generation_error("requested is not leader anymore")
        }
        error => generation_error(&error.to_string()),
    }
}
