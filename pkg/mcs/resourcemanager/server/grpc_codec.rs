// Copyright 2026 AsterSQL.
//! Protobuf conversions at the service/storage boundary only.
use crate::rm_group as d;
use astersql_pd_proto::resource_manager as p;
pub(crate) fn keyspace(id: Option<&p::KeyspaceIdValue>) -> u32 {
    match id {
        None => u32::MAX,
        Some(id) => match id.keyspace {
            Some(p::keyspace_id_value::Keyspace::Value(id)) => id,
            _ => 0,
        },
    }
}
pub(crate) fn keyspace_value(id: u32) -> Option<p::KeyspaceIdValue> {
    Some(p::KeyspaceIdValue {
        keyspace: Some(p::keyspace_id_value::Keyspace::Value(id)),
    })
}
impl From<p::TokenLimitSettings> for d::TokenLimits {
    fn from(v: p::TokenLimitSettings) -> Self {
        Self {
            fill_rate: v.fill_rate,
            burst_limit: v.burst_limit,
            max_tokens: v.max_tokens,
        }
    }
}
impl From<d::TokenLimits> for p::TokenLimitSettings {
    fn from(v: d::TokenLimits) -> Self {
        Self {
            fill_rate: v.fill_rate,
            burst_limit: v.burst_limit,
            max_tokens: v.max_tokens,
        }
    }
}
impl From<p::BackgroundSettings> for d::Background {
    fn from(v: p::BackgroundSettings) -> Self {
        Self {
            job_types: v.job_types,
            utilization_limit: v.utilization_limit,
        }
    }
}
impl From<d::Background> for p::BackgroundSettings {
    fn from(v: d::Background) -> Self {
        Self {
            job_types: v.job_types,
            utilization_limit: v.utilization_limit,
        }
    }
}
impl From<p::RunawayRule> for d::RunawayRule {
    fn from(v: p::RunawayRule) -> Self {
        Self {
            exec_elapsed_time_ms: v.exec_elapsed_time_ms,
            processed_keys: v.processed_keys,
            request_unit: v.request_unit,
        }
    }
}
impl From<d::RunawayRule> for p::RunawayRule {
    fn from(v: d::RunawayRule) -> Self {
        Self {
            exec_elapsed_time_ms: v.exec_elapsed_time_ms,
            processed_keys: v.processed_keys,
            request_unit: v.request_unit,
        }
    }
}
impl From<p::Consumption> for d::Consumption {
    fn from(v: p::Consumption) -> Self {
        Self {
            r_r_u: v.r_r_u,
            w_r_u: v.w_r_u,
            read_bytes: v.read_bytes,
            write_bytes: v.write_bytes,
            total_cpu_time_ms: v.total_cpu_time_ms,
            sql_layer_cpu_time_ms: v.sql_layer_cpu_time_ms,
            kv_read_rpc_count: v.kv_read_rpc_count,
            kv_write_rpc_count: v.kv_write_rpc_count,
            read_cross_az_traffic_bytes: v.read_cross_az_traffic_bytes,
            write_cross_az_traffic_bytes: v.write_cross_az_traffic_bytes,
            tikv_r_u_v2: v.tikv_r_u_v2,
            tidb_r_u_v2: v.tidb_r_u_v2,
            tiflash_r_u_v2: v.tiflash_r_u_v2,
        }
    }
}
impl From<d::Consumption> for p::Consumption {
    fn from(v: d::Consumption) -> Self {
        Self {
            r_r_u: v.r_r_u,
            w_r_u: v.w_r_u,
            read_bytes: v.read_bytes,
            write_bytes: v.write_bytes,
            total_cpu_time_ms: v.total_cpu_time_ms,
            sql_layer_cpu_time_ms: v.sql_layer_cpu_time_ms,
            kv_read_rpc_count: v.kv_read_rpc_count,
            kv_write_rpc_count: v.kv_write_rpc_count,
            read_cross_az_traffic_bytes: v.read_cross_az_traffic_bytes,
            write_cross_az_traffic_bytes: v.write_cross_az_traffic_bytes,
            tikv_r_u_v2: v.tikv_r_u_v2,
            tidb_r_u_v2: v.tidb_r_u_v2,
            tiflash_r_u_v2: v.tiflash_r_u_v2,
        }
    }
}
impl From<p::RunawayWatch> for d::RunawayWatch {
    fn from(v: p::RunawayWatch) -> Self {
        Self {
            lasting_duration_ms: v.lasting_duration_ms,
            kind: v.r#type,
        }
    }
}
impl From<d::RunawayWatch> for p::RunawayWatch {
    fn from(v: d::RunawayWatch) -> Self {
        Self {
            lasting_duration_ms: v.lasting_duration_ms,
            r#type: v.kind,
        }
    }
}
impl From<p::RunawaySettings> for d::Runaway {
    fn from(v: p::RunawaySettings) -> Self {
        Self {
            rule: v.rule.map(Into::into),
            action: v.action,
            watch: v.watch.map(Into::into),
            switch_group_name: v.switch_group_name,
        }
    }
}
impl From<d::Runaway> for p::RunawaySettings {
    fn from(v: d::Runaway) -> Self {
        Self {
            rule: v.rule.map(Into::into),
            action: v.action,
            watch: v.watch.map(Into::into),
            switch_group_name: v.switch_group_name,
        }
    }
}
impl From<p::TokenBucket> for d::Bucket {
    fn from(v: p::TokenBucket) -> Self {
        Self {
            settings: v.settings.map(Into::into),
            tokens: v.tokens,
        }
    }
}
impl From<d::Bucket> for p::TokenBucket {
    fn from(v: d::Bucket) -> Self {
        Self {
            settings: v.settings.map(Into::into),
            tokens: v.tokens,
        }
    }
}
pub(crate) fn decode(v: p::ResourceGroup) -> d::Group {
    d::Group {
        name: v.name,
        mode: v.mode,
        ru_settings_present: v.r_u_settings.is_some(),
        ru: v.r_u_settings.and_then(|v| v.r_u).map(Into::into),
        priority: v.priority,
        runaway: v.runaway_settings.map(Into::into),
        background: v.background_settings.map(Into::into),
        consumption: v.ru_stats.map(Into::into),
    }
}
pub(crate) fn encode(v: &d::Group, id: u32) -> p::ResourceGroup {
    p::ResourceGroup {
        name: v.name.clone(),
        mode: v.mode,
        r_u_settings: (v.mode == 1).then(|| p::GroupRequestUnitSettings {
            r_u: v
                .ru
                .clone()
                .filter(|b| b.settings.is_some())
                .map(Into::into),
        }),
        priority: v.priority,
        runaway_settings: v.runaway.clone().map(Into::into),
        background_settings: v.background.clone().map(Into::into),
        ru_stats: v.consumption.clone().map(Into::into),
        keyspace_id: keyspace_value(id),
        ..Default::default()
    }
}
