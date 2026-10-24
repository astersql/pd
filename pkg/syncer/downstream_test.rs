// Copyright 2026 AsterSQL.
use super::{
    DownstreamError, DownstreamSender, DownstreamTask, Downstreams, HistoryBuffer,
    HistoryIndex as I, SyncFrame,
};
use astersql_pd_core::{RegionInfo, RegionMeta};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    runtime::Handle,
    sync::{Notify, Semaphore},
    time::{Duration, timeout},
};

type Frames = Arc<Mutex<Vec<(u64, Vec<u64>)>>>;
struct Sender {
    frames: Frames,
    gate: Option<Arc<Semaphore>>,
    entered: Arc<Notify>,
    fail: bool,
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
}
struct InFlight(Arc<AtomicUsize>);
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl DownstreamSender for Sender {
    async fn send(&mut self, frame: SyncFrame) -> Result<(), String> {
        let n = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        let _flight = InFlight(self.active.clone());
        self.maximum.fetch_max(n, Ordering::SeqCst);
        self.entered.notify_one();
        if let Some(gate) = self.gate.take() {
            gate.acquire().await.unwrap().forget();
        }
        if self.fail {
            return Err("send failed".into());
        }
        self.frames.lock().unwrap().push((
            frame.start_index().0,
            frame.records().iter().map(|r| r.meta().id).collect(),
        ));
        Ok(())
    }
}
fn sender() -> Sender {
    Sender {
        frames: Arc::default(),
        gate: None,
        entered: Arc::default(),
        fail: false,
        active: Arc::default(),
        maximum: Arc::default(),
    }
}
fn region(id: u64) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
fn setup(base: usize, max: usize) -> Downstreams {
    let mut h = HistoryBuffer::with_config(base, max, 1).unwrap();
    h.reset_with_index(I(10));
    Downstreams::new(h)
}
async fn settle(predicate: impl Fn() -> bool) {
    let _ = timeout(Duration::from_secs(2), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
}
async fn stop(task: DownstreamTask) {
    task.cancel();
    assert!(task.join().await.unwrap().is_ok());
}

#[tokio::test]
async fn state_task_36_downstream_independent_indexes() {
    let d = setup(8, 8);
    let a = sender();
    let af = a.frames.clone();
    let b = sender();
    let bf = b.frames.clone();
    let ta = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    let tb = d.bind(&Handle::current(), "pd3".into(), I(11), b);
    let records = vec![region(1), region(2)];
    d.append(records.clone()).unwrap();
    d.broadcast(false);
    settle(|| ta.send_index() == I(12) && tb.send_index() == I(12)).await;
    assert_eq!(*af.lock().unwrap(), vec![(10, vec![1, 2])]);
    assert_eq!(*bf.lock().unwrap(), vec![(11, vec![2])]);
    assert_eq!(ta.send_index(), I(12));
    assert_eq!(tb.send_index(), I(12));
    assert!(Arc::ptr_eq(&d.records_from(I(10))[0], &records[0]));
    stop(ta).await;
    stop(tb).await;
    assert!(d.names().is_empty());
}
#[tokio::test]
async fn state_task_36_downstream_removes_only_failed() {
    let d = setup(8, 8);
    let mut a = sender();
    a.fail = true;
    let af = a.frames.clone();
    let b = sender();
    let bf = b.frames.clone();
    let ta = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    let tb = d.bind(&Handle::current(), "pd3".into(), I(10), b);
    d.append([region(1)]).unwrap();
    d.broadcast(false);
    settle(|| d.names() == vec!["pd3"] && tb.send_index() == I(11)).await;
    assert_eq!(d.names(), vec!["pd3"]);
    assert_eq!(ta.send_index(), I(10));
    assert!(af.lock().unwrap().is_empty());
    assert_eq!(*bf.lock().unwrap(), vec![(10, vec![1])]);
    assert!(matches!(ta.join().await.unwrap(),Err(DownstreamError::Send(e)) if e=="send failed"));
    stop(tb).await;
}
#[tokio::test]
async fn state_task_36_downstream_append_and_other_stream_progress_when_busy() {
    let d = setup(2, 16);
    let mut a = sender();
    let gate = Arc::new(Semaphore::new(0));
    a.gate = Some(gate.clone());
    let entered = a.entered.clone();
    let af = a.frames.clone();
    let b = sender();
    let bf = b.frames.clone();
    let ta = d.bind(&Handle::current(), "slow".into(), I(10), a);
    let tb = d.bind(&Handle::current(), "fast".into(), I(10), b);
    d.append([region(1), region(2)]).unwrap();
    d.broadcast(false);
    timeout(Duration::from_secs(2), entered.notified())
        .await
        .expect("slow sender must enter Send");
    // Running on one runtime thread also detects a blocking mutex across Send.
    d.append([region(3), region(4), region(5)]).unwrap();
    d.broadcast(false);
    settle(|| tb.send_index() == I(15)).await;
    assert_eq!(tb.send_index(), I(15));
    assert_eq!(ta.send_index(), I(10));
    assert!(af.lock().unwrap().is_empty());
    assert_eq!(d.records_from(I(10)).len(), 5);
    gate.add_permits(1);
    settle(|| ta.send_index() == I(15)).await;
    assert_eq!(
        *af.lock().unwrap(),
        vec![(10, vec![1, 2]), (12, vec![3, 4, 5])]
    );
    assert_eq!(
        bf.lock()
            .unwrap()
            .iter()
            .flat_map(|(_, ids)| ids.clone())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    stop(ta).await;
    stop(tb).await;
}
#[tokio::test]
async fn state_task_36_downstream_drain_serially_and_batch() {
    let d = setup(2048, 4096);
    let mut a = sender();
    let gate = Arc::new(Semaphore::new(0));
    a.gate = Some(gate.clone());
    let entered = a.entered.clone();
    let frames = a.frames.clone();
    let maximum = a.maximum.clone();
    let t = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    d.append((1..=1001).map(region)).unwrap();
    d.broadcast(false);
    timeout(Duration::from_secs(2), entered.notified())
        .await
        .expect("drain must enter Send");
    for _ in 0..20 {
        d.broadcast(true);
    }
    d.append([region(1002)]).unwrap();
    d.broadcast(false);
    assert_eq!(t.send_index(), I(10));
    gate.add_permits(1);
    settle(|| t.send_index() == I(1012)).await;
    let f = frames.lock().unwrap().clone();
    assert_eq!(f[0], (10, (1..=1000).collect()));
    assert_eq!(f[1], (1010, vec![1001, 1002]));
    assert_eq!(maximum.load(Ordering::SeqCst), 1);
    assert_eq!(t.send_index(), I(1012));
    stop(t).await;
}
#[tokio::test]
async fn state_task_36_downstream_keepalive_after_pending_records() {
    let d = setup(8, 8);
    let a = sender();
    let frames = a.frames.clone();
    let t = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    d.append([region(1), region(2)]).unwrap();
    d.broadcast(true);
    settle(|| t.send_index() == I(12)).await;
    assert_eq!(*frames.lock().unwrap(), vec![(10, vec![1, 2])]);
    d.broadcast(true);
    settle(|| frames.lock().unwrap().len() == 2).await;
    assert_eq!(
        *frames.lock().unwrap(),
        vec![(10, vec![1, 2]), (12, vec![])]
    );
    stop(t).await;
}

#[tokio::test]
async fn state_task_36_downstream_cancel_blocked_send_and_rebind() {
    let d = setup(8, 8);
    let mut a = sender();
    a.gate = Some(Arc::new(Semaphore::new(0)));
    let entered = a.entered.clone();
    let active = a.active.clone();
    let frames = a.frames.clone();
    let old = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    d.append([region(1)]).unwrap();
    d.broadcast(false);
    timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let b = sender();
    let bf = b.frames.clone();
    let new = d.bind(&Handle::current(), "pd2".into(), I(10), b);
    assert!(
        timeout(Duration::from_secs(2), old.join())
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(frames.lock().unwrap().is_empty());
    assert_eq!(d.names(), vec!["pd2"]);
    d.broadcast(false);
    settle(|| new.send_index() == I(11)).await;
    assert_eq!(*bf.lock().unwrap(), vec![(10, vec![1])]);
    stop(new).await;
    assert!(d.names().is_empty());
    // Explicit host shutdown while I/O is blocked also cancels and joins.
    let mut a = sender();
    a.gate = Some(Arc::new(Semaphore::new(0)));
    let entered = a.entered.clone();
    let active = a.active.clone();
    let task = d.bind(&Handle::current(), "blocked".into(), I(10), a);
    d.broadcast(false);
    timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    task.cancel();
    assert!(
        timeout(Duration::from_secs(2), task.join())
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(d.names().is_empty());
}

#[tokio::test]
async fn state_task_36_downstream_history_gap_and_future_index() {
    for start in [I(10), I(14)] {
        let d = setup(1, 1);
        let a = sender();
        let frames = a.frames.clone();
        let t = d.bind(&Handle::current(), "pd2".into(), start, a);
        d.append([region(1), region(2)]).unwrap();
        d.broadcast(true);
        let result = timeout(Duration::from_secs(2), t.join())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(result,Err(DownstreamError::History(super::HistoryError::HistoryGap { requested, first:I(11),next:I(12) })) if requested==start)
        );
        assert!(frames.lock().unwrap().is_empty());
        assert!(d.names().is_empty());
    }
}

#[tokio::test]
async fn state_task_36_downstream_send_failure_preserves_last_success() {
    struct FailSecond {
        calls: usize,
        frames: Frames,
    }
    impl DownstreamSender for FailSecond {
        async fn send(&mut self, frame: SyncFrame) -> Result<(), String> {
            self.calls += 1;
            if self.calls == 2 {
                return Err("second frame".into());
            }
            self.frames.lock().unwrap().push((
                frame.start_index().0,
                frame.records().iter().map(|r| r.meta().id).collect(),
            ));
            Ok(())
        }
    }
    let d = setup(2048, 2048);
    let frames = Frames::default();
    let t = d.bind(
        &Handle::current(),
        "pd2".into(),
        I(10),
        FailSecond {
            calls: 0,
            frames: frames.clone(),
        },
    );
    d.append((1..=1001).map(region)).unwrap();
    d.broadcast(false);
    settle(|| d.names().is_empty()).await;
    assert_eq!(t.send_index(), I(1010));
    assert_eq!(frames.lock().unwrap().len(), 1);
    assert!(matches!(t.join().await.unwrap(),Err(DownstreamError::Send(e)) if e=="second frame"));
}

#[tokio::test]
async fn state_task_36_downstream_buffered_then_live_matches_go() {
    let d = setup(8, 8);
    let a = sender();
    let frames = a.frames.clone();
    let t = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    d.append([region(1), region(2)]).unwrap();
    d.broadcast(false);
    settle(|| t.send_index() == I(12)).await;
    assert_eq!(*frames.lock().unwrap(), vec![(10, vec![1, 2])]);
    d.append([region(3)]).unwrap();
    d.broadcast(false);
    settle(|| t.send_index() == I(13)).await;
    assert_eq!(
        *frames.lock().unwrap(),
        vec![(10, vec![1, 2]), (12, vec![3])]
    );
    stop(t).await;
}

#[tokio::test]
async fn state_task_36_downstream_drains_records_appended_during_keepalive() {
    let d = setup(8, 8);
    let mut a = sender();
    let gate = Arc::new(Semaphore::new(0));
    a.gate = Some(gate.clone());
    let entered = a.entered.clone();
    let frames = a.frames.clone();
    let t = d.bind(&Handle::current(), "pd2".into(), I(10), a);
    d.broadcast(true);
    timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    d.append([region(1)]).unwrap(); // No extra notification: drain must recheck after I/O.
    gate.add_permits(1);
    settle(|| t.send_index() == I(11)).await;
    assert_eq!(*frames.lock().unwrap(), vec![(10, vec![]), (10, vec![1])]);
    stop(t).await;
}
