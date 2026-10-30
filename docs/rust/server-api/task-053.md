# 任务053：Store标签与权重HTTP

基准 `0662488f793c47b668b1c72755c840e77047bb47`。依赖[任务052](task-052.md)已满足。Go与模块文件、plan.md只读。

## 来源与实现

- [source-inventory](source-inventory.md)：`server/api/store.go` SetStoreLabel、DeleteStoreLabel、SetStoreWeight；`server/api/label.go` GetLabels。
- [wire-inventory](wire-inventory.md)：HTTP-065、066、067、075。HTTP-061是Region标签查询，依据任务70的明确边界修正归属为70，继续标缺差分；本任务不实现Region标签规则。
- [surface-inventory](surface-inventory.md)：router.go:228–230、242；审计归81，本文不宣称审计等价。
- [HTTP适配](../../../pkg/server/http_store_labels.rs)、[宿主写入](../../../pkg/runtime/store_labels.rs)、[独立网络测试](../../../pkg/server/http_store_labels_test.rs)。使用同一listener、leader转发、Coordinator、资源锁、请求取消和leader-fenced MetaStore。

复用core的标签合并与权重选项、schedule-config语法校验和StorageEndpoint持久化。严格位置标签校验由注册与标签更新共享。标签合并键不区分大小写；DELETE键精确匹配；`force=false`仍强制替换。保留engine保留键、空值删除、缺失与tombstone错误。GET labels保留键值拼接后不区分大小写去重及空列表null。

权重提交顺序与Go一致：leader权重、region权重、Store元数据各自提交；全部成功后发布内存权重。元数据失败不会发布内存权重，但此前已写入的权重可在重载后生效。这不同于非法输入：非法标签/权重在事务前拒绝，持久状态不变。

## 验证记录

首次红灯命令 `cargo test --locked -p astersql-pd-server --lib task_053_ -- --nocapture`：退出101，实际1项失败；缺少路由导致真实HTTP响应Content-Type断言失败，不是编译失败。其后接口接线编译错误保留原始输出，未冒充行为证据。

验证使用真实TCP、tonic、HTTP及etcd。事务故障复用任务52的真实etcd不存在lease注入；在实际写入边界暂停后取消节点并等待shutdown。差分入口为[只读监督脚本](../../../scripts/http-store-labels-oracle.py)，复用已有Go二进制，未生成或编译Go源码。

Go差分覆盖43项两端观察。只对标签数组排序（Go map迭代无稳定次序），权重统一为JSON float64；不忽略状态码、错误或持久化差异。测试通过真实RPC bootstrap和注册Store，再通过HTTP操作并在原MetaStore重启后验证状态。Go二进制build info没有VCS revision，不宣称它来自当前HEAD；外部etcd不冒称Go embedded-etcd拓扑等价。

回归准备中发现并修正测试输入：`location-labels`是逗号分隔字符串，数组被既有配置API拒绝；没有为测试修改配置API。取消时共享中间件可返回纯文本500，所以直接检查传输状态和持久状态，不以JSON助手的Content-Type断言替代取消结果。注册与标签更新的版本/地址准入校验以及错误格式现在共用同一实现；地址冲突错误保留合并前请求metadata。

## 最终检查

使用默认features，`GOMAXPROCS=2 CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=4`。动态原子领取共享槽位1；绝对 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。修改后先执行 `cargo fmt --all`。原始红灯、接线编译错误、测试准备失败、成功结果均保留，只有最后同一份源码的通过结果作为验收。

| 命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 包类型检查 |
| `cargo check --workspace --locked` | 0 | workspace类型检查 |
| `cargo test --locked -p astersql-pd-server --lib task_053_ -- --nocapture` | 0 | 3通过、0失败、1默认忽略（差分另行执行） |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-store-labels-oracle.py` | 0 | 显式 `cargo test --locked -p astersql-pd-server --lib task_053_differential -- --ignored --nocapture`：1通过、0失败、0忽略，43项两端观察一致 |
| `cargo test --workspace --locked` | 0 | 2077通过、0失败、51默认忽略（含doc-tests） |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 184通过、0失败、14默认忽略 |

开始/结束Go只读守卫覆盖1101个文件（含未跟踪文件），存在性、SHA-256及Git状态完全一致。最终源码快照覆盖1253个Rust/Cargo文件；验证后无源码修改。`plan.md`与基准一致。Python AST、证据Markdown链接、`git diff --check`均通过。

最终源码上的聚焦、Go差分、包级、workspace验证全部通过。进程、worker及自己的槽位锁已清理，槽位缓存保留。状态：已完成。

原始日志、命令退出码、两端观察、Go binary build info及快照：[task-053-evidence.json.gz](task-053-evidence.json.gz)。SHA-256 `9e9e5852afcff572fa995e04a98e9270367b58d40f6277d59868f0590438f6e4`。
