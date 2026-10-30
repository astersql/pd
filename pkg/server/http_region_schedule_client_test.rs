// Copyright 2026 AsterSQL.
use serde_json::Value;
/// Long split observation deadlines require an asynchronous blocking transport.
pub(crate) async fn request(url: &str, path: &str, body: &str) -> (u16, Value) {
    let url = format!("{url}/pd/api/v1/regions/{path}");
    let body = body.to_owned();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "80",
                "-X",
                "POST",
                "--data-binary",
                &body,
                "-w",
                "\n%{http_code}\n%{content_type}",
                &url,
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let (text, content) = text.rsplit_once('\n').unwrap();
        assert!(
            content == "application/json; charset=UTF-8" || content == "text/plain; charset=UTF-8",
            "{content}: {text}"
        );
        let (body, code) = text.rsplit_once('\n').unwrap();
        (
            code.parse().unwrap(),
            serde_json::from_str(body).unwrap_or(Value::String(body.into())),
        )
    })
    .await
    .unwrap()
}
