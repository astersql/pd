// Copyright 2026 AsterSQL.
//! Local diagnostics log reader, matching the pinned sysutil SearchLog iterator.
//! File IO runs on the caller's blocking pool; the bounded stream owns cancellation.
use astersql_pd_proto::diagnosticspb::{
    LogLevel, LogMessage, SearchLogRequest, SearchLogResponse, diagnostics_server::Diagnostics,
};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

pub(crate) struct DiagnosticsLog {
    pub services: Arc<dyn crate::PdServices>,
    pub cancel: CancellationToken,
}

pub(crate) struct LogStream {
    receiver: ReceiverStream<Result<SearchLogResponse, Status>>,
    cancel: CancellationToken,
    done: bool,
}
impl Stream for LogStream {
    type Item = Result<SearchLogResponse, Status>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        if self.cancel.is_cancelled() {
            self.done = true;
            return Poll::Ready(Some(Err(Status::cancelled("context canceled"))));
        }
        let result = Pin::new(&mut self.receiver).poll_next(cx);
        if matches!(result, Poll::Ready(None)) {
            self.done = true;
        }
        result
    }
}
impl Drop for LogStream {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[tonic::async_trait]
impl Diagnostics for DiagnosticsLog {
    async fn search_log(
        &self,
        request: Request<SearchLogRequest>,
    ) -> Result<Response<tonic::codegen::BoxStream<SearchLogResponse>>, Status> {
        let path = self
            .services
            .telemetry_config()
            .map(|c| c.log.file.filename.clone())
            .unwrap_or_default();
        let cancel = self.cancel.child_token();
        let worker_cancel = cancel.clone();
        let request = request.into_inner();
        let (tx, receiver) = mpsc::channel(1);
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let send = |batch| {
                runtime.block_on(async {
                    tokio::select! { biased;
                        _ = worker_cancel.cancelled() => false,
                        result = tx.send(batch) => result.is_ok(),
                    }
                })
            };
            if let Err(error) = search(&path, request, &worker_cancel, &send) {
                let _ = send(Err(error));
            }
        });
        Ok(Response::new(Box::pin(LogStream {
            receiver: ReceiverStream::new(receiver),
            cancel,
            done: false,
        })))
    }
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), Status> {
    if cancel.is_cancelled() {
        Err(Status::cancelled("context canceled"))
    } else {
        Ok(())
    }
}
fn io_error(error: std::io::Error) -> Status {
    Status::unknown(error.to_string())
}
fn line(reader: &mut impl BufRead) -> Result<Option<String>, Status> {
    let mut bytes = Vec::new();
    if reader.read_until(b'\n', &mut bytes).map_err(io_error)? == 0 {
        return Ok(None);
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn parse(line: &str) -> Option<LogMessage> {
    let left = line.find('[')?;
    let right = line.find(']')?;
    if left > right {
        return None;
    }
    let stamp = &line[left + 1..right];
    // Chrono permits unpadded months/days; Go's 2006/01/02 layout does not.
    let date = stamp.split(' ').next()?.as_bytes();
    if date.len() != 10
        || date[4] != b'/'
        || date[7] != b'/'
        || !date
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return None;
    }
    let time = chrono::DateTime::parse_from_str(stamp, "%Y/%m/%d %H:%M:%S%.3f %:z")
        .ok()?
        .timestamp_millis();
    let rest = &line[right + 1..];
    let left = rest.find('[')?;
    let right = rest.find(']')?;
    if left > right {
        return None;
    }
    let level = match &rest[left + 1..right] {
        "debug" | "DEBUG" => LogLevel::Debug,
        "info" | "INFO" => LogLevel::Info,
        "warn" | "WARN" => LogLevel::Warn,
        "trace" | "TRACE" => LogLevel::Trace,
        "critical" | "CRITICAL" => LogLevel::Critical,
        "error" | "ERROR" => LogLevel::Error,
        _ => LogLevel::Unknown,
    } as i32;
    Some(LogMessage {
        time,
        level,
        message: rest[right + 1..].trim().into(),
    })
}

// sysutil probes the first ten physical lines and reverse windows of at least
// 512 bytes at the tail. Preserve that window rule rather than accepting every
// file with a timestamp somewhere in it.
fn last_valid(file: &mut File, cancel: &CancellationToken) -> Result<Option<LogMessage>, Status> {
    let mut end = file.metadata().map_err(io_error)?.len();
    let mut tried = 0;
    while end > 0 {
        let mut cursor = end;
        let mut size = 256;
        let mut bytes = Vec::new();
        let mut start = 0;
        while cursor > 0 {
            check_cancel(cancel)?;
            size = (size * 2).min(16 * 1024 * 1024).min(cursor);
            cursor -= size;
            file.seek(SeekFrom::Start(cursor)).map_err(io_error)?;
            let mut chunk = vec![0; size as usize];
            file.read_exact(&mut chunk).map_err(io_error)?;
            for i in 0..chunk.len().saturating_sub(1) {
                if matches!(chunk[i], b'\r' | b'\n') && !matches!(chunk[i + 1], b'\r' | b'\n') {
                    start = i + 1;
                    break;
                }
            }
            chunk.extend(bytes);
            bytes = chunk;
            if start > 0 {
                break;
            }
        }
        let tail = String::from_utf8_lossy(&bytes[start..]).replace("\r\n", "\n");
        let lines: Vec<_> = tail.split('\n').collect();
        for line in lines.iter().rev() {
            if let Some(item) = parse(line) {
                return Ok(Some(item));
            }
        }
        tried += lines.len();
        if tried >= 10 {
            break;
        }
        let read = bytes.len() - start;
        if read == 0 {
            break;
        }
        end -= read as u64;
    }
    Ok(None)
}

fn resolve(
    path: &str,
    begin: i64,
    end: i64,
    cancel: &CancellationToken,
) -> Result<Vec<(i64, File)>, Status> {
    if path.is_empty() {
        return Err(Status::unknown("empty log file location configuration"));
    }
    let path = Path::new(path);
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    // filepath.Ext treats a leading dot as an extension too.
    let extension = name.rfind('.').map(|index| &name[index..]).unwrap_or("");
    let prefix = &name[..name.len() - extension.len()];
    let mut entries = std::fs::read_dir(directory)
        .map_err(|e| Status::unknown(format!("open {}: {}", directory.display(), go_io_error(e))))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    entries.sort_by_key(|e| e.file_name());
    let mut files = Vec::new();
    for entry in entries {
        check_cancel(cancel)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(prefix)
            || !name.ends_with(extension)
            || entry.file_type().map_err(io_error)?.is_dir()
        {
            continue;
        }
        let Ok(mut file) = File::open(entry.path()) else {
            continue;
        };
        let first = {
            let mut reader = BufReader::new(&mut file);
            let mut first = None;
            for _ in 0..10 {
                check_cancel(cancel)?;
                let Ok(Some(line)) = line(&mut reader) else {
                    break;
                };
                first = parse(&line);
                if first.is_some() {
                    break;
                }
            }
            first
        };
        let Some(first) = first else {
            continue;
        };
        let Ok(Some(last)) = last_valid(&mut file, cancel) else {
            continue;
        };
        if begin > last.time || end < first.time {
            continue;
        }
        if file.seek(SeekFrom::Start(0)).is_err() {
            continue;
        }
        files.push((first.time, file));
    }
    files.sort_by_key(|(begin, _)| *begin);
    Ok(files)
}
fn go_io_error(error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".into(),
        std::io::ErrorKind::PermissionDenied => "permission denied".into(),
        _ => error.to_string(),
    }
}

