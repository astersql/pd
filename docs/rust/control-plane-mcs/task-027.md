# 任务027：元服务分组分配的实现推进

状态：已完成。2026-11-05；实现基准`4d4695aeb798f33a70d583627c29c7aedcd3f9a5`，
续验基准`7a9cfd191`。用户已明确授权隔离构建和运行现有Go测试，源码和模块仍只读。
先前直接AssignToGroup逐例oracle缺口已解除，任务28可继续；这不代表GC验收完成。
原始日志、请求响应、输入指纹、Go守卫、CI及Docker核查见
[task-027-evidence.json.gz](task-027-evidence.json.gz)。

## 实现范围

`MetaServiceGroupManager`复用宿主MetaStore，选择enabled且计数最小的分组。
PickGroup按当前Go实际实现预留一次计数（GoDoc“without updating”已与源码和测试不符）；
AssignToGroup拒绝负数，在一次受读值保护的事务中更新计数。
不自动重试冲突或未知提交结果。并列最小值只保证合法最小分组，不保证Go map遍历顺序。
64位加减使用wrapping，沿用既有amd64 Go oracle数值合同。

提供调用方事务内的reservation方法：回调失败或CAS冲突时，预留与其他缓冲写一起丢弃。
这是供后续创建接线使用的组合入口，并不声称已接通Go的两阶段创建流程。
单独rollback用于补偿已提交的预留；空组或已删除/零计数组不重建状态，计数下限为零。
成员目录在本分配handle中不可变，动态分组更新和宿主接线由原拥有任务继续实现。

既有JSON状态类型及存储方法迁入storage-domain；endpoint保留同类型重导出，
其Deref继续复用canonical方法，避免领域依赖protobuf适配crate或复制JSON解析规则。
没有协议、后台任务、私有运行时或新全局状态；没有GC gRPC层算法副本。

为使领域future可被Tokio服务任务发送，将既有CallbackTxn的创建和提交提取为
`Base::begin_txn`及消耗事务的`CallbackTxn::commit`，原run_in_txn调用同一提交逻辑。
没有复制事务算法。manager绕开generic async callback的Send生命周期限制；
编译期Send守卫及八个真实tokio::spawn reservation验证服务接线可用。

## 本轮验证

每阶段原子mkdir动态领取实际槽位1：
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
全部Cargo和fixture子进程退出后删除自有owner记录、释放锁，缓存保留。
PROTOC、etcd路径和SHA使用准入记录核验；真实etcd为官方3.5.15，无Go fixture回退构建。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 最终格式化通过 |
| cargo test --locked -p astersql-pd-keyspace --lib mcs_027_ -- --nocapture | 101 → 0 | 初始可运行red为0通过/2断言失败；随后3通过；最终包级包含4项本任务测试 |
| cargo test --locked -p astersql-pd-keyspace | 0 | 最终14通过、0失败、0忽略，含4本项及10相邻；0 doctests不计 |
| cargo test --locked -p astersql-pd-storage-kv --lib callback_txn -- --nocapture | 0 | 2通过（Memory及真实etcd原契约），0失败、0忽略 |
| cargo test --locked -p astersql-pd-storage-endpoint --lib meta_service_group -- --nocapture | 0 | 3通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-compat-tests --lib mcs_027_readonly_go_reservations -- --ignored --nocapture | 0 | 显式执行环境测试，1通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-compat-tests --lib sa_002_ -- --nocapture | 0 | 1通过，5负向控制，78包、681生产源码 |
| cargo metadata --format-version 1 --no-deps | 0 | 78包 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过；保留原有server弃用警告 |
| git diff --check | 0 | 通过 |

初次测试接线出现AsyncFnOnce生命周期编译错误，未计为red；改用同一runtime上的join
并发后才取得两项真实行为断言失败：PickGroup后计数仍0、并发成功数量与持久总数不符。
修复后真实etcd覆盖count=5、负数、禁用/空组、失败回调、补偿不复活删除状态、确定性
两事务竞争（仅一个计数及对应键空间写提交）、并发计数、停服失败及同WAL重启恢复。
没有以MemoryStore或调用次数替代持久验证。最终去重21项通过，包含4项本任务行为、
10项键空间相邻、3项JSON存储、2项原事务契约、1项实际Go创建差分及1项架构守卫。
完整workspace test依上层计划保留给任务24。

Go差分显式运行已核验的固定f8ad079协议既有二进制，复用readonly_oracle和进程harness。
真实gRPC bootstrap后，通过HTTP创建三次键空间，逐次比较Go组状态与Rust真实etcd中的
reservation计数1/2/3；重复创建返回500后Go计数仍3。原始响应和Rust状态已保存。
这只证明经创建路径调用的单次reservation，不能代替任意count或负count的直接调用。

