// Copyright 2026 AsterSQL.
//! Pass-through SQLite sync instrumentation for the isolated benchmark children.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::ffi;

static ENABLED: OnceLock<bool> = OnceLock::new();
static NATIVE: OnceLock<usize> = OnceLock::new();
static FILES: Mutex<BTreeMap<usize, Box<ProfileFile>>> = Mutex::new(BTreeMap::new());

struct ProfileFile {
    methods: ffi::sqlite3_io_methods,
    native: usize,
    kind: &'static str,
}

pub(super) fn event(phase: &str, detail: impl std::fmt::Display, started: Instant) {
    if *ENABLED.get_or_init(|| std::env::var_os("PD_CANDIDATE_PROFILE").is_some()) {
        let end = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let _ = writeln!(
            io::stderr().lock(),
            "PD_PROFILE {phase} {detail} {} {}",
            end.as_micros(),
            started.elapsed().as_micros()
        );
    }
}

#[cfg(unix)]
pub(super) fn install() {
    if !*ENABLED.get_or_init(|| std::env::var_os("PD_CANDIDATE_PROFILE").is_some()) {
        return;
    }
    // The benchmark child installs one default wrapper before any connection opens.
    // SQLite owns the native descriptor. The wrapper and its name live for the process.
    unsafe {
        let native = ffi::sqlite3_vfs_find(ptr::null());
        assert!(!native.is_null());
        NATIVE.set(native as usize).unwrap();
        let mut wrapper = *native;
        wrapper.pNext = ptr::null_mut();
        wrapper.zName = c"pd-native-sync-profile".as_ptr();
        wrapper.xOpen = Some(open);
        assert_eq!(
            ffi::sqlite3_vfs_register(Box::leak(Box::new(wrapper)), 1),
            ffi::SQLITE_OK
        );
    }
}

unsafe extern "C" fn open(
    _: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: i32,
    out_flags: *mut i32,
) -> i32 {
    // SQLite allocates an unchanged native-sized file and invokes xOpen with valid
    // pointers. The original methods remain callable for the life of this handle.
    unsafe {
        let Some(native) = NATIVE.get().copied() else {
            return ffi::SQLITE_CANTOPEN;
        };
        let native = native as *mut ffi::sqlite3_vfs;
        let Some(open_native) = (*native).xOpen else {
            return ffi::SQLITE_CANTOPEN;
        };
        let result = open_native(native, name, file, flags, out_flags);
        if result != ffi::SQLITE_OK || (*file).pMethods.is_null() {
            return result;
        }
        let original = (*file).pMethods;
        let mut methods = *original;
        if methods.xSync.is_none() || methods.xClose.is_none() {
            return result;
        }
        methods.xSync = Some(sync);
        methods.xClose = Some(close);
        let kind = if flags & ffi::SQLITE_OPEN_WAL != 0 {
            "wal"
        } else if flags & ffi::SQLITE_OPEN_MAIN_DB != 0 {
            "database"
        } else {
            "other"
        };
        let info = Box::new(ProfileFile {
            methods,
            native: original as usize,
            kind,
        });
        let Ok(mut handles) = FILES.lock() else {
            return result;
        };
        (*file).pMethods = &info.methods;
        handles.insert(file as usize, info);
        result
    }
}

unsafe extern "C" fn sync(file: *mut ffi::sqlite3_file, flags: i32) -> i32 {
    let Some((native, kind)) = FILES.lock().ok().and_then(|handles| {
        handles
            .get(&(file as usize))
            .map(|info| (info.native, info.kind))
    }) else {
        return ffi::SQLITE_IOERR_FSYNC;
    };
    let started = Instant::now();
    // SQLite serializes sync and close for this file. The original table is static.
    let result =
        unsafe { (*(native as *const ffi::sqlite3_io_methods)).xSync.unwrap()(file, flags) };
    event("sync", format_args!("{kind}:{flags}:{result}"), started);
    result
}

unsafe extern "C" fn close(file: *mut ffi::sqlite3_file) -> i32 {
    let Some(info) = FILES
        .lock()
        .ok()
        .and_then(|mut handles| handles.remove(&(file as usize)))
    else {
        return ffi::SQLITE_IOERR_CLOSE;
    };
    // The handle is closing and SQLite will not call the wrapper again. Restore
    // the native table before invoking the original close and dropping our table.
    unsafe {
        (*file).pMethods = info.native as *const ffi::sqlite3_io_methods;
        (*(*file).pMethods).xClose.unwrap()(file)
    }
}
