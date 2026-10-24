# 任务 4：桶值对象与报告归一化

基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，开始 HEAD `be2541f86`。
目标 Go 文件与基准无差异。批次 1–3 已验收，见 [任务3](task-03.md) 及其依赖。
已读根规则、Copilot、ADR、依赖基线、移植指南、两个只读 plan；目标无更深规则。

## 实现前 Go oracle

只读运行既有测试：`GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics/buckets -run '^(TestConvertToBucketTreeStat|TestBucketTreeItemClone)$' -count=1 -v`。
退出0，2 passed，见 [原始输出](task04/go-oracle.out)。
输入为 region=1/version=0、keys=1..5、周期1000ms，四桶六维计数各为1..4；
断言范围1..5、region/version及四桶。克隆用010/020/100，六组范围断言数量与裁剪边界。
无随机字段。桶生产和测试均无 failpoint；TestMain 仅 goleak 检查。
调用链 convert→getStart/End 和 clone→keyutil.Min/Max、BucketStat.clone 无注入或I/O。
core 中两个 failpoint 位于 UpdateBuckets/UpdateSubTree，所选测试不调用；init仅阈值及日志/指标注册。
标准 go test 不运行 generate，不使用 make 或注入；readonly/off禁止模块写入/下载。
审查记录见 [call-review.out](task04/call-review.out)。

源码补充契约（不冒称上述运行测试的断言）：周期0回退10000ms；每维
`uint64(count*1000)/interval`，乘法先按64位回绕，整数截断；仅取keys.len-1桶，
多余流量元素忽略，短数组越界panic，无key时panic，单key合法零桶。
六维顺序read bytes/keys/qps、write bytes/keys/qps，CPU不在桶报告中。
空末键保留无穷语义。Go clone 复制loads但共享只读键；Rust自有Vec快照与Clone同时
复制键和loads，避免调用方可变输入污染快照。范围碎片操作留给批次5。
协议入口由后续pkg/server适配，领域不依赖protobuf、存储或runtime。

## Go 运行快照补充与 Rust 差分

实现前已取得上述两项既有行为测试；随后增加字段级运行证据。
本机Delve在Rosetta下无法启动，改用隔离Alpine容器内Delve 1.27.1，
只读挂载由既有Go测试编译出的linux/arm64二进制（没有新增Go源码）。
编译命令：`GOOS=linux GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test -c -gcflags='github.com/tikv/pd/pkg/statistics/buckets=-N -l' -o /tmp/pd-state04-oracle/buckets.test ./pkg/statistics/buckets`。
[调试命令](task04/oracle.commands)、[二进制摘要](task04/oracle-binary.out)、
[真实输出](task04/oracle-snapshots.out)保存来源。
运行命令：`docker exec pd-state04-dlv dlv exec /oracle/buckets.test --allow-non-terminal-interactive --init=/oracle/commands -- -test.run '^TestConvertToBucketTreeStat$'`，退出0。
`go test -c`只编译既有测试，`-gcflags`仅使桶包可调试；不生成/格式化Go。
Delve先在既有测试line120暂停，依次调用原转换函数；将周期从1000改0、3、1，
将read_bytes首项改u64::MAX；最后周期2000、六维首项分别11..16、末键为空。
`call -unsafe`仅跳过Delve的栈指针逃逸检查，指针在暂停栈帧内仍有效，
调用不持有报告指针、不启动服务或后台任务；不属于生产FFI。

[解析器](task04/parse_oracle.py)仅提取返回值，不计算期望速率；
[fixture](../../../tests/rust-compat/fixtures/state/task-04.json)保存5组、20桶、120个实际速率。
ordinary首桶六维为1；zero周期实际10000、全部0；3ms首桶333；
u64::MAX回绕后18446744073709550616；不同维度首桶为[5,6,6,7,7,8]，空末键保留。
Rust逐桶比较实际loads、StartKey、EndKey与周期，无随机/时间归一化。
这些是额外运行差分；短数组和无key的panic仍明确为源码边界断言。

## 实现与红绿

新增canonical crate，生产零外部依赖，仅测试复用已锁定serde_json；根注册member/dependency，
Cargo.lock仅新增本地包，不改变既有版本（离线generate附带pulldown-cmark更新已恢复）。
`BucketReport`、`BucketFlows` → `convert_to_bucket_tree_item` → `BucketTreeItem`/`BucketStat`。
全部键和负载自有，Clone隔离；status为Alive，热度初始0。
生产入口及模块均有相邻.rs.md，测试独立显式登记，不添加猜测feature。

[红灯](task04/red.out)退出101：最小可编译接口返回默认值，4 failed/0 passed，
实际桶数0期望2、周期0期望10000、空keys未panic。不是缺符号或零测试。
实现后[首轮绿灯](task04/green.out)4 passed；补充相同Go输入与运行fixture后共6项测试，
覆盖维度次序、ms/秒、零周期、截断/回绕、空结束键、独立克隆、每维短数组、
空/单键、多余计数，以及5组真实Go快照。
范围碎片、继承、冷热判定和队列仍由后续批次处理。

首次fixture测试发现解析器把Go类型名`[]uint8`中的`[]`误读为空键，
[失败输出](task04/fixture-parser-failed.out)为5 passed/1 failed；修正为提取cap字段之后
的实际数组，再从原始输出重新生成fixture，无手填期望结果或生产实现改动。

## 槽位与只读边界

全部Cargo命令由[动态槽位包装器](task04/slot.sh)原子mkdir领取并记录plan/task/session/PID。
各阶段实际均槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
finally在命令退出后释放锁，异常/信号时先终止并wait子进程组，再释放自己的锁；缓存保留。
调用环境为GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
已复核proto/build.rs仅读取固定ZIP，在槽位OUT_DIR生成协议；etcd_fixture_test.rs
只编译既有scripts/rust-etcd-fixture到槽位，夹具在槽位写WAL/ready，Drop以EOF关闭、
wait，必要时kill/wait，不依赖generate/failpoint。完整workspace使用真实MemoryStore/
EtcdMetaStore；本任务自身仅为纯值快照，不创建后台任务或后端。

## 最终验证

[验证脚本](task04/validate.sh)、[完整日志](task04/validation.out)、
[metadata](task04/metadata.out)记录确切命令及退出码。

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，无其它任务源码格式差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_04_bucket_values -- --nocapture` | 0，6 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-buckets --locked` | 0，6 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，989 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task04/parse_oracle.py` | 0，5组真实Go快照 |
| `python3 docs/rust/state/task04/readonly.py` | 0，1101个Go/模块文件集合、SHA256及Git状态不变，文档链接/diff检查通过 |

[首次workspace输出](task04/validation-initial.out)在未修改的syncutil
`singleflight_randomized`的时序断言失败，退出101；最终原命令完整重跑通过，
该测试亦通过，未改动/忽略该测试。workspace既有ignored为tso_processes，
既有进程环境不足时提前返回的健康测试不作为本任务证据；聚焦6项均真实执行。
被catch_unwind捕获的输入边界panic打印不表示测试失败。

[只读脚本](task04/readonly.py)、[起点哈希](task04/go-before.json)、
[核对输出](task04/readonly.out)、[资源检查](task04/resources.out)保存最终证据。
Cargo子进程已退出，自己的槽位锁已释放且缓存保留；Go调试容器及本任务临时二进制
已清理，无新增后台资源。没有Go源码/模块差异，没有make、生成器或failpoint启停。
两个指定plan.md及来源史诗保持原样，用户已有计划变更未纳入提交。

状态：已完成，仅验收桶值对象与报告归一化，不代表整个热点桶或统计主题完成。
