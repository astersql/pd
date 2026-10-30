// Copyright 2026 AsterSQL.
//! Host-owned wall-clock projection of canonical operator observations.
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_server::http_operator_read::Query;
use serde_json::{Value, json};
use std::time::{Duration, Instant, SystemTime};
#[derive(Clone)]
pub(super) struct ClockMap {
    monotonic: Instant,
    wall: SystemTime,
    zone: Option<chrono_tz::Tz>,
}
impl ClockMap {
    pub(super) fn new() -> Self {
        Self {
            monotonic: Instant::now(),
            wall: SystemTime::now(),
            zone: std::env::var("TZ")
                .ok()
                .or_else(|| iana_time_zone::get_timezone().ok())
                .and_then(|s| s.parse().ok()),
        }
    }
    fn wall(&self, t: Instant) -> SystemTime {
        if t >= self.monotonic {
            self.wall + (t - self.monotonic)
        } else {
            self.wall - (self.monotonic - t)
        }
    }
    fn render(&self, t: Option<Instant>) -> String {
        let Some(t) = t else {
            return "0001-01-01 00:00:00 +0000 UTC".into();
        };
        let date: chrono::DateTime<chrono::Utc> = self.wall(t).into();
        let date = if let Some(zone) = self.zone {
            date.with_timezone(&zone)
                .format("%Y-%m-%d %H:%M:%S%.f %z %Z")
                .to_string()
        } else {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S%.f %z %Z")
                .to_string()
        };
        let date = if let Some((head, tail)) = date.split_once('.') {
            let (fraction, zone) = tail.split_once(' ').expect("formatted time zone");
            format!("{}.{} {}", head, fraction.trim_end_matches('0'), zone)
        } else {
            date
        };
        let (sign, elapsed) = if t >= self.monotonic {
            ('+', t - self.monotonic)
        } else {
            ('-', self.monotonic - t)
        };
        format!(
            "{} m={}{}.{:09}",
            date,
            sign,
            elapsed.as_secs(),
            elapsed.subsec_nanos()
        )
    }
    fn seconds(&self, t: Instant) -> i128 {
        match self.wall(t).duration_since(SystemTime::UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i128,
            Err(e) => {
                -(e.duration().as_secs() as i128) - i128::from(e.duration().subsec_nanos() != 0)
            }
        }
    }
}
pub(super) fn read(
    coordinator: &Coordinator<ProviderSnapshot<()>>,
    query: Query,
    clock: &ClockMap,
) -> Result<Value, String> {
    let handler = astersql_pd_schedule_handler::Handler::new(Some(coordinator));
    let now = tokio::time::Instant::now().into_std();
    let error = |e: astersql_pd_schedule_handler::HandlerError| format!("[{}]{}", e.code(), e);
    let time = |t| clock.render(t);
    match query {
        Query::List { kinds, object } => {
            let filtered = kinds.is_some();
            let ops = if let Some(kinds) = kinds {
                handler
                    .get_operators_by_kinds(&kinds.iter().map(String::as_str).collect::<Vec<_>>())
            } else {
                handler.get_operators()
            }
            .map_err(error)?;
            if ops.is_empty() && filtered && !object {
                return Ok(Value::Null);
            }
            Ok(Value::Array(
                ops.iter()
                    .map(|op| {
                        if object {
                            op.to_json_object()
                        } else {
                            json!(op.render(&time))
                        }
                    })
                    .collect(),
            ))
        }
        Query::Region(id) => {
            let record = handler.get_operator_status(id, now).map_err(error)?;
            let status = match record.status.0 {
                1 => "RUNNING",
                2 => "SUCCESS",
                3 => "CANCEL",
                4 => "REPLACE",
                5 | 6 => "TIMEOUT",
                _ => "5",
            };
            Ok(json!(format!(
                "status: {status}, operator: {}",
                record.operator.render(&time)
            )))
        }
        Query::Records(from) => {
            let records = handler
                .get_records(now - Duration::from_secs(601), now)
                .map_err(error)?;
            // Go time.Unix stores seconds since year 1 with signed wrapping.
            // Its Before comparison uses that representation, even at i64::MAX.
            let records: Vec<_> = records
                .iter()
                .filter(|r| {
                    from.is_none_or(|s| {
                        (clock.seconds(r.finish_time) as i64).wrapping_add(62_135_596_800)
                            >= s.wrapping_add(62_135_596_800)
                    })
                })
                .collect();
            if records.is_empty() {
                return Err("[PD:operator:ErrOperatorNotFound]operator not found".into());
            }
            Ok(Value::Array(
                records
                    .iter()
                    .map(|r| {
                        json!(
                            r.record()
                                .with_zero_start_duration(i64::MAX)
                                .render(&time)
                                .expect("duration resolved")
                        )
                    })
                    .collect(),
            ))
        }
    }
}

#[cfg(test)]
#[path = "schedule_operator_read_test.rs"]
mod tests;
