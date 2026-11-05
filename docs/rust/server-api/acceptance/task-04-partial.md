# 任务04：单键空间GCv2局部接线证据

此为历史局部快照；2026-11-06已完成四方法验收，当前结论见[完整验收](task-04.md)。

2026-11-05，生产基准d25681884。本文件只记录三个已接线方法的当前证据，任务04尚未完成。
完整回执：[task-04-partial.json.gz](task-04-partial.json.gz)。

GetGCSafePointV2、UpdateGCSafePointV2、UpdateServiceSafePointV2通过Grpc和SyncGrpc显式注册，调用132公开的当前任期GCStateManager。
适配层只提取Go GetKeyspaceId对应的oneof值、保留bootstrap/角色校验、转换协议和从真实TSO计算返回TTL；领域判断仍由43–47负责。
未实现GetAllGCSafePointV2，未登记列表为成功功能；剩余准入及完整验收缺口只记录在编号任务文件。

当前差分测试每实现22项比较观察：三个未bootstrap header，空间2/3独立推进及同值写入/回退/超txn，
统一管理类型错误、缺失metadata、Null别名，以及独立屏障实际创建/删除、非法worker TTL、统一管理服务重定向。
成功及错误的header/code/完整message/bytes对照相同。无限minimum TTL按调用前后真实TSO夹定，原始服务响应和TSO均保留。
原始状态保留实际key、bytes、header/create/mod revision；两侧独立停止并重启etcd WAL及PD，fresh连接读取空间2/3的值和完整持久entries相同。
不把独立cluster ID及独立物理revision数字直接横向相等；失败稳定状态、WAL前后entries在各实现内部比较。
本测试尚未提供四方法完整follower/取消/事务故障、有限服务TTL过期清理及列表恢复矩阵；相邻旧版证据不能自动覆盖这些缺口。

验证（确切命令、退出码及完整stdout在压缩回执）：

- 初始`cargo test --locked -p astersql-pd-compat-tests --lib sa_004_ -- --nocapture`：1行为失败，exit101，Rust返回Unimplemented。先前编译和首次启动失败单独保存，不算行为red。
- 最终相同命令：1 passed、0 failed、0 ignored，exit0。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2 passed、1既有ignored，exit0。
- `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture`：2 passed，exit0。
- `cargo test --locked -p astersql-pd-compat-tests --lib sa_003_ -- --nocapture`：1 passed，exit0。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、metadata、最终fmt check、workspace check通过；完整workspace test按任务24集中回归。
- 各阶段原子领取槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；所有自有子进程结束后释放锁，缓存保留。
- Go/模块1101项首尾存在性及SHA完全相同，Git状态没有Go/模块修改；未运行Go测试/构建、make、failpoint注入或生成/整理命令。
- Go、固定f8ad079协议ZIP、官方etcd3.5.15、etcdctl、protoc逐项核验admission SHA；Rust二进制和当前源码指纹保存在回执。
- `git diff --check`通过；既有用户/其它任务变更保留，所有plan.md和来源史诗未改。
