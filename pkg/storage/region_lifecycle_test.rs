// Copyright 2026 AsterSQL.
use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use astersql_pd_core::RegionMeta;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::StorageEndpoint;

use super::{MemoryStore, MetaStore, RegionLifecycle, RegionLifecycleError, Storage, etcd_fixture};

// Narrow external IO boundary: flush publishes pending bytes, close releases the file.
// No metadata operations or transaction outcomes are mocked.
struct FileLifecycle {
    path: PathBuf,
    file: Mutex<Option<File>>,
    pending: Mutex<Vec<u8>>,
}

impl FileLifecycle {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join(format!(
            "region-lifecycle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self {
            file: Mutex::new(Some(File::create(&path).unwrap())),
            path,
            pending: Mutex::new(vec![]),
        }
    }
}
impl RegionLifecycle for FileLifecycle {
    fn flush(&self) -> Result<(), RegionLifecycleError> {
        let mut file = self.file.lock().unwrap();
        let file = file
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "region file closed"))?;
        let mut pending = self.pending.lock().unwrap();
        file.write_all(&pending)?;
        file.flush()?;
        pending.clear();
        Ok(())
    }
    fn close(&self) -> Result<(), RegionLifecycleError> {
        self.flush()?;
        self.file.lock().unwrap().take();
        Ok(())
    }
}
impl Drop for FileLifecycle {
    fn drop(&mut self) {
        fs::remove_file(&self.path).unwrap();
    }
}

async fn contract(default: Arc<dyn MetaStore>, region: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(57);
    let meta = RegionMeta {
        id: 57,
        ..Default::default()
    };
    let lifecycle = Arc::new(FileLifecycle::new());
    let storage = Storage::new_core_with_lifecycle(
        Storage::new(default.clone()),
        StorageEndpoint::new(region.clone()),
        lifecycle.clone(),
    );
    // Default selected: Flush must still reach the injected boundary.
    lifecycle
        .pending
        .lock()
        .unwrap()
        .extend_from_slice(b"default");
    storage.flush().unwrap();
    assert_eq!(fs::read(&lifecycle.path).unwrap(), b"default");
    assert!(lifecycle.pending.lock().unwrap().is_empty());
    storage.flush().unwrap();
    assert_eq!(fs::read(&lifecycle.path).unwrap(), b"default");

    storage.try_switch_region_storage(true).unwrap();
    storage.save_region(&paths, &meta).await.unwrap();
    lifecycle
        .pending
        .lock()
        .unwrap()
        .extend_from_slice(b"/local");
    storage.flush().unwrap();
    assert_eq!(fs::read(&lifecycle.path).unwrap(), b"default/local");
    // Close after switching back still closes the injected resource and flushes its tail.
    storage.try_switch_region_storage(false).unwrap();
    lifecycle
        .pending
        .lock()
        .unwrap()
        .extend_from_slice(b"/close");
    storage.close().unwrap();
    assert!(lifecycle.file.lock().unwrap().is_none());
    assert_eq!(fs::read(&lifecycle.path).unwrap(), b"default/local/close");
    for local in [false, true, false] {
        storage.try_switch_region_storage(local).unwrap();
        for error in [storage.flush().unwrap_err(), storage.close().unwrap_err()] {
            let error = error.downcast::<io::Error>().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(error.to_string(), "region file closed");
        }
    }
    // Closing never clears the source selection or closes either host-owned MetaStore.
    assert!(!storage.are_regions_loaded());
    storage.save_region(&paths, &meta).await.unwrap();
    assert_eq!(
        StorageEndpoint::new(default.clone())
            .load_region(&paths, 57)
            .await
            .unwrap(),
        Some(meta.clone())
    );
    assert_eq!(
        StorageEndpoint::new(region.clone())
            .load_region(&paths, 57)
            .await
            .unwrap(),
        Some(meta)
    );
    storage
        .base()
        .save(b"/lifecycle/alive", b"yes")
        .await
        .unwrap();
    assert_eq!(
        storage.base().load(b"/lifecycle/alive").await.unwrap(),
        b"yes"
    );

    // Plain and ordinary core endpoints are no-ops, including repeated calls.
    for plain in [
        Storage::new(default.clone()),
        Storage::new_core(
            Storage::new(default.clone()),
            StorageEndpoint::new(region.clone()),
        ),
    ] {
        for _ in 0..2 {
            plain.flush().unwrap();
            plain.close().unwrap();
        }
        plain
            .base()
            .save(b"/lifecycle/alive", b"still")
            .await
            .unwrap();
        assert_eq!(
            plain.base().load(b"/lifecycle/alive").await.unwrap(),
            b"still"
        );
    }
    // Reattaching a normal endpoint must not retain a previous resource's lifecycle.
    let replaced = Storage::new_core(storage, StorageEndpoint::new(region));
    replaced.flush().unwrap();
    replaced.close().unwrap();
}

#[tokio::test]
async fn region_lifecycle_memory() {
    contract(
        Arc::new(MemoryStore::default()),
        Arc::new(MemoryStore::default()),
    )
    .await;
}
#[tokio::test]
async fn region_lifecycle_etcd() {
    let default = etcd_fixture::Fixture::start();
    let region = etcd_fixture::Fixture::start();
    contract(
        Arc::new(default.store().await),
        Arc::new(region.store().await),
    )
    .await;
}
