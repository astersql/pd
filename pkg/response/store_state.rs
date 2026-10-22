// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreResponseConfig, build_store_info};
use astersql_pd_core::StoreInfo as CoreStore;
/// Build a store response at a caller-supplied Unix nanosecond instant.
pub fn build_store_info_at(
    opt: &StoreResponseConfig,
    store: &CoreStore,
    now_unix_nanos: i128,
    max_store_down_time_nanos: i64,
) -> StoreInfo {
    let mut response = build_store_info(opt, store);
    if store.state() == astersql_pd_core::StoreState::Up {
        let name = if store.down_time_nanos(now_unix_nanos) > max_store_down_time_nanos {
            "Down"
        } else if store.is_disconnected(now_unix_nanos) {
            "Disconnected"
        } else {
            "Up"
        };
        response
            .store
            .as_mut()
            .expect("builder supplies metadata")
            .state_name = name.into();
    }
    response
}
