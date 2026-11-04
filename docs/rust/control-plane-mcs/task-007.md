# 任务007：同步恢复进度

固定来源为replication_mode.go的async、sync_recover分支，drSwitchToSyncRecover、
updateProgress、estimateProgress、checkRegionRecover及updateRecoverProgress；
相邻参考TestRecoverProgress和TestRecoverProgressWithSplitAndMerge。

恢复等待使用严格大于WaitRecoverTimeout；从async_start计时，nil视为0时长。
进入sync_recover先保存新ID及recover_start，成功后重置扫描游标/连续恢复计数。
连续扫描1024 region，遇到未恢复或gap则抽样至少512个；采样估算严格小于1，
只有已连续覆盖到空end key才返回1。region的state ID须匹配当前恢复ID，
状态必须为INTEGRITY_OVER_LABEL。扫描遇到merge覆盖当前游标仍可继续，gap阻止完成。
进度计数仅更新内存/HTTP，不写成新持久状态；完成时另分配ID保存sync。

## 实现与真实差分

`tick_async`/`tick_recover`复用同一manager、Allocator、MetaStore、StoreInfo、
placement规则和宿主RegionsInfo，没有复制region registry或创建runtime。
恢复游标为同一manager的易失状态，进入恢复状态持久化成功后才重置；
恢复期间切主/重建会重新扫描，不把内存进度当成耐久状态。

固定Go经过sync/1、async_wait/2、真实StoreHeartbeat确认进入async/3；
DR恢复后进入sync_recover/4。随后真实RegionHeartbeat先报告两个分裂region：
左区间INTEGRITY_OVER_LABEL/4，右区间SIMPLE_MAJORITY/4。
HTTP实际观察total_regions=2、synced_regions=1、recover_progress=0.5，
此时etcd恢复记录字节保持不变。将二者合并为覆盖游标的完整region后，
Go写入sync/5；Rust相同输入也得到0.5和相同最终持久JSON，未归一化ID或revision。
墙钟recover_start保留在原始Go记录里，最终sync记录无时间字段。

初始Read参数类型错误只算夹具编译问题。修正后完整真实Go场景先执行成功，
可编译Rust在“Go entered recovery when DR returned”失败，退出101。
实现后同一命令真实差分1通过；新增3项聚焦0失败、0忽略：

- 1030 region覆盖跨1024批次、低于512样本时补扫、错误恢复ID、gap阻止完成，
  合并修复后完成；抽样进度保持为内存状态，持久计数字段仍为0。
- WaitRecoverTimeout前1ns和相等时不切换、超过1ns才恢复；DR再次丢失但主站点
  保有多数时回async，可用集合变化仅在当前async ID全部确认后发布。
- 真实etcd目标TCP写入中断覆盖进入恢复和最终回sync两个阶段，失败保留原状态；
  恢复后重试消耗新ID并成功，取消后不再发布。

## 验证清单

[task-007-evidence.json.gz](task-007-evidence.json.gz)保存所有阶段确切命令、
退出码、原始Go/Rust输出和源码SHA。有效命令结果：

- `cargo test --locked -p astersql-pd-replication --lib mcs_007_ -- --nocapture`：
  真实恢复差分1通过；随后新增3项用同前缀加`--skip mcs_007_behavior`执行，3通过。
  该skip仅避开刚完成的76.66秒真实场景，不计作第二次执行；共4项有效测试。
- `cargo test --locked -p astersql-pd-replication --lib mcs_006_ -- --nocapture`：4相邻通过。
- `cargo test --locked -p astersql-pd-replication --lib mcs_004_real_storage_and_cancel -- --nocapture`：1相邻通过。
- `cargo check --workspace --locked`、`cargo metadata --format-version 1 --no-deps`、
  `cargo fmt --all -- --check`均退出0；metadata仍为76个package。

动态槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
Cargo.lock仅增加测试所需的现有tokio-stream workspace依赖，未升级版本。
Go/模块1101项存在性/SHA/Git状态一致，全部自有进程结束后释放锁，缓存保留。
按照用户授权，完整包/workspace测试留到全部实现后统一运行，本项保留待回归。
