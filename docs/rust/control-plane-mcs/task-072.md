# 任务072：装载分组时间戳分配器

2026-11-06，基准`213abc35e`。状态：已完成。
实现与原始回执见[任务76](task-076.md)及[验收10](../server-api/acceptance/task-10.md)。

GroupAllocatorWatch在canonical TSO crate装载本地分组视图和真实GroupAllocator，复用原Oracle，
不创建第二个分配算法。按成员地址等价判断副本归属；空/缺失/删除默认组恢复本地默认副本，
撤出分组停止并join原工作者。服务视图排序键空间，修复默认组保留空间归属，持久元数据保持原状。
保留空间修复先取得0通过/1失败的真实red，随后`mcs_072_`2通过、0失败/忽略。
回放Go TestWatchAndDynamicallyApplyChanges的11步相同输入，最终assigned=[0,1,4]、
loaded=[0,1,3,4,5]；真实发号、分组独立持久窗口及删除lease均验证。
TestDefaultMembershipRestriction的相同classic输入另验证本地默认/非默认组的保留空间修复及默认副本恢复。

Go目录/watch四个叶例及保留空间叶例实际通过；默认features，不把NextGen或后续交接任务计入本项。
TSO包级17项、相邻classic TSO、宿主、架构和workspace check均通过；完整回归集中验收24。
最终动态槽位1及早期槽位2、绝对target路径、全部命令/退出码及1101项Go只读守卫见共同回执。
