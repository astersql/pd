# Rust 成员与 ID 分配

`astersql-pd-member` 和 `astersql-pd-id` 只依赖后端中立的 MetaStore、
共享 Tokio runtime 和调用方提供的 CancellationToken，不启动独立 runtime、
信号处理或自动续租任务。调用方负责周期性 `renew`，关闭时调用 `resign`；
忘记关闭由服务端 TTL 最终回收，取消后仍可显式 resign。

## Go oracle 与持久化决策

依据 `pkg/member/member.go:InitMemberInfo`、`pkg/election/leadership.go`、
`pkg/election/lease.go`、`pkg/id/id.go` 与 `pkg/id/id_test.go`：

- Go leader value 是 pdpb.Member 的 wire bytes。Rust 领域层接收不透明 identity bytes，
  由未来网络适配器提供相同的序列化值，不依赖 protobuf 类型。
- leader 保留 `/pd/{cluster_id}/leader`，使用 lease 绑定的 create-if-absent。
  每次操作同时比较 identity、lease ID 和 leader mod_revision，防止同名重选 ABA。
- `/pd/{cluster_id}/alloc_id` 保留 Go 的大端 uint64 格式。默认预留 1000 个 ID，
  分配从持久化边界的下一个 ID 开始；重启放弃未使用窗口，所以允许间隙。
- Rust 新增 `/pd/{cluster_id}/rust/members/{member_id}` 保存不可变 identity bytes。
  相同 identity 可恢复，冲突 identity 显式报错；不改写 etcd 自己的成员目录。
  此 bootstrap 是 PD 领域身份注册，不负责 etcd 集群加入、生成 cluster ID 或网络发现。
- 当前 API 分配单个全局 ID；Go 的 keyspace allocator、SetBase 和多 ID 请求不在本任务范围。
  u64 上界采用饱和预留，最后一个 ID 可分配，之后显式 Exhausted，禁止溢出回退。

每个 allocator 用异步 mutex 串行消费自己的窗口，多个实例通过持久化 revision CAS
竞争不相交的窗口。只有比较冲突会重试；传输错误可能对应已提交的写入，直接返回，
下一次从持久化边界恢复。损坏边界返回 CorruptBoundary，不覆盖已有数据。
每次分配（包括缓存命中）执行线性化的 leadership 校验；预留事务再次原子 fencing。
已撤销或自然到期的 lease 不能继续写入或消费缓存。与 lease 撤销并发的操作按
事务的线性化点排序；不会承诺响应送达时仍持有 lease。

Go allocator 的测试性质是唯一、单实例递增及窗口恢复；跨活跃实例有独立窗口，
不保证按响应到达顺序全局递增。Rust 用同样的性质验收，并额外检查每次缓存命中时
的 lease fencing。Go oracle 命令（自动启用并关闭 failpoints）：

```sh
make gotest \
  GOTEST_ARGS='./pkg/id -run "TestMultipleAllocator|TestIDAllocationEndValue" -count=1'
```

## 验证

先按计划领取共享 Cargo 槽位并设置绝对 CARGO_TARGET_DIR，然后运行：

```sh
cargo test --locked -p astersql-pd-member -p astersql-pd-id -- --nocapture
```

内存与真实 etcd 使用相同断言；复用 storage 的真实 etcd v3.5.15 夹具，覆盖注册恢复、
竞争选主、续租、撤销及自然到期、旧 term 拒绝写入、取消、并发 ID、allocator 重建、
损坏边界、u64 耗尽，以及真实 etcd 以相同 WAL 重启后的身份与 ID 恢复。
Go fixture 只用于测试，不进入 Rust 生产产物。
