# 同步历史环缓冲

`HistoryIndex` 是 SyncRegions 的独立序号；`next_index` 是下一条待分配编号，
不是最后一条编号，也不是 MetaStore revision。`first_index = next_index - len`。
对照同目录 `history_buffer.go` 的 normalize/record/get/recordsBetween/reset；
运行差分及边界验证见 [task-30](../../docs/rust/state/task-30.md)。

容量是 unit 乘 2 的幂，零 unit 默认10000，零 size 也至少一个 unit。
Rust接受非负容量，归一化乘法溢出和分配失败返回类型化错误；不模仿Go无限溢出循环。
记录使用真实 core.RegionInfo 的 Arc；VecDeque 实现有界逻辑容量，已满时先弹出最旧项。
不需要Go用于区分空满的额外物理槽位，公开容量和顺序保持一致。
索引达到u64::MAX时拒绝追加且不改动历史，避免Go无符号绕回破坏区间不变量。

读取区间是半开区间，end超过next会裁剪；start过旧、start不小于end或未来区间为空。
get只接受[first,next)。返回Vec中的Arc保持原区域值的所有权，覆盖/重置不影响已取快照；
无外部快照时覆盖和重置立即释放旧区域引用。reset清空全部记录并设置任意新next。
独占可变借用保证每次变更完整；并发宿主负责选择锁，内部不安装全局状态或runtime。

本环只提供纯内存行为；保留窗口及增长/收缩见 [history_window](history_window.rs.md)。
with_config支持base/max/unit；原有构造函数保持固定容量。reset回到base并隔离旧guard。
索引持久化复用 history_persist，流接线复用 downstream/lifecycle。
本文件不创建 protobuf、watch 或后台任务。

Optional host-supplied `SyncMetrics` caches the existing Go gauge handles. A
successful record publishes the assigned (pre-increment) `sync_index`; reset or
reload leaves that gauge unchanged. No registration or label lookup runs here.
