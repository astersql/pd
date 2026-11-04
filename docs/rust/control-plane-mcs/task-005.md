# 任务005：同步转异步等待

先固定Go oracle：replication_mode.go的minimalUpVoters、checkStoreStatus及
 tickUpdateState的sync分支。每条全局非learner规则，最低存活voter为
min(up, max(count-down,0))，两个DC相加后再限到count；多数派严格大于一半。
存活边界为DownTime < WaitStoreTimeout，跳过removed，primary/DR分别判定并排序ID。
仅!canSync && hasMajority时由sync进入async_wait，先持久化再发布，重复sync tick不推进。

Go与Rust复用canonical store/placement类型和实际MetaStore schema；真实Go启动前
装入全部store/rule数据，避免500ms后台tick在不完整夹具上提前切换。

## 实现与真实对照

`ModeManager::tick_sync`接受宿主StoreInfo与已解码Rule快照，不启动worker。
复用canonical label constraints及心跳时间计算；跳过removed和有范围规则，
Learner不贡献票数，每条规则两个DC票数之和封顶到count。
检查和发布共享原有状态锁，ID由真实Allocator分配；写入失败不发布。

固定二进制及协议校验复用任务004的只读入口，摘要与任务001一致。
真实Go使用7个store（主站点4存活/1失联，DR站点1存活/1失联），
规则主站点4票、DR1票。最低存活票为3和0，总计5票；
Go实际从sync/1到async_wait/2，持久化available_stores为[100,200,300,400]。
通过真实etcd历史revision读取前态sync/1，并观察后续tick不重复推进。
Rust同输入、真实MemoryStore事务/选主/Allocator得到完全一致JSON，无归一化。

最初8秒夹具超时以及错误多数派夹具的主动中断均不算行为失败。
读取Go Run发现一分钟idle后，修正夹具并运行68.31秒取得有效红灯：
已编译Rust仍是sync/1，真实Go是async_wait/2；退出101。
修复后同命令3通过、0失败、0忽略；新增边界1通过、0忽略。

## 聚焦和相邻验证

完整命令、退出码、原始输出、源码摘要和Go输入输出位于
[task-005-evidence.json.gz](task-005-evidence.json.gz)。

- `cargo test --locked -p astersql-pd-replication --lib mcs_005_ -- --nocapture`：3通过。
- `cargo test --locked -p astersql-pd-replication --lib mcs_005_liveness -- --nocapture`：新增1通过。
- 上述共4项覆盖严格半数、不足多数、重复tick、排序、Learner、范围规则、
  心跳timeout前1ns及相等边界、removed、并发发布；真实etcd网络中断验证写失败
  保留旧内存和持久状态，恢复重试ID3（失败消耗ID2），取消后状态不变。
- `cargo test --locked -p astersql-pd-replication --lib mcs_004_ -- --nocapture`：相邻4通过。
- `cargo check --locked -p astersql-pd-replication`、`cargo check --workspace --locked`、
  `cargo metadata --format-version 1 --no-deps`和`cargo fmt --all -- --check`均退出0。
- `cargo tree --locked -p astersql-pd-replication --edges normal`审查领域边界：
  不依赖server/runtime/proto/tonic/etcd-client；测试适配依赖仅在dev-dependencies。

动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
正常与中断后仅清理自有进程及锁，保留缓存。Go与模块1101项首尾存在性、
SHA和Git状态相同；未运行Go构建、生成器、failpoint或格式化写入。
Cargo.lock只增加5条canonical依赖，没有版本升级和无关排序变动。
遵循用户授权，完整包与workspace测试统一留到全部实现后；本项保留待回归，
不宣称异步确认、恢复、运行周期或任务102已经完成。
