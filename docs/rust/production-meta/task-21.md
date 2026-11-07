# 生产任务 21 跨后端 MetaStore 契约

原生与真实 etcd 运行相同的 11 个契约断言和 5 个扩展断言。共享用例覆盖 CAS、
单 revision 原子事务、历史分页、watch 回放和取消、压缩边界、删除前值、
租约到期/续期/撤销、非法请求的原子拒绝和 fencing。扩展用例覆盖 revision
保护、分支、点读、范围和比较。真实 etcd 对照固定为 3.5.15，fixture 不走
Go 构建分支。
[共享断言](../../../pkg/storage/kv/contract_test.rs)、
[原生登记](../../../pkg/storage/kv/native_contract_test.rs)、
[etcd 登记](../../../pkg/storage/kv/etcd_store_test.rs)。

补充三节点用例检查隔离的旧 leader 拒绝线性读写，另一任期接管同值租约后
拒绝旧租约条件。触发快照和日志截断，再验证新 learner 安装及重开都保留新租约
和压缩水位。首次等待 learner 应用时失败，实际错误是快照内 JSON 数字键解码。
修复保留原快照格式，显式解析 revision 和租约 ID。
[故障用例](../../../pkg/storage/kv/native_failover_test.rs)、
[解码实现](../../../pkg/storage/kv/native_state.rs)。

通过 `cargo test --offline --locked --release -p astersql-pd-storage-kv --lib --no-run`
在槽位 1 构建同机性能、故障和长稳夹具。复制该二进制后运行
`FIXTURE --test-threads=3`，退出码 0。64 项通过，3 项只作子进程入口。
运行环境使用绝对 `CARGO_TARGET_DIR`，构建目录为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`。本轮 release 单元测试复用
已校验的 etcd fixture，并以新临时目录承载数据。
