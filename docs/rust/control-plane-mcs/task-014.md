# 任务014：密钥监听、重连与关闭

KeyWatch启动时先成功加载字典，再只在宿主Handle上运行一个任务；接受父CancellationToken，
显式shutdown取消并join，Drop取消并abort。所有watch打开/读取、字典解码、重建和轮换
等待都可被取消，不创建runtime或安装全局日志/信号。保存实例内last_failure供宿主诊断。

同一任务调用真实KeyRotation，周期为min(data-key-rotation-period,10分钟)，首tick延后，
错过tick时Skip；非正周期返回配置错误。PUT经过同一DictionaryStorage.load_entry，
DELETE/坏数据只记错误并保留已接受快照。MetaStore提供prefix watch，因此显式过滤
精确encryption_keys键，不将相似备份键的数据当作密钥。

初次存在字典时保留Go的inclusive已加载revision起点；连接断开/compaction后实际
point read的header revision用于新监听游标，已接受字典仍保持自身mod_revision。
即使字典未修改、坏数据或删除使其不能发布，也能从新header+1继续接收修复事件；
不把watch游标冒充已接受key revision。失败重试100ms，避免连续重连忙循环。

## 真实观察

- 原生etcd的真实写入使watcher精确收敛到事务revision；旧Arc字典保持不变。
  shutdown后后续写入不更新缓存；重新建立监听后Drop也释放原生watcher。
- 把未修改字典的revision留在compaction边界之前，真实compact到较新的header，
  观察Compacted，再写入修复值并收敛。初始key revision不会因游标重建被虚增。
- 坏外层protobuf、精确键删除保留旧快照；备份前缀事件忽略，后续合法值恢复。
- 真正停止/重新启动同一原生etcd WAL，现有连接重连后收到新字典；再SIGSTOP后端，
  挂起实际I/O时shutdown在500ms内返回，CONT后后续提交不改变已关闭缓存。
- 真实计时器驱动有效leader轮换，取消父token后不再更新；原生etcd的
  etcd_debugging_mvcc_watcher_total在关闭/Drop后回到0。该既有指标仅用于资源观察，
  没有新增全局metrics或以调用次数代替业务断言。

只读Go oracle启动真实Go PD，Rust watcher连到同一个实际etcd：同一事务修改合法字典，
双方按相同revision加载；同一坏protobuf事件双方报错，修复后双方按相同revision收敛。
Rust join关闭后，第三个真实提交仅由仍运行的Go watcher接收，Rust snapshot冻结。
没有归一化revision，没有Go源码/模块改动、Go构建或failpoint注入。
Go私有缓存未被读取；Go侧观察为真实存储和确切revision的reload/error日志。
compaction和断线故障证明来自原生Rust端场景，不冒充Go运行时故障注入。

## 验证

[完整证据](task-014-evidence.json.gz)保存所有阶段命令、退出、原始日志及来源SHA。
red编译后在真实提交必须通过watch收敛的断言超时。green首项通过；green2三项通过，
断线测试的第二次重启后旧unary连接尚未恢复，夹具直接写入失败；保留该记录。
现在第一次重启先以实际linearizable读确认旧连接恢复，取消用例则改为实际SIGSTOP/CONT。
green3五项通过。final串行5聚焦+15相邻通过；之后合并原生夹具注册，避免多个模块
各自static目录计数器互撞，并做实际跨模块并行回归：

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_01 -- --test-threads=2 --nocapture | 0 | 24通过、0失败、0忽略：5本项+19文件/KMS/字典/轮换相邻 |
| cargo test --locked -p astersql-pd-storage-kv --test etcd_contract watch_replay -- --exact --nocapture | 0 | 1相邻通过 |
| 同上，过滤compaction_recovery | 0 | 1相邻通过 |
| 同上，过滤watch_cancel | 0 | 1相邻通过 |
| 同上，过滤disconnect_restart_resume_and_cancel | 0 | 1相邻通过 |
| cargo check --workspace --locked | 0 | 工作区编译通过；共享夹具变更后再次通过 |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo tree --locked -p astersql-pd-encryption --edges normal --prefix none | 0 | 无PD proto/etcd-client/server普通依赖 |

不将重复阶段重复计数，独立有效结果为5聚焦+23相邻。动态槽位1，
CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；结束释放自己的锁，
保留缓存。Go/模块1101项存在性、SHA、Git状态一致。
完整包级/workspace测试依用户授权在全部实现后统一执行，本项保留待回归；
不宣称Region加密宿主接线或任务102完成。
