# 第 20 项协议来源与注册覆盖

审计基准 `7ce928296bd41ab64b2df4394ee08b7a385d14d9`。依赖核对使用
[批次 19 的当前来源回执](task-19.md)、[额外服务交接](extra-services.md)及
[原生拓扑证据](task-18.md)。[coverage.json](coverage.json) 的 `wire_audit_20`
指向[机器回执](task-20.json.gz)。原 [wire manifest](../wire-inventory.md)和
[102 历史回执](../task-102-resume-evidence.json.gz)保留旧值。

## 来源与注册

- 原 manifest 的 365 条以 fenced text 的 `hash + 两空格 + path` 解析，
  与任务 1 冻结清单逐项核对。339 条字节一致，25 条漂移，1 条是盘点前
  已移除的旧计划入口，核验该文件在 Git 中的原始字节。另登记 378 条当前
  来源，涵盖协议构建、Rust 服务与 MCS 适配、实际验收测试、注册行的引用和回执。
  机器回执记录每路径的历史值、当前值、归因提交和审计责任。
- `wire-inventory.md` 更新前 SHA-256 为
  `4540629edddbc35c14c77aa9701681eeaafa863b900feb13a7cca6464ae6dbb9`，
  更新后为
  `7ea60801dbf47a80a9072f3e59a7083de0f56fc45e37d96c5c10367e6520444e`。
  批次 3 已更新三条 GC 行的接线与证据，现按方法 ID 对接历史责任和当前行摘要。
  原 manifest 的集合摘要仍为
  `19c692a402d9a234dc083ada3e346aa1f247f25f8b13f47363c3d6af91b8b501`。
- 固定 kvproto 生成的当前 descriptor 给出 56 个 classic PD 方法、
  35 个额外服务方法和 7 个传递未注册方法。逐项比对全名、请求及响应类型和
  两端流标记。独立固定的 gRPC Health descriptor 另有 Check、List 和 Watch，
  计入额外服务。两个 descriptor 的原始字节摘要和 10 个固定 proto 摘要均已记录。
- HTTP 核验原注册清单与当前测试目录的 263 个逐路由矩阵 ID，保留 Go 注册
  或 handler 源码字节。方法与路径为稳定键，复用模板的调度器各带来源类型。
  其中 HTTP 11 条、传递协议 3 条的原 owner 列是 102，正文或行号中的 102
  无归属意义。测试候选、历史回执和本轮运行分开存放。
- 第 19 项继续核验生产输入。对本项导致字节改变的三个审计文件，
  按相邻审计交接逐个核验旧值和当前值。原批次 19 回执保留原样。

## 当前运行边界

当前 Rust debug 构建同固定 kvproto Go oracle 配对运行。原始数据、精确命令、
退出码和构建指纹保存在机器回执。本轮四次精确测试各通过 1 项，绑定 22 条注册行。

- 七个传递方法在双方 classic 与四种独立 MCS 上共返回 70 个原始
  `Unimplemented`，逐项比较状态和文本。未知 HTTP 路径的状态均为 404，
  Go 返回文本，Rust 返回空体，分别保留原值。
- 四种独立 MCS 复验 Diagnostics 和 Health 的辅助注册。记录 protobuf 字节、
  流响应与错误文本。适用角色及输入由此场景限定。
- 三个旧版 GC 方法复验真实响应、follower、事务故障和重启后的物理键值。
  七个键空间 HTTP 注册行复验 classic/API 两种拓扑、真实 HTTP 与 RPC，
  保存重启前后的物理键值和修订号。
- RM 双流取消差异仍归批次 19 已记录的兼容缺口。其余行保留具体来源、
  责任和历史证据位置。完整 PD 及 HTTP 回归分别归第 22、23 项。

## 验证

| 命令或阶段 | 退出码与有效测试 |
| --- | --- |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_020_ -- --nocapture` | 初次 101，1 项因缺少当前审计失败。最终 0，2 通过，0 失败，0 忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_102_source_ -- --nocapture` | 0，2 通过，0 失败，0 忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_019_ -- --nocapture` | 0，2 通过，0 失败，0 忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 |
| `cargo metadata --format-version 1 --no-deps`、`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo check --workspace --locked`、`git diff --check` | 均退出 0 |

负向对照从真实 manifest 移除或复制原行、替换当前输入摘要、将已通过的配对回执
改成 metric-only。首次运行及审计校对期间的失败日志保留在机器回执。

使用共享槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
预检逐项核对 Go PD、SDK、kvproto ZIP、protoc、官方 etcd 3.5.15 和 etcdctl，
使用已存在且摘要一致的 kvproto 缓存。测试及子进程退出后释放锁，保留缓存。
1101 个 Go 源码和模块路径的首尾字节及 Git 状态一致。主计划原摘要核验通过。
