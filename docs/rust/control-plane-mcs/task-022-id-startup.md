# 任务022依赖修复：leader就绪前的ID窗口

补洞差分发现真实上游差异：连续范围没有生成新Region时，Go仍已有持久边界1000，
Rust却不存在边界。Go `server.go`在创建raft cluster之后、leader ready之前调用Rebase，
预留窗口但不消耗首个ID。现已补齐canonical Allocator::rebase及相同宿主调用顺序。

Rebase从真实持久边界读取并以原值/CreateRevision作CAS，事务仍受同一Leadership
lease/value/revision fencing；成功才发布本地window和指标，失败保留旧window。
保留Go的显式Rebase无符号回绕及namespace cap规则，不改变普通Alloc/SetBase契约。
不重试不确定提交；比较冲突有明确类型。宿主等待预留时继续续租，取消或失败会
join已创建cluster并清理请求指标，再释放任期；没有新增私有runtime或全局计数器。

该修复还暴露了未bootstrap时调度器提前消耗128个ID的启动竞态。
调度准入现以实际持久bootstrap状态为前置，bootstrap成功后开始准备计时，保留配置
命令的串行处理。节点测试主动等待调度状态、检查未初始化，并在首个AllocID前
读取真实边界1000，首个ID严格为1，没有放宽断言。旧暂停时钟夹具补真实bootstrap元数据。

## 验证

[全部阶段原始记录、命令、退出及来源SHA](task-022-id-startup-evidence.json.gz)。
保留最初边界不一致、测试共享取消令牌导致的错误分类、启动时ID129竞态及暂停夹具
缺少bootstrap的中断记录；它们均未计通过。最终：

| 实际命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-id | 0 | 12 lib + 5 integration通过；3新增Rebase、14既有相邻；0 doctests不计 |
| cargo test --locked -p astersql-pd-runtime --lib node_test:: -- --nocapture | 0 | 3通过，包含实际预留及bootstrap准备状态 |
| cargo test --locked -p astersql-pd-runtime --lib member_role_worker_test::task_007 -- --nocapture | 0 | 2通过 |
| cargo test --locked -p astersql-pd-runtime --lib start_cleanup_test:: -- --nocapture | 0 | 2通过 |
| cargo test --locked -p astersql-pd-runtime --lib embedded_isolation_test:: -- --nocapture | 0 | 2通过；既有1 ignored不计 |
| cargo test --locked -p astersql-pd-runtime --lib schedule_worker_test:: -- --nocapture | 0 | 7通过；既有1 ignored不计 |
| cargo test --locked -p astersql-pd-runtime --lib grpc_alloc_id_differential_test::readonly_alloc_id_differential -- --ignored --exact --nocapture | 0 | 1真实Go差分通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |

Go与Rust实际ID序列1、2、7、1012、1014及重启后的2003完整比较，删除旧取模和省略ID
的比较；耗尽响应逐字相同。原生etcd测试覆盖未消费的预留、抛弃旧窗口、坏边界、
真实租约撤销、SIGSTOP挂起I/O取消，以及64位回绕/cap。初始失败阶段的0 tests、
ignored和中断不算通过；新增3测试+1真实差分，另有30相邻，重复阶段去重。

动态槽位1，CARGO_TARGET_DIR为`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
自有进程结束后释放锁、保留缓存。1101项Go/模块文件存在性、SHA及Git状态一致。
仅该小型上游ID依赖执行包级测试，控制面和workspace全量仍依用户授权统一执行。
