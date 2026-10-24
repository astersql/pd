# 任务35：全量同步追赶与溢出

依赖[任务34](task-34.md)通过，起点 `593a2c9066c0d2278c0f6222a9576a7278ef27cf`。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，server.go/server_test.go无漂移。
已读取两份只读计划、根规则、Copilot、ADR、移植指南与依赖基线，无更深AGENTS。

## 实施前只读 oracle

[入口](task35/oracle.sh)、[输出](task35/oracle.out)，退出0，三个既有测试通过。
显式编译已有server/client/history/metrics与server/history测试，排除client_test的
failpoint TestMain。CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off，
Linux二进制由Alpine执行。所选路径无RunServer、failpoint.Enable或取消全量迭代；
Sync失败先unbind直接退出，不启动下游发送循环。没有生成、注入或写入Go文件。

运行断言契约（不是额外打印的字段快照）：
- KeepsLiveRecordsAppendedDuringCatchUp：reset10，快照ID1；历史ID100/101，
  追赶第二次发送阻塞时追加ID102。完成start12/empty，sendIndex12，history next13，
  recordsFrom12为ID102；后续sendDownstream发送(start12,ID102)，sendIndex13。
- FailsWhenCatchUpHistoryExceedsMax：最大容量1、reset100，快照ID1发送阻塞期间
  写ID2/3；只收到(start0,ID1)，全量返回ResourceExhausted。
- FullSyncDisconnectsWhenHistoryBufferOverflowsDuringCatchUp：同上经Sync入口请求1，
  返回ResourceExhausted且GetAllDownstreamNames为空，未发送完成帧。

源码审查补充：retainedRecordsFrom捕获完整[start,next)后发送，新增记录不修改该上界；
空region快照追赶帧从位置0开始、completion携带真实next。retain覆盖发送全程。
领域错误保留HistoryGap，协议适配映射ResourceExhausted；断开通过宿主回调落实，
不在本任务引入protobuf、注册表、私有runtime或MetaStore revision。

## TDD 与实现

[失败输出](task35/red.out)：最小可编译实现实际运行5项，退出101，1通过/4失败。
断言包含：遗漏追赶ID100/101、completion10而非12、窗口缺失未返回错误、
发送失败未调用disconnect、空快照仅发送42而非位置0/1000及completion1043。

新增 `full_catchup.rs`、独立 `full_catchup_test.rs`、相邻行为文档；lib显式注册并
导出 `CatchUpFrame` 与 `FullCatchUpError`。`FullSnapshot::catch_up` 用真实
HistoryBuffer、BasicCluster与RegionInfo；锁内固定完整历史，锁外发送。
retain覆盖全部帧与断开回调，成功后返回后续live drain起点。使用原有plan_replay
验证ID102及Arc身份、最终history13与索引12→13；溢出保留真实cluster的ID1/2/3，
断开回调一次、零完成帧；追赶首帧、后续帧、completion错误均释放retain并缩回base。
空快照1001条追赶覆盖1000上限、位置索引与真实历史completion的区别。
未新增依赖、feature或修改Cargo配置；无生产后台任务，无新增存储边界。

## 验证

所有命令经[槽位入口](task35/slot.sh)执行，oracle/红灯/绿灯阶段实际均领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[验证入口](task35/validate.py)、[总输出](task35/validation.out)、[逐命令统计](task35/summary.json)。
执行顺序：cargo fmt --all，聚焦5项、包级回归、metadata、fmt check、workspace check、
workspace test；确切命令、退出码、有效测试数见统计。Go构建子进程继承readonly/off设置；
etcd夹具只在槽位写二进制、数据目录，EOF停止后wait，proto构建仅写OUT_DIR，
已审查scripts/rust-etcd-fixture与pkg/proto/build.rs，无Go生成/注入/模块整理。
本任务无MetaStore读写；包级持久化测试覆盖真实MemoryStore与EtcdMetaStore。

最终全部命令退出0：聚焦5通过、包级43通过（含真实Etcd持久化回归），
workspace 1182通过、0失败、1既有ignored：tso_processes需要独立进程二进制，
与任务34基线一致；本任务5项无忽略。metadata/fmt/check退出0。
[资源核对](task35/resources.out)：所有子进程结束，自己的slot-1.lock已释放，缓存保留。
[只读核对](task35/readonly.out)：1101个Go/模块文件清单、SHA256及Git状态与起点相同，
所有plan.md哈希不变，Markdown链接与git diff --check通过。

状态：已完成。
