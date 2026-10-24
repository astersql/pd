# 任务 2：多维前列集合更新

实现前记录：基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，开始 HEAD
`7853d38e3`；topn.go/topn_test.go 与基准无差异。批次1证据见
[task-01.md](task-01.md)，主题4公共能力见其中引用的批次41。
已审查根规则、Copilot、ADR、依赖基线、移植指南及两个只读计划；无更深规则。

## Go oracle（实现前）

直接复用现有源文件与测试，未生成任何 Go 工具或测试。命令：
`GOFLAGS=-mod=readonly GOPROXY=off go test pkg/statistics/utils/topn.go pkg/statistics/utils/topn_test.go pkg/statistics/utils/kind.go pkg/statistics/utils/constant.go -run '^(TestPut|TestRemove)$' -count=1 -v`。
退出0，2 passed，见[真实输出](task02/go-oracle.out)。
此文件列表仅选入既有 TestPut/TestRemove，不引入 kind_test.go 的 TestMain/etcd 清理链。
标准 go test 不运行 generate、格式化或模块整理；readonly/off 禁止模块更新/网络下载。
完整 syncutil 生产依赖选中文件列表见[依赖审查](task02/go-deps.out)，逐文件确认无
failpoint.Inject；选中的四文件无 failpoint、init、外部 I/O 或生成调用，测试只使用
rand/sort/time/testing/testify。无需注入，故在此审查后的特定文件列表上直接运行。

运行契约：TestPut 使用 Total=10000,N=50,DimLen=4；独立随机排列每维，
首次值 -x+1、二次值 -x，同ID Put 返回 false/true；第0维前50项为
ID 0..49、最小值 -49；全体仍为10000且每ID值更新。其它维度降序前50值
与全集降序前50值相同。TestRemove 初值 -x，删除 id%3!=0，返回对应旧项；
再次删除返回nil；保留3334项，第0维最小前列项ID147、值-147，四维均保持前50。
随机排列不保存精确顺序；Rust复用相同输入公式与确定排列验证同一性质，
这是共享断言契约，不声称逐事件随机快照差分。

源码补充契约（与运行证据区分）：TopNItem 以 uint64 ID 标识，Less(k,item)
定义该维严格弱序；相等负载不指定排序或边界成员身份。无序读取返回独立切片；
空最小项/缺失Get/Remove为nil。更新须在所有维度替换旧对象；删除后从rest晋升。
构造 k<=0/n<=0 panic，非法维度访问panic。调用方 hot_peer_cache.go
维护每store的TopN并读取各维最小负载，ID为peer标识，维度值单位由item决定，
本集合不转换单位。TTL在批次3独立实现，本批次只实现集合与显式删除。

## Rust 实现与红绿证据

新增 `topn.rs` 的 `TopNItem`、`TopN<T>`，每维以两棵带ID索引的堆分别维护
前N/余集。真实item共享Arc，更新/删除O(k log M)，不排序扫描全集；无新依赖、
feature或manifest/lock变化。生产文件与crate入口均有相邻说明。
Rust以独占借用保证修改期间所有维度一起更新，保留返回快照的所有权；
跨线程锁由宿主提供，不引入运行时、全局订阅器、protobuf或存储依赖。

显式登记5项 `state_task_02_topn` 行为测试：跨维升降/更新返回值/旧快照保留，
Go 10000项/四维/N=50断言契约，N=1/2/20及相等值/最大ID，2000次确定性
交错更新删除验证，非法构造和维度panic。每次断言检查各维前N值与全集独立排序
一致、最小值正确、ID不重复、各维对象是Get返回的同一Arc。

[初次红灯](task02/red.out)退出101：测试负号/cast优先级编译错误；修正harness后
[确认红灯](task02/red-confirmed.out)退出101，1 passed/4 failed/0 ignored，
可编译最小接口Put尚无行为，插入后len实际0、期望1或10000，事件ID集合也不符。
该行为红灯发生在完整实现之前，编译错误不计入行为证据。
实现双堆索引/修复/晋升后，[绿灯](task02/green.out)退出0，聚焦5 passed，
包级12 passed；panic边界被catch_unwind捕获，nocapture日志中的panic非测试失败。

## 验证与资源

全部Cargo阶段通过[槽位包装器](task02/slot.sh)动态原子领取；实际均为槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner包含计划/任务/会话/PID，finally处理退出和中断，命令退出后释放自己的锁，
保留缓存。构建阶段设置 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off`。
复核 `pkg/proto/build.rs` 仅读取固定模块ZIP并在槽位OUT_DIR生成；
`pkg/storage/kv/etcd_fixture_test.rs` 构建既有fixture到槽位，fixture仅在槽位目录写
WAL/ready，EOF请求关闭，Drop有wait及必要时kill/wait；无需generate或failpoint。
本任务没有后台任务/后端，完整workspace回归仍验证既有MemoryStore/EtcdMetaStore。

[最终验证脚本](task02/validate.sh)、[完整日志](task02/validation.out)、
[metadata输出](task02/metadata.out)包含命令及退出码。

| 确切命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，无其它批次源码差异 |
| `cargo metadata --format-version 1 --no-deps` | 0，canonical crate不变 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-utils --locked --lib state_task_02_topn -- --nocapture` | 0，5 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-utils --locked` | 0，12 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，979 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task02/readonly.py` | 0，1101个Go/模块文件集合、SHA256、Git状态不变，文档链接和diff检查通过 |

workspace既有ignored为`tso_processes`，未计入本任务验收；既有进程环境不足时
提前返回的健康测试也不作为本任务证据。聚焦5项均实际执行行为断言，无忽略。
全部资源已释放，见[资源检查](task02/resources.out)。日志仅清除行尾空白与文件尾
多余空行；未改动结果。没有新增Go文件，未运行make、failpoint启停或生成工具。
[只读校验脚本](task02/readonly.py)、[最终输出](task02/readonly.out)及
[起点哈希](task02/go-before.json)保存前后验证。两个指定plan.md未修改，
原有计划变更不纳入提交。无本任务待回归项。

状态：已完成，仅验收多维前列集合更新/删除；TTL与整个主题验收留给后续批次。
