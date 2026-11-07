# 生产任务 11 原生目录与重开

`NativeMetaStore::open` 由宿主传入运行时、共识配置和网络。每个节点持有独占锁，
SQLite 状态表持久绑定格式 1、共识集群 ID 和节点 ID。身份核对后才能使用 WAL。
打开成功后从快照和已提交日志恢复。关闭取消请求和 lease worker，再等待 Raft。
[实现](../../../pkg/storage/kv/native.rs)、[目录格式](../../../pkg/storage/kv/candidate.rs)。

`theme10_native_open` 写入真实磁盘，拒绝第二个打开者、错误集群/节点和未知格式，
重开后核对键值及 revision。最终 release 夹具上的同组回归通过。
[打开用例](../../../pkg/storage/kv/native_open_test.rs)。
