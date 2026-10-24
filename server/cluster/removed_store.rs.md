# 移除节点的迟到心跳

[removed_store.rs](removed_store.rs) 实现已有store报告的检查/提交边界，
由[handle_store_heartbeat](store_heartbeat.rs)调用，保持其公开签名和未知store错误。
常规Removed报告成功且不触碰raw stats、lastHeartbeat、slow状态和热点入队；
仍执行真实StoresStats健康清理。完全未知store仍返回 `store N not found`。

预检查只读取快照。提交通过core的StoresInfo::patch_store在现有注册表写锁内
重新读取最新元数据；已Removed跳过，已删除的条目由patch_store跳过，不能重新注册。
只在通过第二次检查后更新共享raw holder和lastHeartbeat，保留并发更新的其他元数据。
观察rolling/slow和非阻塞Read/Cold任务入队也在该锁内，避免检查后移除再发布的窗口。
健康清理需要重新读取注册表，因此在锁外执行，避免重入死锁。
不改变HotCache算法；早于移除已接纳任务的处理、旧热点清除属于统计生命周期。

私有handle_checked的before_commit闭包只提供确定的检查后时序接缝，生产调用为空闭包。
[独立测试](removed_store_test.rs)用真实core/HotStat和双Barrier在另一个线程执行移除，
验证tombstone、完整删除、共享raw holder保护和并发metadata保留；所有工作线程join，
HotStat显式shutdown取消并join，FIFO查询屏障确认未发布新热点。

领域函数不接受存储后端或持久化回调，不会重新持久化任何store。
该事实不扩展为外部宿主持久化的并发保证；宿主不得在此边界外保存迟到快照。
Go源码已明确记录SaveStoreMeta残余竞争，本任务不伪称Go已修复该窗口。
运行oracle、红绿验证和范围说明见[任务42证据](../../docs/rust/state/task-42.md)。
