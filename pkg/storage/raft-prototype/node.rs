// Copyright 2026 AsterSQL.
use crate::{log_store, state_machine::State};
use raft::{Config, RawNode, StateRole, Storage, prelude::*, storage::MemStorage};
use std::{
    collections::VecDeque,
    fs, io,
    path::{Path, PathBuf},
};

struct Node {
    raft: RawNode<MemStorage>,
    state: State,
    applied: u64,
    path: PathBuf,
    base: Snapshot,
}
impl Node {
    fn open(id: u64, path: PathBuf) -> io::Result<Self> {
        let (store, snap) = log_store::load(&path)?;
        let mut applied = snap.get_metadata().index;
        let mut state = if snap.data.is_empty() {
            State::default()
        } else {
            State::decode(&snap.data)
        };
        let commit = store.initial_state().unwrap().hard_state.commit;
        if commit > applied {
            for e in store
                .entries(
                    applied + 1,
                    commit + 1,
                    None,
                    raft::GetEntriesContext::empty(false),
                )
                .unwrap()
            {
                if e.entry_type == EntryType::EntryNormal {
                    state.apply(&e.data);
                }
                applied = e.index;
            }
        }
        let config = Config {
            id,
            election_tick: 10,
            heartbeat_tick: 1,
            applied,
            check_quorum: true,
            pre_vote: true,
            ..Default::default()
        };
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let raft = RawNode::new(&config, store, &logger).unwrap();
        Ok(Self {
            raft,
            state,
            applied,
            path,
            base: snap,
        })
    }
    fn apply(&mut self, entries: Vec<Entry>) {
        for e in entries {
            if e.entry_type == EntryType::EntryNormal {
                self.state.apply(&e.data);
            }
            self.applied = e.index;
        }
    }
    fn ready(&mut self) -> io::Result<Vec<Message>> {
        let mut messages = Vec::new();
        if !self.raft.has_ready() {
            return Ok(messages);
        }
        let mut ready = self.raft.ready();
        if !ready.snapshot().is_empty() {
            let s = ready.snapshot().clone();
            if s.data.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "MemStorage snapshot transport omits state-machine payload; candidate rejected",
                ));
            }
            self.state = State::decode(&s.data);
            self.applied = s.get_metadata().index;
            self.base = s.clone();
            self.raft.mut_store().wl().apply_snapshot(s).unwrap();
        }
        self.raft.mut_store().wl().append(ready.entries()).unwrap();
        if let Some(hs) = ready.hs() {
            self.raft.mut_store().wl().set_hardstate(hs.clone());
        }
        // Persist before any acknowledgement is delivered to another node.
        log_store::save(&self.path, self.raft.store(), self.base.clone())?;
        messages.extend(ready.take_messages());
        messages.extend(ready.take_persisted_messages());
        self.apply(ready.take_committed_entries());
        let mut light = self.raft.advance(ready);
        if let Some(commit) = light.commit_index() {
            self.raft.mut_store().wl().mut_hard_state().commit = commit;
            log_store::save(&self.path, self.raft.store(), self.base.clone())?;
        }
        messages.extend(light.take_messages());
        self.apply(light.take_committed_entries());
        self.raft.advance_apply();
        Ok(messages)
    }
}

