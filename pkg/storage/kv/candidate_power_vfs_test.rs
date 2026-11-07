// Copyright 2026 AsterSQL.
//! Single-connection SQLite VFS for explicit write-back and power cut experiments.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use sqlite_vfs::{DatabaseHandle, LockKind, OpenAccess, Vfs, WalDisabled};

const MAX_FILE: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Device {
    cut: bool,
    syncs: Vec<PathBuf>,
}

#[derive(Clone)]
pub(super) struct PowerVfs {
    root: PathBuf,
    device: Arc<Mutex<Device>>,
}

impl PowerVfs {
    pub(super) fn new(root: &Path, name: &str) -> Self {
        let vfs = Self {
            root: root.canonicalize().unwrap(),
            device: Arc::default(),
        };
        sqlite_vfs::register(name, vfs.clone(), false).unwrap();
        vfs
    }

    fn path(&self, name: &str) -> io::Result<PathBuf> {
        let path = Path::new(name);
        let Some(parent) = path.parent() else {
            return Err(io::Error::other("missing image directory"));
        };
        let parent = parent.canonicalize()?;
        let valid_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| matches!(n, "meta.db" | "meta.db-wal" | "meta.db-journal"));
        if !valid_name || parent.parent() != Some(self.root.as_path()) || path.is_symlink() {
            return Err(io::Error::other("invalid experiment image path"));
        }
        Ok(parent.join(path.file_name().unwrap()))
    }

    pub(super) fn cut(&self, source: &Path, destination: &Path) -> io::Result<Vec<PathBuf>> {
        let mut device = self
            .device
            .lock()
            .map_err(|_| io::Error::other("device lock poisoned"))?;
        device.cut = true;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let path = self.path(
                entry
                    .path()
                    .to_str()
                    .ok_or_else(|| io::Error::other("non-UTF8 path"))?,
            )?;
            fs::copy(path, destination.join(entry.file_name()))?;
        }
        Ok(device.syncs.clone())
    }
}

pub(super) struct CachedFile {
    file: File,
    path: PathBuf,
    bytes: Vec<u8>,
    dirty: bool,
    lock: LockKind,
    device: Arc<Mutex<Device>>,
}

fn powered(device: &Mutex<Device>) -> io::Result<MutexGuard<'_, Device>> {
    let state = device
        .lock()
        .map_err(|_| io::Error::other("device lock poisoned"))?;
    if state.cut {
        return Err(io::Error::other("experiment device powered off"));
    }
    Ok(state)
}

impl DatabaseHandle for CachedFile {
    type WalIndex = WalDisabled;

    fn size(&self) -> io::Result<u64> {
        Ok(self.bytes.len() as u64)
    }

    fn read_exact_at(&mut self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        let _power = powered(&self.device)?;
        let offset = usize::try_from(offset).map_err(io::Error::other)?;
        let bytes = self.bytes.get(offset..).unwrap_or_default();
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        buf[count..].fill(0);
        if count < buf.len() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }

    fn write_all_at(&mut self, buf: &[u8], offset: u64) -> io::Result<()> {
        let _power = powered(&self.device)?;
        let start = usize::try_from(offset).map_err(io::Error::other)?;
        let end = start
            .checked_add(buf.len())
            .filter(|&end| end <= MAX_FILE)
            .ok_or_else(|| io::Error::other("image capacity exceeded"))?;
        if end > self.bytes.len() {
            self.bytes.resize(end, 0);
        }
        self.bytes[start..end].copy_from_slice(buf);
        self.dirty = true;
        Ok(())
    }

    fn sync(&mut self, _data_only: bool) -> io::Result<()> {
        let mut device = powered(&self.device)?;
        if self.dirty {
            self.file.seek(SeekFrom::Start(0))?;
            self.file.write_all(&self.bytes)?;
            self.file.set_len(self.bytes.len() as u64)?;
            self.file.sync_all()?;
            self.dirty = false;
            device.syncs.push(self.path.clone());
        }
        Ok(())
    }

    fn set_len(&mut self, size: u64) -> io::Result<()> {
        let _power = powered(&self.device)?;
        let size = usize::try_from(size).map_err(io::Error::other)?;
        if size > MAX_FILE {
            return Err(io::Error::other("image capacity exceeded"));
        }
        self.bytes.resize(size, 0);
        self.dirty = true;
        Ok(())
    }

    fn lock(&mut self, lock: LockKind) -> io::Result<bool> {
        self.lock = lock;
        Ok(true)
    }

    fn reserved(&mut self) -> io::Result<bool> {
        Ok(matches!(
            self.lock,
            LockKind::Reserved | LockKind::Pending | LockKind::Exclusive
        ))
    }

    fn current_lock(&self) -> io::Result<LockKind> {
        Ok(self.lock)
    }

    fn wal_index(&self, _readonly: bool) -> io::Result<Self::WalIndex> {
        Err(io::Error::other(
            "experiment requires exclusive SQLite locking",
        ))
    }
}

impl Vfs for PowerVfs {
    type Handle = CachedFile;

    fn open(&self, name: &str, opts: sqlite_vfs::OpenOptions) -> io::Result<CachedFile> {
        let _power = powered(&self.device)?;
        let path = self.path(name)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(opts.access != OpenAccess::Read)
            .create(opts.access == OpenAccess::Create)
            .create_new(opts.access == OpenAccess::CreateNew)
            .open(&path)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take((MAX_FILE + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE {
            return Err(io::Error::other("image capacity exceeded"));
        }
        Ok(CachedFile {
            file,
            path,
            bytes,
            dirty: false,
            lock: LockKind::None,
            device: self.device.clone(),
        })
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        let _power = powered(&self.device)?;
        fs::remove_file(self.path(name)?)
    }

    fn exists(&self, name: &str) -> io::Result<bool> {
        let _power = powered(&self.device)?;
        self.path(name)?.try_exists()
    }

    fn temporary_name(&self) -> String {
        String::new()
    }

    fn random(&self, buffer: &mut [i8]) {
        let mut bytes = vec![0; buffer.len()];
        getrandom::fill(&mut bytes).expect("experiment entropy");
        for (slot, value) in buffer.iter_mut().zip(bytes) {
            *slot = value as i8;
        }
    }

    fn sleep(&self, duration: Duration) -> Duration {
        std::thread::sleep(duration);
        duration
    }
}
