// Copyright 2026 AsterSQL.
#[test]
fn standalone_runtime_lifecycle() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(crate::entry::run_until(async {
            tokio::task::yield_now().await;
            Ok(())
        }))
        .unwrap();
    println!("standalone: create runtime -> start -> ready -> shutdown -> joined");
}

#[tokio::test]
async fn shutdown_signal_error_is_returned() {
    let result =
        crate::entry::run_until(async { Err(std::io::Error::other("signal unavailable")) }).await;
    assert_eq!(result.unwrap_err().to_string(), "signal unavailable");
}
