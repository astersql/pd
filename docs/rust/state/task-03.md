# 任务 3：前列集合过期与续期

实现前记录：源码基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，
开始 HEAD `5666294669a825759591035d042c9da114d7078f`。
批次1–2依赖证据见 [task-01.md](task-01.md)、[task-02.md](task-02.md)。
既有 TopN 的索引堆更新/删除已验收；本次复用，不增加协议、存储或运行时依赖。
已阅读根规则、Copilot、ADR、依赖/移植指南及两个只读计划；目标无更深规则。

## Go oracle（实现前）

`topn.go` / `topn_test.go` 与基准无差异。审查 TestTTL 和调用方
hot_peer_cache.go：以固定 TTL 续期，读取和 Put 不隐式过期；RemoveExpired
才清理，更新已到期但未清理的 ID 仍返回 true。每个过期 ID 在返回值重复 DimLen 次。
`ttlList.takeExpired` 的 `expire.After(now)` 表明相等即到期（源码推导）。

只读复用既有四个文件，排除 kind_test.go 的 TestMain/etcd 清理链：
`GOFLAGS=-mod=readonly GOPROXY=off go test pkg/statistics/utils/topn.go pkg/statistics/utils/topn_test.go pkg/statistics/utils/kind.go pkg/statistics/utils/constant.go -run '^TestTTL$' -count=1 -v`。
退出0，1 passed，真实输出见 [go-oracle.out](task03/go-oracle.out)。
复核与批次2相同的 syncutil 选中文件 lock_group.go/mutex_sync.go/
ordered_single_flight.go 无 failpoint/init；四个输入文件无生成、外部I/O、failpoint。
标准 go test 不运行 generate；readonly/off 禁止模块改写及下载，无需注入。

运行契约：4维，1000项，N=50，TTL=900ms。插入 -x 后等待900ms，续期ID0
为第0维100，Put=true；清理返回999*4项；重新插入3,6,..999，Put=false；
总量334，第0维前50值为100-3*i，其余维前50值与全集排序一致。
随机维度只比较排序性质，Rust使用确定输入复现同一契约；不声称随机逐事件差分。
边界纳秒、零TTL与显式删除的测试来自源码契约，区别于上述真实运行证据。

## Rust 实现与红绿

新增 `topn_ttl.rs` 的 `TtlTopN<T>`，组合既有 `TopN`，`items()` 仅提供只读视图，
所有写操作同时维护期限。调用方传入相同单调时钟原点的 Duration；不引入线程、
私有运行时、全局时钟、protobuf 或后端。默认features，无manifest/lock变化。
期限使用BTreeSet与ID索引，每ID至多一条；更新/删除移除旧期限，空间O(M)，
无需积累陈旧事件或扫描未到期项。Rust零TTL立即到期，不表示Go负TTL；
时间加法溢出在修改前panic。相等期限ID间顺序不承诺，返回次数保留Go维度重复。

4项 `state_task_03_topn_ttl` 独立行为测试显式登记：
纳秒边界/相等即到期/零TTL/重复清理，过期未清理续期/保留旧Arc/989次再次续期，
删除再插入/批量部分过期/剩余项晋升，Go TestTTL的1000项续期与334项重插入契约。
每个关键状态检查全量ID、每维独立排序后的前N、最小项及Arc身份一致。
实现前先补最小可编译接口，真实[红灯](task03/red.out)为0 passed/4 failed，
如边界清理实际`[]`而期望`[7,7,7,7]`，批量清理实际0而期望3996；
不是缺符号/编译失败或零测试。完成期限索引后4项均通过；包级16项通过。

## 共享槽位和只读边界

所有Cargo调用通过[动态槽位包装器](task03/slot.sh)原子mkdir领取，
实际红灯与最终验证均槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；finally在退出/中断时等待或终止本阶段子进程组，
随后释放自己的锁，保留编译缓存。未另建target或覆盖target-dir。

已审查 `pkg/proto/build.rs`：固定模块ZIP校验后仅在OUT_DIR生成协议；
`pkg/storage/kv/etcd_fixture_test.rs`：仅编译既有Go fixture到槽位，
fixture在槽位中写WAL/ready，EOF关闭，Drop wait并在必要时kill/wait。
二者无需generate/failpoint；沿用已验收依赖链，不创建或改写Go源码。
构建阶段继承 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off`。
本任务不引入后台任务或存储，workspace验证复用真实MemoryStore/EtcdMetaStore。


## 最终验证

[验证脚本](task03/validate.sh)、[完整输出](task03/validation.out)、
[metadata](task03/metadata.out)记录以下确切命令与退出码：

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，无其它任务Rust源码差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-utils --locked --lib state_task_03_topn_ttl -- --nocapture` | 0，4 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-utils --locked` | 0，16 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，983 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task03/readonly.py` | 0，1101个Go/模块文件集合、SHA256与Git状态不变；相对链接和diff检查通过 |

workspace既有ignored为`tso_processes`，不计入本任务验收；环境不足时提前返回的
既有健康进程测试也不计入本任务证据。聚焦4项均真实执行、未忽略。
没有新增Go文件、模块差异或failpoint启停；两个指定plan.md保持只读。
[只读脚本](task03/readonly.py)、[起点哈希](task03/go-before.json)、
[只读结果](task03/readonly.out)和[资源结果](task03/resources.out)保存核对证据。
原有计划改动排除于提交。日志仅清理行尾空白，不改变结果。

状态：已完成，仅验收前列集合过期/续期；不表示整个主题已完成。无待回归项。
