# 任务38：客户端全量就绪语义

起点 `371dac696c9c62d8e026729bae8edb30d1cac808`；依赖任务37的响应应用已具备并有完整
必要行为证据。其默认x86进程回收仍待系统解除U状态，本任务不删除槽位1锁、不使用其产物。
本任务复用已经通过ARM64验证的领域依赖；不需要该挂起进程才能执行验证。

## 实施前 Go oracle

基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；当前 client.go、server_test.go
相对基准无差异。只读审查 client.go:97–186、StartSyncWithLeader收流循环、
server_test.go:1181–1332，及其 newTestRegionSyncer/内存存储/LevelDB夹具调用链。
三项指定测试不需要failpoint；TestMain仅goleak及Etcd连接等待。
复用[任务37构建入口](task37/oracle.sh)生成的既有Linux ARM64调试测试二进制，
SHA256 `1b5427ae96908f6a42a5e4ba3946e7054a807e3370f86edbf007645a30939d2e`。
没有新增、改写或注入任何Go文件，不运行Go make入口。

实现前[实际运行输出](task38/oracle.out)：指定三项全部PASS、退出0。
命令在仓库根通过[槽位入口](task38/slot.sh)执行：

```sh
bash docs/rust/state/task38/slot.sh bash -c 'docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/syncer-oracle37-arm64.test -test.run "^Test(ClientWaitsForFullSyncCompletionBeforeRunning|ClientFinishesFullSyncWithOldLeaderKeepAlive|ClientWaitsForEmptySnapshotCatchUpCompletion)$" -test.v'
```

实际槽位2，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`；
容器退出后释放锁、保留缓存。该命令复用现有二进制；重新构建可用任务37入口并设GOARCH=arm64。

| Go测试 | 输入顺序 (startIndex, regions数量) | 每帧(fullSyncing,running,historyNext) |
| --- | --- | --- |
| TestClientWaitsForFullSyncCompletionBeforeRunning | (0,1),(1,0) | (true,false,0),(false,true,1) |
| TestClientFinishesFullSyncWithOldLeaderKeepAlive | 初始42；(0,1),(42,1),(43,0) | (true,false,0),(true,false,0),(false,true,43) |
| TestClientWaitsForEmptySnapshotCatchUpCompletion | 初始42；(0,1000),(1000,1),(1043,0) | (true,false,0),(true,false,0),(false,true,1043) |

表中第一、二项所有字段均由原Go测试实际断言；第三项中间historyNext=0是源码契约，
原测试只断言其中间fullSyncing/running，末尾historyNext=1043有运行断言。
区域ID/范围使用既有newTestSyncRegionWithRange，数量单位为区域、索引为history sequence；
全量中间StartIndex为位置，不是MetaStore revision。无时间归一化或wall-clock等待。
错误响应handled=false、running=false且保持fullSyncing/history；重连在新stream入口
清fullSyncing，收流错误清running。后者为源码推导扩展，不冒充额外Go运行差分。

## 验证策略

任务37已实现就绪语义；先增加指定前缀行为测试检查真实ClientApply、BasicCluster、
HistoryBuffer与Storage，再将已有判断提取至client_ready.rs保持单一实现。
若测试已通过，记录复用证据，不为红灯破坏既有正确行为（计划开发策略允许）。

## 实现与验证

`client_ready.rs::ClientReady` 私有封装两个标志，`classify`生成帧分类、`complete`仅在
应用成功后提交迁移、`reject`保持全量阶段但清running；`reset_stream`清两个标志。
`ClientApply`委托该组件，不引入第二套区域/历史实现；公共API、Cargo依赖及锁文件不变。
生产版权与相邻rs.md、独立测试注册均已完成。

[提取前验证](task38/before-refactor.out)：fmt退出0，指定测试6通过/0失败/0忽略，
退出0；已有实现无真实行为缺口，按计划“已有实现通过则复用，不人为破坏”规则，
不制造断言红灯、不声称有失败证据。新增测试覆盖三项指定Go序列，及无追赶空快照、
错误/重连/增量索引归零等源码推导扩展。真实FullSnapshot/catch_up生成1001区域帧，
逐帧检查running/full_syncing/history，末尾检查所有区域的缓存和存储状态。
Etcd测试执行上述三项序列；内存扩展用例另检查错误帧不落盘。

完整验证入口为 `bash docs/rust/state/task38/slot.sh python3 docs/rust/state/task38/validate.py`。
[摘要](task38/summary.json)记录确切命令、退出码和通过数量，逐项输出为
[聚焦测试](task38/validation-1.out)、[包级测试](task38/validation-2.out)、
[metadata](task38/validation-3.out)、[workspace check](task38/validation-5.out)、
[workspace test](task38/validation-6.out)；[总入口输出](task38/validation.out)。
所有Cargo命令从根执行，使用默认features、--locked测试，先cargo fmt --all。
夹具及协议生成仅复用既有Go源码；GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、
GOARCH=arm64，Go子进程不注入failpoint。proto/build.rs只读固定模块ZIP并在OUT_DIR生成；
Etcd夹具go build只将二进制和数据写入领取的槽位，EOF关闭服务器并等待进程退出。
本模块没有新后台任务、运行时或监听器。

最终结果：fmt、metadata、fmt check、workspace check全部退出0；聚焦6通过、包级69通过，
workspace共1208通过/0失败/1既有ignored（tso_processes需独立进程二进制），退出0。
本任务聚焦与包级无忽略，MemoryStore与EtcdMetaStore均通过。

所有阶段实际动态领取槽位2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[资源退出核验](task38/resources.json)：本任务slot进程均已退出、锁已释放、缓存保留。
原有槽位1锁和不可中断进程未操作，不影响本任务全部ARM64验证。

[只读核验](task38/readonly.json)：1101个Go/模块文件清单、内容和Git状态均相同；
前后聚合SHA256均为`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份指定只读plan.md哈希相同；无新Go文件、无模块/锁文件变化。用户已有计划变更保留。

状态：已完成。本任务仅交付客户端全量就绪状态，不表示启动生命周期或整个主题迁移完成。
