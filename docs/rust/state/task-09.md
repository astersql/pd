# 任务9：读处理器负载归一化

实现前 HEAD `f1535d112291e1b3f390dcca30f52899cfd3c232`，源码基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；cpu.go/cpu_test.go 与基准无差异。
批次1–8依赖已验收，见[任务8](task-08.md)及其累积证据。
已审查根规则、Copilot、两个只读plan、ADR、依赖基线、移植指南；无更深规则。

## 实现前 oracle

命令 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test pkg/statistics/cpu.go pkg/statistics/cpu_test.go -run '^(TestStoreReadCPUUsage|TestRegionReadCPUUsage)$' -count=1 -v`
退出0，2 passed，见[真实输出](task09/go-oracle.out)。明确的文件列表只编译现有CPU函数与
两项测试，不包含包内TestMain或其它测试。调用仅strings.HasPrefix、protobuf纯getter、
uint64求和及float64转换，测试使用testing/testify；没有failpoint、生成、文件写入。
标准go test不运行generate；readonly/off禁止模块更新和下载，未运行make/failpoint启停。

真实执行断言：store依次输入unified-read-0=80、grpc-server-0=20、other=30，返回80.0；
peer初始无cpu_stats返回0.0，再设置UnifiedRead=80返回80.0。此为运行已有断言，
不声称Go打印了逐字段快照。无时间/随机归一化，CPU保留原单位，不转百分比。

源码补充契约（不是新增输入的Go运行差分）：匹配大小写敏感的任意unified-read前缀，
包括unified-reader；重复记录分别计入。Go uint64累加溢出后再转换float64；
nil列表/无匹配返回0，nil peer/缺cpu_stats为0，scheduler不参与。
Go nil RecordPair getter返回空key/零值，因此对固定非空前缀无贡献；Rust复用core的
非可选RecordPair切片，空切片/默认记录对应缺失贡献，peer复用Option<&PeerStat>。
调用方store.go的Observe/Set读取store CPU；Region helper尚无调用方。库不转换协议，
直接复用现有core RecordPair、PeerStat、CpuStats，无新模型或protobuf依赖。

## Rust 实现与红绿证据

建立 canonical `astersql-pd-statistics` crate，在根 workspace 登记 member/dependency，
生产依赖仅已有core，无新版本或feature；Cargo.lock仅新增本地包。
初次 `cargo update -p astersql-pd-statistics --offline` 因新包未入lock退出101，未运行测试；
随后 `cargo generate-lockfile --offline` 退出0。生成附带pulldown-cmark升级已恢复原条目，
最终锁文件不改变任何既有版本。后续测试/检查全部使用--locked。

最小可编译零返回接口，先fmt再聚焦测试：[红灯](task09/red.out)退出101，
0 passed/5 failed/0 ignored，store/peer实际0期望80，前缀组合实际0期望28，
溢出输入实际0期望80，最大peer CPU实际0期望18446744073709552000。
为真实行为断言失败，不是缺符号或零测试。补齐前缀筛选、wrapping_add及Option缺失链，
公开 `store_read_cpu_usage` 与 `region_read_cpu_usage`，无额外运行时/存储/I/O。

独立cpu_test.rs显式登记5项行为测试：复现Go两项输入断言，精确前缀/相似前缀/
大小写/前置字符/重复记录/空列表/默认记录，无匹配为零；u64最大值溢出与2^53
舍入边界，确认求和后再转换；nil peer/缺cpu_stats/缺省值，scheduler忽略与最大CPU。
额外边界明确为源码契约测试，不冒称它们是新增的Go运行快照差分。
生产文件及入口具有AsterSQL标记和相邻.rs.md说明，库保持领域独立。

## 构建与资源边界

全部Cargo调用经[包装器](task09/slot.sh)从仓库根原子mkdir动态领取槽位，
实际均为槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录plan/task/session/PID，正常、失败及信号退出在finally等待或终止并wait
命令组后清理自己的owner/锁，保留缓存，无target-dir覆盖。
Rust修改后先cargo fmt --all；格式化无任务范围外Rust差异。

已重新审查proto/build.rs：只读固定模块ZIP并校验SHA256，解压/生成仅槽位OUT_DIR。
workspace Etcd fixture的既有Go build只输出二进制到槽位；main.go的WAL/ready/log
均写槽位，无generate/failpoint；stdin EOF关闭，Drop等待并必要时kill/wait。
全部构建子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
本任务是同步纯值函数，无后台资源或后端路径；workspace真实MemoryStore/EtcdMetaStore
回归不替代本任务的行为证据。

## 最终验证

[验证脚本](task09/validate.sh)、[完整日志](task09/validation.out)和
[metadata输出](task09/metadata.out)记录命令与退出码。

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_09_cpu -- --nocapture` | 0，5 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0，5 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，1020 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task09/readonly.py` | 0，1101个Go/模块文件集合、SHA256与Git状态相同，两个只读plan哈希不变，链接与diff检查通过 |

workspace唯一ignored为既有tso_processes；既有环境不足提前返回的健康检查不计入
本任务行为证据。本任务5项测试均执行真实断言且无忽略。
[源码调用审查](task09/call-review.out)、[只读脚本](task09/readonly.py)、
[起点哈希](task09/go-before.json)、[核对输出](task09/readonly.out)、
[资源核对](task09/resources.out)保留完整证据。
全部Cargo和fixture子进程已退出，自身锁释放，缓存保留。未新增/改写任何Go或模块文件；
未运行make、failpoint启停、generate、Go格式化或模块整理。日志仅规范行尾空白。
用户已有计划变更未纳入提交，两个指定plan.md与来源史诗保持原样。

状态：已完成，无待回归项。仅验收CPU负载归一化，不代表整个统计主题完成。
