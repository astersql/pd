# 任务003：只读协议差分入口

本任务扩展既有supervisor和真实进程harness，不复制领域实现。
实现为 tests/rust-compat/oracle_runner.rs，独立测试在 oracle_runner_test.rs，
lib.rs显式注册；只新增workspace已有sha2的测试依赖。

输入门槛：Go PD摘要固定于任务001准入记录；Rust摘要取本轮构建产物并要求前后相同；
fixture摘要固定于编译时include_bytes，运行前后重新比对。既有supervisor强制
固定kvproto、dual模式、不同二进制、共享槽位、非空fixture。
当前runner只调度supervisor已明确支持的case；未知领域case返回失败，不能借用
相似case冒充该领域差分。

回执校验要求真实非空Go快照前后相同、全部输入哈希一致、恰好一个有效场景测试、
零失败/忽略、进程组及目录回收。负向控制基于真实回执变异，不把合成回执算作业务回放。
supervisor将自身、runner、测试、readonly validator和harness也加入输入身份守卫。

首次行为运行已编译可执行，真实bootstrap场景1通过；新增回执路径校验随后失败，
因为macOS /tmp与/private/tmp是同一文件的路径别名，报告保存解析后的路径。
修复使用canonicalize定位实际文件并保留SHA比较，没有放宽身份或业务字段断言。
失败日志及退出码保存在本任务压缩证据中；不是以编译错误证明缺陷。

## 本轮验证

完整命令、输入SHA、原始失败/成功日志和真实回执保存在
[task-003-evidence.json.gz](task-003-evidence.json.gz)。

| 命令 | 退出 | 实际结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | Rust格式化 |
| cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server | 0 | 当前独立Rust PD二进制 |
| cargo test --locked -p astersql-pd-compat-tests --lib mcs_003_ -- --nocapture（首次） | 101 | 1通过、1失败；业务已运行，回执路径断言失败 |
| 同一聚焦命令（修复后，最终源码） | 0 | 2通过、0失败、0忽略；3组supervisor各恰好1个实际场景通过 |
| cargo test --locked -p astersql-pd-compat-tests --lib task_004_ -- --nocapture | 0 | 3通过、0失败、0忽略，相邻输入守卫 |
| cargo check --locked -p astersql-pd-compat-tests | 0 | 编译检查 |
| cargo metadata --format-version 1 --no-deps | 0 | 75个package |
| cargo fmt --all -- --check | 0 | 格式检查 |
| cargo check --workspace --locked | 0 | 工作区编译检查 |
| cargo test --locked -p astersql-pd-compat-tests | -15 | 有意中止，未完成，不计通过 |
| cargo test --workspace --locked --no-fail-fast | 未运行 | 留待最终统一回归 |

最终3组实际回放：health-member启动真实Go和Rust独立进程，验证HTTP观察及leader键、
端口/目录清理；bootstrap和cluster-restore复用真实Go进程及Rust PdNode/TCP端点，
分别保留8种无效bootstrap、成功/重复、持久对象，以及同一WAL恢复和坏region拒绝观察。
后两组不是独立Rust进程，不能将仅校验独立Rust二进制描述为每组都启动了该进程。
各组原始业务输出/请求实现及响应观察保存在回执日志和输入身份记录中；
只认证已有场景的具体字段，不扩张为GC/keyspace/MCS已经兼容。

完整兼容包中，Go客户端、23组协议矩阵及协议负向控制已经实际通过；
HTTP矩阵尚未全部执行。用户于2026-11-04明确选择“聚焦及相邻回归通过后推进依赖，
全部实现后统一全量回归”，故停止这一轮重复全量验证，保留中断记录。
停止时先暂停槽位持有者，结束其9个后代及全部自有进程组，确认没有活动worker，
再恢复持有者释放槽位；未触及其他会话的进程或锁。
该中断不是测试失败的豁免，也不能当作兼容包或工作区通过；最终回归仍为验收门槛。

各阶段动态领取槽位1，CARGO_TARGET_DIR为
/Users/xiangmin/worker/code/pd/target/rust-slot-1，正常及中断阶段均已释放自己的锁。
环境沿用admission.md：已有Go SDK仅检查build info、固定协议ZIP和vendored ARM protoc、
官方etcd3.5.15及相邻etcdctl、PD_COMPAT_MODE=dual、RUST_TEST_THREADS=1。
Go源码/模块1101项存在性、SHA及Git状态首尾相同；未生成Go文件或运行Go构建/测试。
git diff --check退出0。实现和聚焦/相邻验证完成；保留任务文件待最后统一全量回归。

