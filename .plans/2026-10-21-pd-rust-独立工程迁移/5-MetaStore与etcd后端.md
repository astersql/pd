# 任务 5: 实现 MetaStore 契约与 Etcd 过渡后端

批次：【批次 5】 依赖批次 4

状态：未开始

目的：把 PD 对 etcd 的真实语义收敛成可替换的领域接口，并用 Etcd 后端证明事务、revision、watch、lease 和选主行为。

来源任务：无

预计会话范围：只实现元数据接口、内存测试模型和 `EtcdMetaStore`；不启动完整 PD，不实现 Rust Raft。

## 文件

- 新建：`pkg/storage/Cargo.toml`
- 新建：`pkg/storage/lib.rs`
- 新建：`pkg/storage/meta_store.rs`
- 新建：`pkg/storage/memory_store.rs`
- 新建：`pkg/storage/etcd_store.rs`
- 新建：`pkg/storage/meta_store_test.rs`
- 新建：`pkg/storage/etcd_store_test.rs`
- 修改：`Cargo.toml`

## 上下文

- 先从现有 `pkg/storage`、`pkg/election`、`pkg/member`、`pkg/gc` 和 `pkg/keyspace` 读取实际 etcd 用法，不凭抽象习惯扩展接口。
- 接口至少表达 linearizable/serializable read、compare-and-swap transaction、revision、prefix scan/watch、lease/TTL 和 compaction/watch 恢复。
- 领域层不得暴露 `etcd_client` 类型；后端错误要映射成稳定、可判断的错误枚举。

## Cargo 共享槽位规则（仅 Rust/Cargo 任务）

1. 同一仓库的所有计划和任务共用仓库根目录下的 `target/rust-slot-1` 至 `target/rust-slot-10`，不得为任务、会话或计划另建构建目录，也不得按任务编号取模固定分配。
2. 每次构建/验证阶段开始前，创建仓库根目录下的 `target/rust-slot-locks`；依次用原子命令 `mkdir target/rust-slot-locks/slot-N.lock` 尝试领取槽位，`N` 为 1–10。仅 `mkdir` 成功者获得该槽位；这些相对路径命令从仓库根目录执行。
3. 领取成功后，在自己的锁目录中记录计划、任务、会话标识和持有进程 PID。同一槽位同时只允许一个持有者；已占用时尝试下一个，仅全部 10 个槽位都占用时等待并重试。
4. 将 `CARGO_TARGET_DIR` 设置为领取槽位的绝对路径，例如领取槽位 3 后，从仓库根目录执行 `export CARGO_TARGET_DIR="$PWD/target/rust-slot-3"`。示例编号不代表固定分配。
5. 所有 Cargo 构建、检查、测试以及调用 Cargo 的脚本和子进程均继承该设置，不得通过 `--target-dir` 或其他配置绕过槽位；生成的二进制也从该槽位定位。
6. 该阶段的 Cargo、脚本以及使用槽位产物的子进程全部结束后，删除自己写入的占用记录，并用 `rmdir` 释放自己的锁目录；保留槽位编译缓存供后续任务复用。
7. 正常结束、失败和中断均须清理自己的锁；在后台进程或其他使用槽位产物的进程仍运行时不得提前释放。规划清理步骤时覆盖这三种退出路径。
8. 遗留锁须先核实持有会话及相关进程已结束；无法确认则保持占用，不得删除其他活跃任务的锁、进程或缓存。
9. 生成或更新计划时，统一移除与这些规则冲突的单一 Rust 共享构建空间、逐任务独立目录和 Cargo 缓存全局独占窗口要求；仍须遵守源码文件占用、真实业务依赖和其他共享验证资源的互斥，不能仅因缓存分槽就把任务判定为可并行。
10. 最终验证记录包含实际槽位、绝对路径 `CARGO_TARGET_DIR`、确切命令、退出码和有效测试数量；Rust 代码修改后先运行 `cargo fmt --all`，再运行相应验证。编译成功或零测试不能替代行为验收证据。

## 测试计划

- 行为：同一契约测试套件在内存模型和真实 etcd 后端上得到一致结果。
- 失败验证测试：`meta_store_test.rs` 先定义 CAS 冲突、watch revision、lease 到期和 compaction 后恢复场景。
- 失败验证命令：`cargo test -p astersql-pd-storage --lib meta_store_test -- --nocapture`
- 预期失败原因：trait 和后端尚不存在。
- 通过验证命令：`cargo test -p astersql-pd-storage --lib -- --nocapture`
- 模拟策略：内存模型用于确定性单测；Etcd 后端测试必须启动真实隔离 etcd，不 mock 网络响应。

## 步骤

1. 从 Go 调用点形成最小语义清单。
2. 先编写后端无关契约测试。
3. 实现内存模型，使契约定义可快速执行。
4. 实现 Etcd 后端并复用同一测试套件。
5. 加入连接中断、watch 重连、lease 失效和取消测试。

## 验证

- 运行：`cargo test -p astersql-pd-storage --lib -- --nocapture`
- 运行：`cargo test -p astersql-pd-storage --test etcd_contract -- --nocapture`
- 预期：两个后端通过同一非零契约测试集，资源和 watch task 无泄漏。
- 所需证据：实际槽位、真实 etcd 版本、命令退出码、测试数量和故障场景日志。

## 完成

`EtcdMetaStore` 是过渡后端，不得被文档描述为最终单二进制方案；接口也不得机械复制完整 etcd API。