/// Deterministic transport; disks are real, membership is fixed at three nodes.
pub struct Cluster {
    nodes: Vec<Option<Node>>,
    paths: Vec<PathBuf>,
    links: [[bool; 3]; 3],
}
impl Cluster {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        let paths: Vec<_> = (1..=3)
            .map(|i| root.as_ref().join(format!("node-{i}.wal")))
            .collect();
        let nodes = paths
            .iter()
            .enumerate()
            .map(|(i, p)| Node::open(i as u64 + 1, p.clone()).map(Some))
            .collect::<io::Result<_>>()?;
        Ok(Self {
            nodes,
            paths,
            links: [[true; 3]; 3],
        })
    }
    pub fn leader(&self) -> Option<usize> {
        self.nodes.iter().position(|n| {
            n.as_ref()
                .is_some_and(|n| n.raft.raft.state == StateRole::Leader)
        })
    }
    pub fn campaign(&mut self, id: usize) -> io::Result<()> {
        self.nodes[id].as_mut().unwrap().raft.campaign().unwrap();
        self.drain()
    }
    pub fn drain(&mut self) -> io::Result<()> {
        let mut queue = VecDeque::new();
        for _ in 0..1000 {
            for node in self.nodes.iter_mut().flatten() {
                queue.extend(node.ready()?);
            }
            if queue.is_empty() {
                return Ok(());
            }
            while let Some(message) = queue.pop_front() {
                let from = message.from as usize - 1;
                let to = message.to as usize - 1;
                if self.links[from][to] {
                    if let Some(node) = &mut self.nodes[to] {
                        node.raft.step(message).unwrap();
                    }
                }
            }
        }
        Err(io::Error::other("transport did not quiesce"))
    }
    pub fn tick(&mut self, count: usize) -> io::Result<()> {
        for _ in 0..count {
            for node in self.nodes.iter_mut().flatten() {
                node.raft.tick();
            }
            self.drain()?;
        }
        Ok(())
    }
    pub fn cas(&mut self, id: usize, key: &str, value: &str, expected: u64) -> io::Result<()> {
        let node = self.nodes[id].as_mut().unwrap();
        if node.raft.raft.state != StateRole::Leader {
            return Err(io::Error::other("not leader"));
        }
        node.raft
            .propose(vec![], serde_json::to_vec(&(key, value, expected))?)
            .unwrap();
        let proposed = node.raft.raft.raft_log.last_index();
        self.drain()?;
        if self.nodes[id].as_ref().unwrap().applied < proposed {
            return Err(io::Error::other(
                "commit quorum unavailable; outcome unknown",
            ));
        }
        Ok(())
    }
    /// Probe a quorum-confirmed read-index. No local-read fallback is permitted.
    pub fn read(&mut self, id: usize) -> io::Result<(u64, Option<String>)> {
        let node = self.nodes[id].as_mut().unwrap();
        node.raft.raft.read_states.clear();
        node.raft.read_index(b"probe".to_vec());
        // Ready consumes read states, so capture acknowledgement in drain separately.
        let mut confirmed = false;
        let mut queue = VecDeque::new();
        for _ in 0..100 {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if let Some(n) = node {
                    if i == id
                        && n.raft
                            .raft
                            .read_states
                            .iter()
                            .any(|r| r.request_ctx == b"probe" && n.applied >= r.index)
                    {
                        confirmed = true;
                    }
                    queue.extend(n.ready()?);
                }
            }
            if queue.is_empty() {
                break;
            }
            while let Some(m) = queue.pop_front() {
                let (from, to) = (m.from as usize - 1, m.to as usize - 1);
                if self.links[from][to] {
                    if let Some(n) = &mut self.nodes[to] {
                        n.raft.step(m).unwrap();
                    }
                }
            }
        }
        if !confirmed {
            return Err(io::Error::other("read quorum unavailable"));
        }
        let node = self.nodes[id].as_ref().unwrap();
        Ok((
            node.state.revision,
            node.state.values.get("key").map(|v| v.0.clone()),
        ))
    }
    pub fn isolate(&mut self, id: usize, isolated: bool) {
        for i in 0..3 {
            self.links[id][i] = !isolated;
            self.links[i][id] = !isolated;
        }
    }
    pub fn kill(&mut self, id: usize) {
        self.nodes[id] = None;
    }
    pub fn restart(&mut self, id: usize) -> io::Result<()> {
        self.nodes[id] = Some(Node::open(id as u64 + 1, self.paths[id].clone())?);
        Ok(())
    }
    pub fn events(&self, id: usize) -> Vec<(u64, String, String)> {
        self.nodes[id].as_ref().unwrap().state.events.clone()
    }
    pub fn revision(&self, id: usize) -> u64 {
        self.nodes[id].as_ref().unwrap().state.revision
    }
    pub fn snapshot(&mut self, id: usize) -> io::Result<()> {
        let node = self.nodes[id].as_mut().unwrap();
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = node.applied;
        snap.mut_metadata().term = node.raft.store().term(node.applied).unwrap();
        snap.mut_metadata()
            .set_conf_state(node.raft.store().initial_state().unwrap().conf_state);
        snap.data = node.state.encode().into();
        node.base = snap.clone();
        log_store::save(&node.path, node.raft.store(), snap)
    }
}
