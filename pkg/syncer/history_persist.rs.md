# history_persist.rs

`PersistentHistory` 独占已有 `HistoryBuffer` 并借用后端无关 `MetaStore`，
经规范 `Base` 仅读写相对键 `historyIndex`。宿主负责命名空间与单写者；
历史索引不是 MetaStore revision。无 protobuf、后台线程、私有运行时或进程退出。

`load` 只恢复十进制 uint64 nextIndex，清空 records/retains 并恢复 base capacity；
缺失与空值从零开始。重启后 first=next，旧记录不能追赶，必须回退全量同步。
格式损坏/越界/非ASCII数字返回 `CorruptIndex`，替代 Go 的 log.Fatal；
读取失败返回 `Load(StoreError)`，由宿主选择重试或启动策略。

`record` 调用真实历史环，每100条尝试保存当前 nextIndex；自动写失败仍重新计数，
内存记录不回滚。`persist` 显式保存，不改变计数；`reset_with_index` 只重置内存与计数，
`reset_with_index_and_persist` 重置后立即写。写失败返回带 nextIndex 与源错误的 `Save`。
恢复能力只保证最近成功保存的索引，不承诺最后不足100条的索引或 records 持久化。

`persist` 要求独占借用，避免本对象的旧快照晚于新快照写入。宿主仍须串行所有实例的
写入。取消 future 可发生在内存变更后或存储提交后：用 `persist` 重试当前检查点，
不能重新 `record` 同一事件或假定 reset 已回滚；没有隐式 Drop 刷盘。
retain、观察窗口和缩容直接委托已有领域算法；外部只读访问 history 防止绕过刷盘计数。

独立显式登记的 history_persist_test.rs 覆盖 Go 两个现有测试快照、99/100边界、
显式刷盘计数、重置、十进制/损坏值/uint64边界、索引耗尽、窗口保留，
真实 MemoryStore 与 EtcdMetaStore，etcd 停机写/读失败、重启恢复及取消写入。
夹具复用既有 Go 程序，只读构建；退出关闭 stdin 并 wait，必要时 kill 后 wait。
证据见 docs/rust/state/task-32.md。
