# 任务015：Region边界加密

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

## 实现与证据边界

`region_crypter`使用同一个AES-CTR流连续处理start/end，保留跨边界的半块状态，
支持128/192/256位材料及完整大端计数器回绕。加密只改变副本的边界和加密元数据；
禁用时返回原借用对象。解密完成后移除元数据；密钥和IV校验在修改之前完成。
历史key在current=0时仍可解密；缓存未命中通过真实字典存储刷新。
保留Go的nil、重复加密、缺失管理器、未知key、未知method和AES材料长度错误顺序。
损坏IV在Rust返回错误，文本对应Go的panic；不会复制Go进程崩溃行为。

StorageEndpoint的保存、点读和扫描统一调用该实现。节点从listener安全配置建立
一个根生命周期字典/watch，角色使用真实Leadership轮换并在恢复Region前完成装配。
heartbeat持久化、bootstrap后的二次保存、重启恢复和历史数据查询共享字典。
遥测配置覆盖不替代加密配置。旧任期退出会取消正在等待的密钥I/O，再等待操作结束，
避免关闭时被后端故障挂住；迟到的旧任期清理不取消新任期。

历史记录使用已有`/pd/<cluster>/rust/hot-history/<member>`私有命名空间，
其存储密文使用显式base64-v1编码，API继续返回原明文字符串。此格式不声称兼容
Go的LevelDB history JSON格式。HotHistory通过宿主codec接受加密，领域存储不依赖协议。

## 只读Go互通

来源：`pkg/encryption/region_crypter.go`、`pkg/storage/endpoint/meta.go`、
`pkg/storage/leveldb_backend.go`及`server/server.go`，SHA保存在归档。
已有Go1.26.8 oracle和固定kvproto使用任务1/3已核验的二进制；没有Go构建或源码写入。
Go SDK保存的三组NIST CTR向量覆盖不同start/end分割位置，来源文件SHA再次核对一致。

真实Go PD启用file master key和Region LevelDB存储，接收非块对齐边界的gRPC heartbeat。
正常停机并flush后，仅在该自有夹具目录用Rust读取实际LevelDB记录：Rust解密Go密文
并比较整个原始Region。再写入同key ID下Rust新IV密文，关闭数据库，Go重启从同一
目录恢复出完全相同的Region（epoch、peers、flashback等字段逐项相等）。
`rusty-leveldb =4.0.1`仅是runtime的测试依赖，不引入生产本地持久层；API依据
[官方crate文档](https://docs.rs/rusty-leveldb/4.0.1/rusty_leveldb/)。

Go默认etcd StorageEndpoint没有传入加密管理器，因此初始etcd密文断言失败是夹具
假设错误；最终互通使用其真实加密LevelDB路径，不把etcd明文记录当作加密证据。

## 验证

[完整阶段日志与来源SHA](task-015-evidence.json.gz)保留实际命令、退出码和原始输出。
最初red编译后在启用密钥却没有Region加密元数据的行为断言失败。
后续导入/类型编译错误及宿主async布局深度问题均保留；后者在主密钥工厂I/O边界
Box::pin解决，不提高全局递归限制。双向流夹具等待首包前响应头的挂起运行已中止，
未算通过；最终夹具并发发送首包并设观察超时，真实Go双向互通通过。

| 实际命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_01 -- --nocapture --test-threads=2 | 0 | 29通过：5本项+24相邻，0忽略 |
| cargo test --locked -p astersql-pd-storage-endpoint --lib region_ -- --nocapture | 0 | 6通过：1本项+5相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_015_ -- --nocapture | 0 | 3本项通过：Go互通、原生节点重启、历史key兼容 |
| cargo test --locked -p astersql-pd-runtime --lib wire_region_heartbeat_test:: -- --nocapture | 0 | 1相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib member_role_worker_test:: -- --nocapture | 0 | 2相邻通过；既有1 ignored不计通过 |
| cargo test --locked -p astersql-pd-runtime --lib cluster_host::hot_history::tests:: -- --nocapture | 0 | 2相邻通过；最初错误过滤0项不计通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 packages |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo tree -p astersql-pd-encryption -e normal | 0 | 无PD proto、etcd-client和server普通依赖 |

去重为9聚焦+34相邻通过。动态领取槽位1，CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`，已结束自有进程并释放锁、保留缓存。
1101个Go/模块文件存在性、SHA及Git状态与起始保护基线一致。
既有锁文件包版本无升级；新增LevelDB依赖限测试用途。
完整包级/workspace测试依用户授权在全部实现后统一执行，保留任务待回归。