Go源码/模块1101项首尾存在性、SHA与受保护路径Git状态一致；未构建/测试/生成Go，
未运行make、failpoint、模块整理。新增依赖只作用于Rust Cargo；锁文件只保留本项新依赖边。

## 历史外部证据边界（下方续验已解除）

已保存的CI raw日志和本轮浏览器读到的1430行完整日志都只有
`TestMetaServiceGroupTestSuite`汇总，缺少`TestAssignToGroup`、
`TestAssignToGroupRejectsNegativeCount`、`TestPickGroup`的独立运行记录。
CI测试文件与本地摘要完全一致，但生产文件回滚实现不同，差异在证据包保留。
不从suite汇总反推子用例实际执行，不冒称所有错误及事务故障已完成Go差分。

用户授权后已拉取并保留[官方PD nightly镜像](https://hub.docker.com/r/pingcap/pd)：
`sha256:50f456a40392f64acfbc6fa86fcd332297fb132d6384df93b926dddac08ff6a7`，
linux/amd64。隔离、只读、无网络容器中扫描无`*.test`、`pd-ut`或Go SDK，
PD程序为commit `89998850efa4effedb7542ee64c965d4e428fedc`，kvproto为
`9192d38c8b67`，与本任务固定`f8ad079c8e98`不符；没有采用其替换已准入oracle。
所有本任务容器已删除，镜像按用户要求保留。没有在Docker中构建或生成Go。


## 2026-11-05：隔离同源Go逐例运行，解除准入阻塞

当前源码原样只读，未复制/编辑Go源码或模块，也没有failpoint、make或生成/整理命令。
先尝试官方`golang:1.26.8-bookworm`linux/amd64镜像，源码/模块缓存只读挂载、独立缓存
及输出；因缓存缺少两个Linux专用依赖，setup退出1，0用例，不计行为失败。
转用已缓存Go 1.26.8 darwin/arm64 SDK，目标为darwin/amd64，CGO=0、without_dashboard；
系统沙箱禁止写仓库、模块缓存及SDK，网络在构建和四项直接用例阶段禁用。
构建中编译器临时中间产物仅在独立可写输出目录生成并自动清理，未新增任何仓库/缓存
输入Go源文件。源码目录touch负向探针返回Operation not permitted，退出1，文件未创建。

`go test -c -tags=without_dashboard -p=2 -o <isolated>/keyspace-darwin-amd64.test ./pkg/keyspace`
退出0；`GOCACHE`、`GOTMPDIR`使用独立目录，`GOMODCACHE`只读、`GOFLAGS=-mod=readonly`、
`GOTOOLCHAIN=local`、`GOPROXY=off`。Go源码/模块1101项首尾SHA及存在性完全一致。
通过`go tool test2json -t -p github.com/tikv/pd/pkg/keyspace <binary>`执行精确过滤，
`-test.count=1 -test.timeout=120s`；四个子例各有run/pass事件，退出0、4通过/0失败/0跳过：

- `TestMetaServiceGroupTestSuite/TestAssignToGroup`（三个enabled、初始0组，count=5）。
- `.../TestAssignToGroupRejectsNegativeCount`（count=-1，InvalidAssignmentCount）。
- `.../TestPickGroup`（同样三个组，仅所选组计数变1）。
- `.../TestGetAssignmentCountsInitialZero`（初始组全禁用，自动分配返回错误）。

Rust新增`mcs_027_replays_direct_go_cases`，使用相同三个ID和地址、初始状态、5/-1/1输入，
在真实etcd执行同样状态断言；并列选择只比较合法组及其计数，不伪造Go随机选择的具体ID。
最终`cargo test --locked -p astersql-pd-keyspace --lib mcs_027_ -- --nocapture`退出0，
5通过/0失败/0忽略；`cargo check --locked -p astersql-pd-keyspace`退出0。
格式化先于测试，原子动态槽位1，绝对CARGO_TARGET_DIR仍为上文路径；所有子进程退出后释放锁。

同时同样隔离构建既有`pkg/storage/kv`测试，运行`^TestEtcd$`退出0、1通过，真实embedded
etcd执行值/版本冲突、额外条件和已取消context的原断言，不需failpoint。
既有Rust callback事务/并发/故障/WAL与Go创建路径证据继续保留，不以MemoryStore
直接用例替代持久化证据。另跑全部元服务分组Go suite：16个独立子例通过，0失败，
健康检查用例使用实际etcd与网络，用于接续任务28，不提前认证其Rust实现。

本轮完整命令、环境、构建失败/成功日志、17个去重Go用例的JSONL、二进制和源码摘要、
首尾守卫以及Rust精确输出见[task-027-oracle-resume.json.gz](task-027-oracle-resume.json.gz)。
直接四例包含在16个suite子例中，不重复累计。旧证据包保持原样以免把历史缺口冒充通过。
