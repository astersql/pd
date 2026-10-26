// Copyright 2026 AsterSQL.
//! Startup restoration over the canonical factory, controller and MetaStore.
use astersql_pd_schedule_config::SchedulerConfig;
use astersql_pd_schedule_schedulers::{ControlError, SchedulerController, SchedulerRegistry};
use astersql_pd_schedule_types::{convert_old_str_to_type, scheduler_type_compatible};
use tokio_util::sync::CancellationToken;

/// Per-entry failures are nonfatal, matching Go's skip-and-continue startup policy.
#[derive(Debug, Default)]
pub struct SchedulerRestoreReport {
    pub valid_configs: Vec<SchedulerConfig>,
    pub skipped: Vec<String>,
    pub persistence_error: Option<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum SchedulerRestoreError {
    #[error("scheduler initialization cancelled")]
    Cancelled,
    #[error(transparent)]
    Control(#[from] ControlError),
    #[error("invalid startup scheduler list: {0}")]
    Config(#[from] serde_json::Error),
}
/// The host drives scheduling after initialization; need_run=false retains handlers only.
pub async fn restore_schedulers(
    controller: &mut SchedulerController,
    need_run: bool,
    cancellation: &CancellationToken,
) -> Result<SchedulerRestoreReport, SchedulerRestoreError> {
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().map_err(ControlError::from)?;
    let mut loaded = None;
    for attempt in 0..10 {
        let ctx = controller.context();
        let result = ctx.storage.load_all_scheduler_configs(&ctx.paths).await;
        check_cancelled(cancellation)?;
        match result {
            Ok(configs) => {
                loaded = Some(configs);
                break;
            }
            Err(error) if attempt == 9 => return Err(ControlError::from(error).into()),
            Err(_) => {}
        }
    }
    let list = &controller.configuration()["schedule"]["schedulers-v2"];
    let configs: Vec<SchedulerConfig> = if list.is_null() {
        vec![]
    } else {
        serde_json::from_value(list.clone())?
    };
    let mut report = SchedulerRestoreReport::default();
    for (name, bytes) in loaded.unwrap_or_default() {
        check_cancelled(cancellation)?;
        let Ok(name) = std::str::from_utf8(&name) else {
            report.skipped.push("invalid UTF-8 scheduler name".into());
            continue;
        };
        let Some(typ) = registry.find_type_by_name(name) else {
            report.skipped.push(format!("unknown scheduler: {name}"));
            continue;
        };
        let Some(config) = configs
            .iter()
            .find(|c| Some(c.r#type.as_str()) == scheduler_type_compatible(&typ))
        else {
            report
                .skipped
                .push(format!("scheduler type not configured: {name}"));
            continue;
        };
        if config.disable {
            continue;
        }
        match registry.create_json(&typ, &bytes, controller.context()) {
            Ok(scheduler) => {
                if let Err(error) = controller.add(scheduler, &[], !need_run).await {
                    report.skipped.push(error.to_string());
                }
            }
            Err(error) => report.skipped.push(error.to_string()),
        }
    }
    for config in configs {
        check_cancelled(cancellation)?;
        if config.disable {
            report.valid_configs.push(config);
            continue;
        }
        let Some(typ) = convert_old_str_to_type(&config.r#type) else {
            report
                .skipped
                .push(format!("unknown scheduler type: {}", config.r#type));
            continue;
        };
        let args: Vec<_> = config
            .args
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect();
        let scheduler = match registry.create_args(&typ, &args, controller.context()) {
            Ok(scheduler) => scheduler,
            Err(error) => {
                report.skipped.push(error.to_string());
                continue;
            }
        };
        match controller.add(scheduler, &args, !need_run).await {
            Ok(()) | Err(ControlError::Exists(_)) => report.valid_configs.push(config),
            Err(error) => report.skipped.push(error.to_string()),
        }
    }
    controller.replace_scheduler_configs(
        report
            .valid_configs
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<_, _>>()?,
    );
    let ctx = controller.context();
    report.persistence_error = ctx
        .storage
        .save_config(&ctx.paths, controller.configuration())
        .await
        .err()
        .map(|e| e.to_string());
    Ok(report)
}
fn check_cancelled(cancellation: &CancellationToken) -> Result<(), SchedulerRestoreError> {
    if cancellation.is_cancelled() {
        Err(SchedulerRestoreError::Cancelled)
    } else {
        Ok(())
    }
}