fn search(
    path: &str,
    request: SearchLogRequest,
    cancel: &CancellationToken,
    send: &impl Fn(Result<SearchLogResponse, Status>) -> bool,
) -> Result<(), Status> {
    let end = if request.end_time == 0 {
        i64::MAX
    } else {
        request.end_time
    };
    let files = resolve(path, request.start_time, end, cancel)?;
    let patterns = request
        .patterns
        .iter()
        .map(|p| crate::http_store_read::go_pattern(p).map_err(Status::unknown))
        .collect::<Result<Vec<_>, _>>()?;
    let levels = request.levels.iter().fold(0u64, |mask, level| {
        // Go's int64 shift discards bits >=64 and panics on a negative shift.
        mask | if (0..64).contains(level) {
            1u64 << *level
        } else {
            0
        }
    });
    let mut previous: Option<LogMessage> = None;
    let mut messages = Vec::with_capacity(1024);
    'files: for (_, file) in files {
        let mut reader = BufReader::new(file);
        loop {
            check_cancel(cancel)?;
            let Some(line) = line(&mut reader)? else {
                break;
            };
            let line = line.trim();
            if previous.is_none() && line.len() < 30 {
                continue;
            }
            let item = match parse(line) {
                Some(item) => {
                    previous = Some(item.clone());
                    item
                }
                None => match &previous {
                    Some(previous) => LogMessage {
                        time: previous.time,
                        level: previous.level,
                        message: line.into(),
                    },
                    None => continue,
                },
            };
            if item.time > end {
                break 'files;
            }
            if item.time < request.start_time {
                continue;
            }
            if item.level > 0 && levels != 0 && levels & (1u64 << item.level) == 0 {
                continue;
            }
            if !patterns
                .iter()
                .all(|pattern| pattern.is_match(&item.message))
            {
                continue;
            }
            messages.push(item);
            if messages.len() == 1024
                && !send(Ok(SearchLogResponse {
                    messages: std::mem::replace(&mut messages, Vec::with_capacity(1024)),
                }))
            {
                return Ok(());
            }
        }
    }
    send(Ok(SearchLogResponse { messages }));
    Ok(())
}

#[cfg(test)]
#[path = "diagnostics_log_test.rs"]
mod tests;
