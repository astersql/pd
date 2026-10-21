# MetaStore 契约与过渡后端

`astersql-pd-storage` 位于 `pkg/storage`。公开接口只含领域类型，
不暴露 etcd/protobuf。`MemoryStore` 是无持久化、按操作推进到期的测试模型；
`EtcdMetaStore` 使用外部 etcd，是过渡后端，尚非最终单二进制方案。

## Go 行为依据

- `pkg/storage/kv/etcd_kv.go`：默认 linearizable read、range/prefix scan、
  原子 compare/then、冲突不写入。
- `pkg/storage/kv/etcd_kv_test.go`：读写、range 与事务基准。
- `pkg/election/leadership.go`：create-revision=0 抢占 leader；
  `LeadershipTerm.Comparisons` 同时比较 leader value 和 lease ID，防止同名重选 ABA；
  watch 从指定 revision 开始，断连后恢复，压缩时显式处理。
- `pkg/election/lease.go`：grant、keepalive、revoke，以及过期后失去 leadership。
- `pkg/member/member.go`：leader-fenced 元数据更新。
- `pkg/gc/enabled_keyspace_loader.go`：同 revision 分页读取。
- `pkg/keyspace/keyspace.go`：事务更新元数据，不能出现部分写入。

固定 oracle 为 Go 模块中的 etcd v3.5.15，测试直接运行该真实服务。
`contract_test.rs` 的同一套断言分别驱动内存模型和真实网络后端；不模拟响应。
只覆盖 PD 需要的相等比较、原子写入、prefix 扫描/分页、watch、lease 和 fencing，
不复制完整 etcd API。

## 语义与调用约定

- revision 从 1 开始；一个有写入的事务使用同一 revision。比较失败或删除不存在
  的 key 不增加 revision。删除后重建获得新的 create revision。
- read 默认为指定的一致性；`revision=None` 表示最新。历史读返回的 header revision
  仍是服务当前 revision。分页应固定首个快照 revision，并使用 prefix 内的 inclusive
  start；调用方负责计算上一页最后 key 的后继。
- 所有比较取事务执行前的状态；缺失 key 的 revision/lease 为 0，value 为空。
  一个事务不能重复修改同一 key；输入及 lease 检查失败时全部回滚。
- watch 起点必须是正数且 inclusive。快照后的 watch 从 `snapshot.revision + 1`
  开始。返回 batch 的 revision 来自最后一个事件，不能取创建/进度响应的 header。
  batch 不拆分一个 revision；重新订阅从 `batch.revision + 1` 开始。
- watch 是 pull API，没有独立后台 worker；drop 关闭流，CancellationToken 使其永久
  终止。Unavailable 后可再次 next，adapter 从最后交付位置重连；调用方负责退避。
  Compacted 必须重新取得快照，再订阅新 revision，不能假装已收到丢失的历史。
- grant/keep_alive 是显式请求；不会自动无限续租。TTL 秒值允许服务端取整。
  到期或 revoke 删除附属 key，产生有 revision 的 delete 事件。
- 选主使用 lease 绑定的 create-if-absent；后续写入同时比较 value 和 lease，
  不能只比较成员名称。禁止在收到 Unavailable 后盲重试 mutation：结果可能已提交。
- RPC 有调用方设置的 timeout；watch 空闲等待不受 RPC timeout 限制，可取消。
  库不创建 runtime、全局 subscriber 或信号处理器。

## 执行测试

先按任务规则领取 `target/rust-slot-locks/slot-N.lock`，设置绝对路径
`CARGO_TARGET_DIR="$PWD/target/rust-slot-N"`。需 Go 与 protoc，etcd-client 的
build script 使用 PATH 中的 protoc（也可设置 PROTOC）；无需预装 etcd 可执行文件。

```sh
cargo fmt --all
cargo test --locked -p astersql-pd-storage --lib -- --nocapture
cargo test --locked -p astersql-pd-storage --test etcd_contract -- --nocapture
```

真实集成测试自动 `go build ./scripts/rust-etcd-fixture`，在所持槽位创建 fixture
二进制和每例独立数据目录，并启动随机 loopback 端口的 etcd v3.5.15。测试结束关闭
子进程并清理数据，失败保留目录内日志。故障测试关闭并以同一 WAL 重启 etcd，
验证连接失败分类、watch 恢复、已提交数据和 lease 最终失效；同时验证取消。
该 Go 进程只用于测试 oracle，绝不链接进 Rust 生产产物，也不构成 Go FFI。
