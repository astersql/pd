# 任务02：工作区依赖方向审计

状态：已完成。2026-11-05。基准 HEAD `263a2703e`。
上游任务01的责任交接已完成；coverage.json没有分配给02的业务来源行，
本项承接历史 task-102.md 第333–336行的架构审计责任，不代替业务准入。
原始 metadata、679个源码SHA-256、输入摘要、首尾Go守卫和完整日志见
[task-02.json.gz](task-02.json.gz)。本轮metadata包含78个package。

## 结果与适配边界

新增显式注册的 `sa_002_dependency_direction`，读取本轮真实Cargo metadata，
遍历全部workspace包的非dev依赖（含build/条件依赖），拒绝领域到server、runtime、
compat harness及proto的反向边；core额外拒绝直接传输和具体后端依赖。
每条边逐一检查，因此域内传递路径也不能藏入反向边。
包名、manifest和lib目标源码唯一性检查防止重复canonical注册。
源码扫描覆盖pkg/server/cmd下679个非*_test.rs、非build.rs文件，
禁止领域协议引用、库内常见runtime/全局信号/日志/进程退出调用，根facade只允许重导出。
这是明确拼写的静态回归守卫，不是完整Rust语义分析或领域算法等价证明。

显式适配范围按ADR审查：server负责HTTP/gRPC转换，runtime负责宿主装配，
proto负责生成协议；storage-endpoint的endpoint_codec负责protobuf持久化转换，
storage-kv的tonic仅由EtcdMetaStore后端适配使用，不能据crate名称自动判错。
storage-domain通过path复用kv/meta_store.rs等中立实现，kv/lib.rs重导出同一契约；
endpoint facade调用canonical domain而非重写规则。domain/lib.rs复用的endpoint
辅助模块也在源码扫描集合内；未发现协议引用。runtime/node.rs接收宿主Handle和
MetaStore，bin拥有进程资源。没有发现本项需要修复的生产依赖边，未改领域算法。

真实图/源码副本的5种负向控制均正确拒绝：注入core→runtime、复制canonical包、
领域公开proto引用、库私建runtime、根facade重复实现。测试未提前返回或ignore。
本项是架构审计，没有RPC/HTTP及耐久性结果需要重放；不宣称GC/MCS或原生后端完成。

## 本轮验证

实际槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
槽位以原子mkdir动态领取；所有Cargo子进程继承此绝对路径。

- `cargo metadata --format-version 1 --no-deps`：退出0。
- `cargo fmt --all`、`cargo fmt --all -- --check`：退出0。
- `cargo test --locked -p astersql-pd-compat-tests --lib sa_002_ -- --nocapture`：
  初次退出101，proto build找不到Go/cache，0测试执行；不是断言失败。
  读取准入文档，核对既有ZIP摘要后设置`GOMODCACHE=/tmp/pd-task1-go-env/mod`；
  重跑退出0，1通过/0失败/0忽略，4负向控制。增加facade守卫后的最终重跑退出0，
  1通过/0失败/0忽略，5负向控制；`SA_002_REPORT`保存本轮输入。
  纯审计按计划无需人为破坏实现制造红灯。
- `cargo test --locked -p astersql-pd-compat-tests --lib schedule_foundation_dependency_direction -- --nocapture`：
  退出0，1通过/0失败/0忽略。
- `cargo check --workspace --locked`：退出0；既有server警告保留原始日志。
- `git diff --check`：退出0。完整workspace测试按计划留给任务24。

Go/模块1101项首尾摘要一致、受保护文件Git状态为空；未运行Go、make或failpoint。
所有Cargo进程结束后释放自己的slot-1.lock，占用记录清理，编译缓存保留。
原plan、来源史诗、领域plan均只读。用户已有旧计划删除和本计划未跟踪文件不纳入提交。
