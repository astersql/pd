# 操作类别与状态图

`status.rs` 是 `astersql-pd-schedule-operator` 的领域实现。`OpKind` 保存Go的
12个bit及低bit优先显示顺序，支持`FromStr`、`Display`、`BitOr`和`BitOrAssign`。
解析逐个逗号分隔名称，不trim、不改变大小写；重复名称按位合并。`witness`
存在于显示表而不在解析表，保留Go的这一不对称契约。错误为`UnknownKind`，
保留首个非法名称和稳定错误文本。未知bit不参与显示，全未知时显示`unknown`。

`OpStatus`保留u32原始表示；未知值可表示、可显示`Unknown`，不能进入tracker。
`OpStatusTracker`通过同一个Mutex原子地更新状态与到达时间：

- Created → Started、Canceled、Expired。
- Started → Success、Canceled、Replaced、Timeout。
- 所有终态均无后继，拒绝相同状态、未知状态和其它非法边；拒绝时不更新时间。

tracker保存created、started及唯一终态的三个时间槽。`reach_time_of`以`None`
表示Go零时间；查询其它终态不会返回当前终态时间。`Clock`是唯一受控边界，默认
`Instant::now()`，宿主可注入单调时钟；没有runtime、后台任务或全局状态。
时间是进程内单调时间，供elapsed判断；墙上时间/序列化与protobuf映射由后续适配层
负责。`check_expired`、`check_timeout`提供该图的基本定时转换，等于阈值即转换；
只在对应起始状态检查，重复查询已到对应终态返回true且不更新时间。
Rust使用非负`Duration`；若适配Go有符号时长，非正值应转为零，其立即到期效果相同。

`AdditionalInfo`是为后续Operator组合的唯一元数据容器，不创建第二份Operator。
零值可用，读写受RwLock保护，读取返回拥有所有权的副本。日志为空映射时返回空串，
否则按key排序并保留Go encoding/json对HTML和U+2028/U+2029的转义。
`related_merge_region`只接受非空ASCII十进制u64；不存在、非法、溢出返回0；
`has_related_merge_region`只判断非空，因此非法值及字符串`0`仍可返回true。
库不安装日志订阅器；无MetaStore或外部I/O操作。

## 证据与限制

依据只读`kind.go`、`status.go`、`status_tracker.go`及相邻测试；来源commit为
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`，文件hash记录于
[fixture](../../../docs/rust/schedule-execution/fixtures/task_015.json)。
用户明确豁免必需Go运行oracle/运行差分；fixture标记`source_assertion`及
`go_execution=false`，不是Go运行输出。未复用此前排除的运行产物。

[task_015_test.rs](task_015_test.rs)显式注册4项Rust测试：125个源码契约表驱动
案例，另外验证时限精确边界、拒绝转换后的时间、并发唯一终态、32线程元数据
读写及JSON转义。错误实现变异和workspace回归见
[任务证据](../../../docs/rust/schedule-execution/task-015.md)。
这证明本任务领域行为通过Rust实测，不声称完整Operator/步骤/Controller已经迁移，
也不声称Go/Rust运行差分或wire适配已经验收。
