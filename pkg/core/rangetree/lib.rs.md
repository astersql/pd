# Canonical range tree

`RangeTree<T>` owns the canonical order-statistics B-tree and a mutable debris
factory. Updates remove ordered overlaps and retain valid factory fragments;
queries use half-open byte ranges and the empty end as infinity. Production
behavior and Go oracle mapping remain those recorded in the range tree migration.
Tests are explicitly registered from `range_tree_test.rs`.

The factory trait object and `with_factory` argument require `Send`. This makes
an owned `RangeTree<T>` transferable when `T: Send`, allowing the existing bucket
index to move into a host Tokio task. It adds no locks, runtime, protobuf or
parallel index implementation. A factory capturing non-Send state must use
Send-owned state instead; no current repository caller does so. Shared access
still requires caller synchronization; the tree is not promised to be `Sync`.
