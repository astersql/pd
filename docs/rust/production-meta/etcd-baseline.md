# 第17项本轮实验阈值

在 Darwin arm64 的私有临时目录中运行同一固定版本外部 etcd 3.5.15 三节点。
三轮都使用三名 voter 和 loopback。每轮单客户端串行写128笔事务，每笔原子
创建两个键，每值256字节，提交后依次线性读取两个键并核对值。重启计时从
发送启动请求到三节点同意当前主节点且首次线性读验证成功，随后对账全量128笔。

运行 `python3 -B docs/rust/production-meta/check_etcd_baseline.py target/rust-slot-1/etcd-official/etcd`，
退出码0，三轮重启各找回128笔。脚本核验二进制摘要，只运行自身临时集群。
[脚本](check_etcd_baseline.py)、[冻结样本](etcd-baseline.json)。

| 比较项 | 本轮阈值 | 推导方式 |
| --- | --- | --- |
| 两键事务写入 p99 | ≤ 37.879 ms | 三轮中最慢的 p99 |
| 两次线性读合计 p99 | ≤ 16.618 ms | 三轮中最慢的 p99 |
| 写入吞吐 | ≥ 73.580 事务/秒 | 三轮中最低值，事务数除以写 RPC 耗时总和 |
| 同配置重启到首次正确读 | ≤ 418.032 ms | 三轮中最慢的重启读 |
| 容量与确认丢失 | 至少256键，128笔确认丢失0 | 三轮均逐键恢复验证 |

## 候选实测

同一脚本加 `--candidate <release storage-kv 单元测试二进制>`，在同一台
Darwin arm64/APFS 主机启动三个独立进程。客户端沿用同样的 HTTP 请求、数据
大小、顺序、计时和重开对账。候选的节点间 RPC 走 loopback HTTP JSON，使用
OpenRaft 实验选举配置 30 ms 心跳和 120–240 ms 选举窗口。etcd 使用其默认
配置及内部协议。因此重启数字反映各自的配置，不能归因于 SQLite 或 WAL 本身。
[候选进程入口](../../../pkg/storage/kv/candidate_bench_test.rs)。

候选先启用 WAL、`synchronous=FULL` 和 macOS `fullfsync=ON`，使本机 SQLite
VFS 请求和 etcd 一样的 `F_FULLFSYNC` 设备缓存屏障。启用该设置前的试跑
不进入本表。随后按 OpenRaft 的约定，移除独立提交水位写入，仍在应用事务中
持久化 applied 索引和业务记录。复跑七项故障用例和同一工作负载。
[etcd macOS 同步实现](https://github.com/etcd-io/etcd/blob/v3.5.15/client/pkg/fileutil/sync_darwin.go)、
[SQLite fullfsync](https://sqlite.org/pragma.html#pragma_fullfsync)、
[OpenRaft 存储契约](https://docs.rs/openraft/0.9.22/openraft/storage/trait.RaftLogStorage.html#method.save_committed)。

| 比较项 | 冻结阈值 | 候选三轮 | 判定 |
| --- | --- | --- | --- |
| 写入 p99 | ≤ 37.879 ms | 38.297 / 34.653 / 613.636 ms | 未通过 |
| 两次线性读合计 p99 | ≤ 16.618 ms | 1.453 / 1.154 / 1.430 ms | 通过 |
| 写入吞吐 | ≥ 73.580 事务/秒 | 36.798 / 38.238 / 28.536 | 未通过 |
| 同配置重启到首次正确读 | ≤ 418.032 ms | 64.634 / 64.471 / 65.298 ms | 通过 |
| 恢复容量 | 每轮128笔、256键 | 128 / 128 / 128 笔全部找回 | 通过 |

在动态领取槽位后，运行
`cargo test --offline --locked --release -p astersql-pd-storage-kv --lib --no-run --message-format=json`，
选用目标 `astersql_pd_storage_kv` 的 `executable` 字段，持有同一槽位运行脚本。
本轮两次 release 构建和各三轮测量均退出0，槽位1。报告中的 `false` 记录未达标，
退出0表示采集完成。基线与候选的重启都是正常进程终止，设备掉电另见故障回执。
[优化前样本](candidate-baseline-before.json)、[最终样本](candidate-baseline.json)、
[掉电回执](power-evidence.md)。
