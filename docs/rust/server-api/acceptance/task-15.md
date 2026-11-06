# 任务15：Diagnostics/ServerInfo 机器信息契约

2026-11-06，基准 `fd7a667cb`。状态：已完成。
[压缩原始回执](task-15.json.gz)保存本轮失败/通过日志、RPC、机器采样、
来源及二进制指纹、Go守卫和清理结果。
来源为 [wire-inventory.md 第100行](../wire-inventory.md#L100)，交接 ID
`wire:652c16ee3bcb0a4e873e87ce`。直接上游 [任务14](task-14.md) 的 canonical
Diagnostics 注册、真实双实现及清理证据已核对；本项没有外部领域前置。

## 实现与准入范围

复用现有 Diagnostics 服务和监听器，在独立的机器采集模块实现 ServerInfo；
没有平行服务、领域算法、生产后端或私有 runtime。同步内核/命令采集放在宿主
blocking pool，局部文件描述符和子进程逐次回收。新增直接依赖仅为锁中已有的
`libc=0.2.191`，用于 Darwin sysctl、statfs、getifaddrs ABI；没有引入新版本。

准入限定为**本机 macOS amd64、固定 Go oracle 的 CGO=0 采集契约**。
Go oracle 的 SHA、SDK、kvproto ZIP、protoc、官方 etcd3.5.15/etcdctl 均按
[准入记录](../../control-plane-mcs/admission.md)重新核验，未构建或改写 Go。
现有 Go 是 Rosetta amd64；Rust 原生 arm64 的 CPU、sysctl 及页大小视图不同，
因此以 `CARGO_BUILD_TARGET=x86_64-apple-darwin` 构建同架构 Rust 服务。
产物仍在领取的共享槽位内，复制到该槽位的 `debug/pd-server` 供既有 harness 使用。
这不是将架构字段归一化；两侧实际运行的 CPU 架构都为 amd64。
原生 arm64 的本地单元检查不构成 Go 差分准入；Linux、Windows及其他平台保持未准入。
非 macOS 的已声明请求显式返回 Unimplemented，未知类型仍为空响应；不宣称跨平台完成。

| 请求 tp | 固定契约 |
| --- | --- |
| 0 / All | load、hardware、system 的完整并集，按 tp、name 排序 |
| 1 / HardwareInfo | CPU、内存容量、`/dev/` 磁盘分区和网卡的真实观察 |
| 2 / SystemInfo | sysctl 条目及原始 pair 顺序；Darwin 无 `/proc/sys` |
| 3 / LoadInfo | CPU load、virtual/swap 字节与比例、网卡累计计数 |
| -1、4、i32::MIN、i32::MAX | OK，items 为空；不将未知 enum 当参数错误 |

CPU 保留架构、逻辑/物理核数、MHz 两位小数和 cache 字段；磁盘保留文件系统、
选项顺序、路径、字节数及两位小数比例。网卡保留 MAC、五个标志、地址/CIDR
及其顺序。累计发送字节字段保留 Go 的 `bytes-ent` 拼写。
系统行按每个冒号拆分，只保留前两个字段，故 `user.cs_path` 的值仍仅为 `/usr/bin`；
没有把 Go 的截断行为改为完整冒号后缀。pair 不排序、不去重。

Rosetta 的 vm_stat 输出声明16KiB内核页，但固定 gopsutil 用进程 Getpagesize
的4KiB换算 Free/Inactive 字节。Rust保持此行为，独立命令采样检查实际RPC字节区间。
固定 Darwin !cgo 的 CPU times 和 disk IO 返回不可用，故没有伪造 CPU usage 或
磁盘速率条目；真实的 CPU load、内存和网卡仍保留。

## 双实现与故障证据

真实 TCP/tonic 请求覆盖21个配对场景：全部声明类型、四种未知类型；
真实非 leader 节点的资源失败、未知类型；leader 不受 follower 本地失败影响；
采集资源恢复后全部声明类型重新读取；成功命令的 stderr 合并输出；命令超时。
另有同轮 Go 系统采样用于识别动态字段。
非 leader 由真实 members 响应确认 leader 为 compat，接收节点为 follower15。
仅 follower 的 PATH 命令包装在启动后切为非零退出，输出包含部分 stdout 和
permission denied stderr；两侧 RPC 仍成功，丢弃部分输出并省略对应资源。
SystemInfo 为空，LoadInfo 保留 CPU load 与 swap，HardwareInfo 保留 CPU、磁盘、
网卡而省略 memory。leader 的系统信息仍非空，恢复后 follower 返回完整结构。
这证明读取接收节点本机资源，没有转发到 leader；未把注入命令失败声称为内核权限故障。

成功 sysctl 的 stderr 追加 `sa015-stderr: kept: ignored`，两侧都在1840个pair的
末尾保留 `{key:sa015-stderr,value:kept}`，证明合并输出及冒号截断均到达真实RPC。
采集使用共享 stdout/stderr socket，边读边等待子进程，正常1839-pair输出也验证了
超过64KiB的输出不会因管道填满而挂起。gopsutil 的 vm_stat/netstat 命令保留3秒
背景超时，独立于RPC上下文；sysutil的sysctl命令保留原来的无命令超时行为。
超时/读取失败均杀停并wait回收本次子进程。真实 `exec sleep 6` 故障中，本轮Go为
3028ms、Rust为3059ms，均省略memory而保留其余53个硬件条目；宿主成功退出。
新增边界red记录Rust原先耗时6076ms且错误保留54个条目，0通过/1失败/0忽略、退出101；
修复合并输出、排空和超时清理后21个场景通过，不以固定空列表代替资源失败。


所有条目、pair 名称和顺序以及静态值均比较；原始 RPC 完整保存。
先检查原始 tp/name 排序，再仅对相同排序键的条目按 pair-key signature 配对。
Go 的 sort.Slice 对相同键没有稳定次序保证，All 中 CPU 和 net 各保留两类条目；
不要求这些相同键的内部次序相等，不删除或合并任何条目。

动态值单列验证：

- CPU load：非负、有限、两位小数，范围 `[0,u32::MAX]`。
- virtual/swap：整数原始字节，`used+free=total`，各值在 `[0,total]`；比例为对应
  分数的两位小数，零 swap 分母保留 `NaN`。物理 total 和 hardware capacity 精确比较；
  OS可动态调整swap总量。virtual 的 free 另落在同请求前后独立 vm_stat/getconf
  字节观察的 min/max 外扩256MiB区间，保留原始观察文本，防止4KiB/16KiB单位误接线。
- disk：fstype、opts、path、total 精确相等；free/used 在 `[0,total]`，
  `used+free<=total`，比例按 `used/(used+free)` 核验，保留 Go 的舍入路径。
- net load：所有计数必须解析为原始 u64；后运行的 Rust 计数不小于 Go，
  资源重置或结构漂移必须失败；硬件网卡字段继续精确比较。
- sysctl：固定清单仅包含已由真实 Go/同架构系统观察证明改变的键；本轮再从 Go
  自身前后响应识别变化键并保存每个键的实际值集合。只有这两种证据可将键列为动态。
  每个动态值保留数字 token 数量、顺序、文本、标点和单位，数值须有限、非负且不超过
  u64::MAX。其余键精确比较，不整项删除 system，不放宽 key 或 pair 顺序。

首次 red 为 Go 返回真实硬件而 Rust 返回 Unimplemented：0通过/1失败/0忽略，退出101。
随后真实双实现暴露两个动态字段清单缺口：`vm.page_purgeable_wired_count`、
`vm.compressor.segment.filling`。两者都由同轮 Go 自身的不同值确认，原始失败和
Go→Go 观察保留；加入同轮前后采样后继续验证，没有把 Rust 独有差异认作波动。
初次编译还遇到磁盘不足和 Darwin flag 类型错误，均为0有效测试，单独保留；
环境空间恢复、修正 flag 转换后继续，没有清除其他任务的锁或缓存。

## 本轮验证

默认features；下列最终结果均退出0：

| 确切命令 | 有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-server --lib sa_015_ -- --nocapture` | 3通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_015_ -- --nocapture` | 1通过、0失败、0忽略；21个配对场景、2个额外Go系统采样 |
| `scripts/run-pd-compat-readonly.sh --require-pinned-protocol --case diagnostics-info` | 同一真实差分入口；严格1个有效测试；守卫与清理全部通过 |
| `cargo test --locked -p astersql-pd-server --lib sa_014_ -- --nocapture` | 2通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_014_ -- --nocapture` | 1通过、0失败、0忽略；41个既有日志配对场景 |
| `CARGO_BUILD_TARGET=x86_64-apple-darwin cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 成功；同架构wire产物 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 成功；结束时恢复当前源码的原生arm64默认构建产物 |
| `cargo metadata --format-version 1 --no-deps` | 成功 |
| `cargo fmt --all -- --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `git diff --check` | 成功 |

去重7个测试通过，0最终失败、0忽略。SearchLog验证在最终仅涉及ServerInfo的
命令输出适配修复前执行，其源码及执行路径未改动，未重复无关日志回归。
全部历史red单独保留，未以最终日志覆盖；编译错误不计行为red。
已有deprecated/dead_code警告没有作为本项改动扩展。
完整 workspace test 按只读总计划集中在任务24，本项不重跑无关领域验收。

所有 Cargo 阶段实际动态领取槽位1，
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
精确 argv、退出码、原始日志、RPC、输入/源码/二进制指纹、Go守卫及清理结果
已保存至本项压缩回执。阶段结束后已释放本项锁，保留编译缓存。
最后wire supervisor为 `compat-readonly-1793954013679830000.json`：
`exit=0`、`readonly_unchanged=true`、`inputs_unchanged=true`、
`process_group_cleaned=true`、`work_directory_cleaned=true`。
Go源码/模块1101项存在性与SHA-256首尾相同，受保护Git状态为空；没有临时Go、
Go构建/测试、make、failpoint、生成、格式化或模块整理。总计划、领域计划及来源史诗
保持只读，开始前已有计划删除和未跟踪计划保留，仅提交本项13个文件。

后续重现同架构差分时，在动态领取槽位后设置相同只读环境，构建amd64并将该槽位
`x86_64-apple-darwin/debug/pd-server` 复制到本槽位 `debug/pd-server`，再设置
`PD_COMPAT_MODE=dual`、固定 `PD_COMPAT_GO_BIN`、本槽位绝对 `PD_COMPAT_RUST_BIN`、
准入的 `PD_COMPAT_ETCD_BIN`/`PD_ETCD_BIN`、`GOMODCACHE`、`PROTOC` 和SDK PATH。
结束时重新执行默认native build恢复默认产物；amd64缓存仍保留在同一共享槽位内。
回执保存完整环境指纹及精确argv；机器观察不声称可跨主机逐字复现。
