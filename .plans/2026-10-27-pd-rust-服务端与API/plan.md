# PD Rust 服务端与 API 计划

目标：把来源主题7拆为可独立验收的小任务，补齐真实服务装配、PD协议、管理API与生命周期证据。

范围：扩展现有 server/runtime/proto/tso 与 pd-server；覆盖集群worker、classic PD方法、HTTP v1/v2适配、配置、TLS、认证、限流、审计接线、指标、join预检、独立及嵌入生命周期。每项以来源矩阵和真实差分验收。

范围外：Dashboard、独立MCS、主题8领域内核、主题9运维实现、Go客户端移植及主题10原生后端；不实现Go embedded-etcd或Go插件ABI。

假设：基准 `5d389785abd44b5bbd9d826b24bc454e6e7e2f5c` 已有workspace、心跳/同步适配及调度内核；runtime仍为最小纵切。来源任务未开始；主题6证据入口为 `docs/rust/schedule-execution/task-156.md`，准入仍须核验。

## 设计决策

- 比较全量重写、另建平行服务、扩展现有canonical crate；采用第三种，减少协议与状态分叉。
- 比较按目录整包迁移与按行为接线；采用按行为拆分，复用真实领域层。

## 架构说明

- domain/storage/schedule → cluster → server → runtime → bin；proto止于适配层。
- 宿主拥有handle、取消、Registry及listener；MetaStore保持外部etcd过渡模式。

## 开发策略

- 对行为变化执行失败验证、最小实现、通过验证、重构；测试独立于生产文件并显式注册。
- 成功、错误、取消、持久化副作用均须真实验证；桩、编译成功、零测试和源码推导不能替代运行差分。
- 按现有crate扩展传输适配，工具只迁移调用链确实需要的行为，不为目录齐全创建空crate。
- 来源主题8/9领域依赖按矩阵移交，适配层不得重新实现；未满足时明确拒绝且保留兼容缺口，不声称主题7整体完成。Go embedded-etcd拓扑属于主题10，不能用外部etcd伪装等价。
- Go只读硬约束适用于所有任务；禁止make测试入口、failpoint启停和间接Go写入。现有 `scripts/run-pd-compat.sh` 不可直接执行，先完成只读入口任务。
- 槽位规则在各Cargo任务中定义；规划阶段不调用Cargo。所有验证从仓库根目录执行，默认features，不自行添加feature开关。
- 计划只读，进度仅写编号任务文件，运行证据写 `docs/rust/server-api/`。来源史诗及其plan/prompt保持不变。

## 参考

- 来源：`.plans/2026-10-22-pd-rust-渐进迁移主题/7-补齐PD服务端与API.md`。
- 架构：`docs/rust/architecture.md`、`docs/rust/dependencies.md`、`docs/rust/porting-guide.md`。
- 当前装配：`pkg/server/lib.rs`、`pkg/runtime/node.rs`、`cmd/pd-server/main.rs`。
- 当前验证：`tests/rust-compat/Cargo.toml`、`tests/rust-compat/harness.rs`、`pkg/storage/kv/etcd_fixture_test.rs`。
