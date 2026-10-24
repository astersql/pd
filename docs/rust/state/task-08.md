# 任务8：桶任务队列与取消

实现前：HEAD `93ec851ae214d473c63e910af0a52fbf4b7256b6`，基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；桶Go源码与基准无差异。
依赖批次1–7已验收，见[任务7](task-07.md)及其累积证据。
已读根规则、Copilot、ADR、依赖基线、移植指南与两个只读plan，无更深规则。

## 实现前 oracle 与契约

只读运行 `GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics/buckets -run '^(TestCheckBucketsTask|TestCollectBucketStatsTask)$' -count=1 -v`，
退出0，2 passed，见[真实输出](task08/go-oracle.out)。调用链已审查：newTestBuckets→
NewCheckPeerTask→CheckAsync→schedule→convert/inherit/calculate/putItem，collect→
GetHotBucketStats→buffered ret→WaitRet；TestMain仅goleak。上述路径没有failpoint、生成、
源码写入，core其它注入路径不参与。标准go test不运行generate，readonly/off禁止模块改写/下载。
既有defer cancel回收goroutine且goleak通过。未运行make或failpoint启停。

真实执行断言：冷报告region1/version1/ASCII keys=[10,20,30]/period1000ms/六维flow0，
check再collect返回2桶degree[-1,-1]；region2[20,30]后collect为[-2]；
region1[10,20]后collect为[-2]。collect测试直接put 10个degree0的转换项（id0…9，
keys为i*10与(i+1)*10的ASCII十进制），min=-100返回10个region，min=1为空。
此为运行已有断言，不声称Go输出了未打印的逐字段快照。

源码补充：FIFO channel容量20000，非阻塞入队满则false；WaitRet的context取消返回nil，
ret容量1，取消调用者不应阻塞worker。处理耗时单位秒，metric名
pd_scheduler_bucket_task_duration，type标签check_buckets/collect_bucket_stats，
help与23个0.0001*2^i桶继承metric.go。Go关闭select存在竞争、无join；Rust按任务要求
明确关闭为丢弃待执行队列并join，分别用Full/Closed/Cancelled/WorkerStopped表达错误，
不冒称这些额外边界已有Go运行差分。统计结果复用真实领域索引，不mock。

## 实现与红绿证据

新增 `BucketWorker`、`BucketWorkerClient`、`BucketStatsWait` 和类型化提交/等待错误，
显式注册独立测试。worker独占真实BucketRangeIndex，报告转换→同范围原值/重叠继承→
冷热计算→put；查询返回独立快照。父令牌child保证关闭不影响宿主，shutdown取消并join，
重复关闭安全、取消shutdown future可再次join；Drop请求取消、panic传播JoinError。
无存储、网络、协议或私有runtime，故不新增本任务后端路径；workspace回归仍运行真实
MemoryStore/EtcdMetaStore，不能替代本任务队列验收。

[初次编译](task08/compile-initial.out)暴露canonical RangeTree的dyn FnMut未带Send，
不能移入宿主Handle::spawn；另修正prometheus测试getter。依用户授权修复此依赖：
`DebrisFactory`与with_factory参数要求Send，不新增范围树副本/unsafe/专用runtime。
当前所有调用者为函数或无非Send捕获闭包；增加owned_range_tree_can_move_to_host_worker
真实跨线程更新测试，记录外部捕获非Send的回调需要调整这一API约束。

最小空Check接口可编译后，先fmt再执行聚焦命令：[红灯](task08/red.out)退出101，
3 passed/3 failed/0 ignored。具体断言：已有region3 degree实际0期望-1，FIFO及取消后
检查因未执行报告找不到region。此为实际行为失败，不是缺符号或零测试。
补齐真实处理逻辑后[绿灯](task08/green.out)退出0，10 passed/0 failed/0 ignored。
增加workspace既有Tokio/tokio-util/prometheus/thiserror依赖，cargo update离线维护lock，
无新版本或feature、无根manifest变化，后续验证全部--locked。

10项测试覆盖Go两项输入及断言、FIFO中间快照、满20000队列拒绝/恢复且拒绝无效果、
挂起等待期间取消/丢弃reply后后续真实报告仍执行、待执行任务关闭丢弃、父取消与Drop后
实际join、重复shutdown、挂起shutdown被丢弃后再join、无效报告panic→唤醒→JoinError、
重复Registry注册失败不spawn、同宿主多生产者最终4个region各degree=-10及宿主继续工作。
panic测试故意触发既有报告转换前置条件，输出panic文本为预期，测试自身通过。
时间边界仅用调度控制（current-thread先填队列再await、手动poll Pending），不mock领域结果。

指标测试检查原name/help/type标签、两任务各sample_count=3、全部23个桶边界。
只在宿主Registry注册，重复失败为Result，符合库边界；label handle在worker启动时缓存，
处理耗时秒，完成才observe，无region/id动态标签。Registry累计指标保留到宿主释放。

## 构建与资源边界

全部Cargo阶段经[槽位包装器](task08/slot.sh)从仓库根原子mkdir动态领取共享槽位，
实际均为槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner含plan/task/session/PID，正常、失败及信号退出在finally中终止/等待命令组后清理自身锁，
保留缓存。无target-dir覆盖。Rust修改后先cargo fmt --all再验证，格式化差异仅本批文件。
依赖更新命令 `cargo update -p astersql-pd-statistics-buckets --offline` 退出0；
无新版本、无根Cargo.toml变更。

重新审查[调用链](task08/call-review.out)：proto/build.rs只读固定模块ZIP、校验摘要，
解压/生成仅在槽位OUT_DIR；etcd_fixture_test.rs仅go build既有fixture到槽位，
既有scripts/rust-etcd-fixture/main.go的WAL/ready/log均写槽位，无generate/failpoint；
fixture EOF关闭，Drop等待，必要时kill/wait。包装器向所有子进程继承GOARCH=arm64、
CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off，Go/模块最终哈希涵盖整个阶段。

## 最终验证

[脚本](task08/validate.sh)、[完整输出](task08/validation.out)和
[metadata](task08/metadata.out)保存确切命令与退出码。

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，仅本批Rust文件差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_08_bucket_worker -- --nocapture` | 0，10 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-buckets --locked` | 0，31 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-rangetree --locked` | 0，7 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，1015 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task08/readonly.py` | 0，1101个Go/模块文件集合、哈希与Git状态相同，两个指定plan哈希相同，文档链接及diff检查通过 |

workspace唯一ignored为既有tso_processes；环境不足时提前返回的既有健康测试不计入
本任务行为证据。本任务10项行为测试无忽略，均执行真实断言。
[只读脚本](task08/readonly.py)、[起点哈希](task08/go-before.json)、
[核对结果](task08/readonly.out)与[资源检查](task08/resources.out)保存最终证据。
全部Cargo及夹具子进程已退出，自身锁释放、缓存保留。没有新增/修改Go或模块文件，
没有运行make、failpoint启停、generate、Go格式化或模块整理。输出日志仅去除行尾空白
与文件尾多余空行。用户已有计划变更未纳入提交，两个指定plan.md和来源史诗未修改。

状态：已完成，无待回归项。仅验收桶任务队列与取消，不代表整个统计主题完成。
