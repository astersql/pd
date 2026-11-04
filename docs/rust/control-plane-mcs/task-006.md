# 任务006：异步确认与超时

源码边界：`tickUpdateState`的async_wait分支、`UpdateStoreDRStatus`、
`drCheckStoreStateUpdated`、`drSwitchToAsyncWithLock`及`IsRegionSplitPaused`；
真实调用来自server/grpc_service.go的StoreHeartbeat。

Go并无async_wait超时强制切换：只有全部当前存活primary报告相同state和state_id
才进入async。等待多久都不能替代确认。WaitStoreTimeout影响存活集合，
WaitRecoverTimeout是后续async恢复的等待条件，不能当成确认超时。
存活集合变化优先生成新async_wait/ID，canSync恢复优先回sync。
报告采用最后写入值（包括陈旧报告），重复报告不直接分配ID。

## 当前实现

公开领域输入`StoreDRStatus`不含protobuf；协议转换留给服务适配层。
所有边沿复用任务005的同一个`store_quorum`，没有第二套store或placement缓存。
报告存入同一manager，发布状态仍在原有锁内；全体确认、canSync恢复与可用集合变化
按Go优先级处理。成功分配ID并持久化后才发布；async状态记录宿主时钟。

## Go/Rust差分

固定版本Go通过真实StoreHeartbeat RPC依次收到：100陈旧ID1、200错误sync状态、
300和400正确async_wait/2（300重复）。经历多个500ms tick仍保持async_wait/2。
100、200补齐正确报告后，真实etcd记录async/3及async_start；Rust同样得到async/3，
available_stores仍为[100,200,300,400]。只归一化宿主墙钟async_start，先检查可解析，
不归一化state ID、可用列表和revision。Go进程先正常停止，再检查Rust业务断言。

首次最小可编译接口退出101：Go确认完成且已持久化async/3，Rust的tick返回false，
断言“all four real acknowledgments must publish async like Go”失败。
修复后同前缀4项聚焦通过、0失败、0忽略。
另覆盖最后报告回退、重复tick、不以等待时长代替确认、可用集合变化生成新ID、
旧ID报告不能确认新集合、canSync恢复优先回sync，以及split暂停的政策条件。
真实etcd TCP中断目标写入，确认失败后仍保留async_wait/2；恢复重试得到async/4，
ID3只消耗未发布，持久记录与snapshot一致；取消后不再切换。

相邻任务005的4项回归通过，工作区check及fmt检查退出0。复核夹具时进一步
统一Go和Rust的WaitStoreTimeout为1h（此前虽分类相同但阈值不同），同步打开
pause-region-split，并比较Go真实StoreHeartbeat响应的暂停标志与Rust领域判断。
async_start须落在最终确认请求/切换的实际墙钟窗口内，验证后才归一化该时间戳。

最终同配置及时间窗口补强后，`cargo test --locked -p astersql-pd-replication --lib
mcs_006_behavior -- --nocapture`再次1通过、0失败、0忽略，fmt检查退出0。
[task-006-evidence.json.gz](task-006-evidence.json.gz)保留首次有效行为失败、
4项聚焦、4项相邻、workspace check、最终真实差分与各次确切命令及原始日志。
所有阶段动态槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
Go/模块1101项首尾SHA、存在性与Git状态一致；自有进程结束后释放锁并保留缓存。
用户授权全部实现后统一运行完整包/workspace测试，本项保留待回归。
