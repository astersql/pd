# 任务03：旧版GC安全点验收

状态：已完成。2026-11-05，生产基准`babbc44ce`；本项变更及最终源码摘要见
[task-03.json.gz](task-03.json.gz)。仅完成GetGCSafePoint、UpdateGCSafePoint、UpdateServiceGCSafePoint。

三个真实tonic入口和SyncGrpc显式登记，复用132的当前任期GCStateManager。
server适配只转换协议、取真实全局TSO和计算返回剩余TTL；单调推进、barrier、服务最小值、native_br及事务逻辑均留在canonical GC领域。
角色/cluster校验先于bootstrap检查；未bootstrap返回原NOT_BOOTSTRAPPED header，领域错误通过gRPC Unknown保留Go全文。
三个旧接口固定NULL scope，没有可传入的keyspace ID或management-type字段；不把v2的管理类型检查算作本组功能。
两个废弃watch继续返回37规定的精确Unimplemented错误，本组不新增watch成功路径。

前置27–47已顺序实现并提交，43–47分别保留原生及同源Go运行证据；132持有实际宿主任期、取消token和三重value/lease/revision存储fence。
本项不声称其余GCv2/Advance/barrier/批量HTTP、warmup48–51、整套控制面或完整工作区回归已经完成。

验证：

- 初始可编译双进程测试中，Rust旧RPC仍返回生成桩Unimplemented，行为断言失败；接线后通过。
- `cargo test --locked -p astersql-pd-compat-tests --lib sa_003_ -- --nocapture`：1项真实双实现测试通过，0 ignored。
  每个实现39个比较观察项，覆盖三接口未bootstrap、GC推进/相等重写/回退只读/超过txn、普通服务缩小及behind忽略、
  worker最小值及非法TTL、native_br全局映射、非法UTF-8服务ID、删除/empty、有限TTL及真实过期清理。
- 每次成功GC提交的GC revision counter递增，point与counter的etcd mod_revision相等且推进；兼容回退和错误保持完整bytes/create/mod revision。
  普通/worker服务变更逐次比较GC counter，ignored behind及非法参数保持原始entries。原始全路径、bytes、header/create/mod revision均保存。
- Get读取损坏的真实十六进制值，双方Unknown及完整strconv错误一致；Update遇到present-empty GC revision在真实etcd CAS失败，无写入；
  Service请求900KB合法protobuf在同配置的真实etcd max-request-bytes限额失败，双方Unknown及完整ErrEtcdTxnInternal文本一致，原entries不变。
  PD Go默认embedded限额是150MiB，最初900KB请求成功，不能当作故障证据；最终通过临时只含`max-request-bytes = 1572864`的TOML把Go与官方etcd fixture设为相同限额。
- 有限TTL分别使用真实TSO，绝对存储expiration保留在原始证据，断言落在TSO整秒+TTL至+TTL+1；TTL60删除后与对方完整bytes一致。
  TTL1等待真实TSO越过存储期限，再通过worker推进确认过期barrier实际删除；不伪造时钟或省略时间字段。
  39项比较只归一化独立cluster ID、经前后真实TSO夹定的无限minimum返回TTL；有限期限由上述实际区间和47的精确时钟字节对照共同证明。
- 两实现分别停止并重启真实etcd WAL及PD进程，GC bytes/create/mod revision完整保留，RPC重新读取一致。
  Rust使用既有外部官方etcd，harness重启不再误传embedded-only参数，并真实重启该fixture。
- 每个实现再启动真实follower（Go实际join embedded集群、Rust共享MetaStore的独立PD进程），三个RPC均Unavailable / `not leader`，GC entries不变。
- 三个有效read/no-op请求通过真实TCP deadline取消，双方Cancelled / `Timeout expired`，GC entries不变；RAII恢复暂停的PD进程。
  这证明协议取消，不承诺已发出的写入回滚：初次有效Service写入在Go超时后仍提交，原始失败保存。
  对实际mutating取消/未知提交结果，44、47已有响应gate、真实提交watch、停服、缓存失效和WAL证据；132另验证退役取消及旧事务不能覆盖新任期。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2通过、1既有ignored（需另行生成HTTP-v2 oracle，不计本项证据）。
- 相邻`task_037_`：2通过，废弃watch契约保持；`mcs_132_`：2通过。
- workspace check、metadata、fmt以及sa_002动态架构审计通过：79 packages、710 production sources、5负控制。
  完整workspace test按分项24集中执行；132包回归的环境/资源竞争和定向重跑完整保存，不改写成一次全包通过。
- Cargo每阶段原子领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，所有自有进程退出后释放锁。
  Go版本1.26.8，kvproto严格固定f8ad079c8e98，oracle SHA及官方etcd3.5.15、SDK和Rust二进制指纹附证据。
- 1101项Go/模块文件首尾存在性/SHA相同，Git状态无Go/模块变化；没有写Go、运行failpoint、make或生成器，plan.md及来源史诗不变。

Docker环境已具备官方golang:1.26.8-bookworm及pingcap/pd:nightly镜像。nightly协议版本不同且没有测试二进制，仅用于检查产物，未用作oracle；实际对照使用核验过的同源只读Go测试二进制和PD server。
