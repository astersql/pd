// Copyright 2026 AsterSQL.

use std::io::{self, Write};

use crate::BackupInfo;

/// Write the existing backup JSON format, followed by a newline.
pub fn write_backup(mut out: impl Write, backup: &BackupInfo) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut out, backup)?;
    out.write_all(b"\n")
}
