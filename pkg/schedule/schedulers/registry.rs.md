# 内置工厂注册

`SchedulerRegistry` 从 base.rs 的空表迁入此 canonical 模块；默认仍为空，宿主显式调用
`register_builtins`。完整注册幂等；单类型重复返回 Duplicate，替代 Go 的进程退出。
局部表与全量注册冲突时先报错，保持原表；未知类型和 Go plugin 不可注册。

`FactoryContext` 仅提供真实 StorageEndpoint、ClusterPaths、显式时间及热点能力/排名输入。
`create_args` 按只读 init.go 接线全部18类；`create_json` 恢复各策略配置并校验。
范围散射CLI是start/end/name，现有模块构造接口是name/start/end，由注册层转换；
shuffle-hot只在恰有一个参数时解析limit；不使用参数的Go decoder继续忽略额外参数。
历史非空hot配置先以byte/key兼容优先级及空rank版本为基线，再覆盖持久字段；空对象
使用当前默认值。leader/witness/evict的零batch回填各自默认值。传输witness工厂与Go
一致完全忽略decoder，其默认配置编码为null。未知JSON字段沿用各canonical serde规则。

`RegisteredScheduler` 的公开 `BuiltinScheduler` 枚举装有真实策略，无Base或空策略。
宿主匹配枚举并传入各策略已有typed context执行调度；此处不制造统一空schedule。
slow-store变体同时拥有disk与network两个canonical策略，保存合并网络字段与disk快照；
沿用前序任务的宿主串行协调契约：宿主负责两者策略更新与存储事务的协调，而非同时
调用其各自持久化写入。工厂保留两者完整初始配置，未新增后台任务或运行时。
hot变体保留原始配置与有效能力投影后的solver，配置更新应重新创建以同步solver。
range变体装配105真实策略与104持久队列，恢复保留job ID/status/create。

名称来自Go类型，散射附加range-name；hot为balance-hot-region-scheduler。查找选择
最长匹配，支持scatter-range历史别名。save_config先编码再调用真实MetaStore endpoint；
restore按名称定位类型、读取并重新校验配置，缺失/损坏/读取失败返回错误。默认策略的
disabled属于owner快照，由set_disabled加save_config显式持久化；查询本地快照不触发I/O，
需要刷新时由宿主restore。range与Go一样编码为jobs数组，不在数组中写disabled。
HTTP、全局注册、运行时、操作提交及热输入采样都属于宿主适配层。

来源证据为只读Go源码推导，非Go运行差分。冻结fixture、红灯、真实策略操作、
Storage I/O故障及全部命令见 [task-106](../../../docs/rust/schedule-execution/task-106.md)。
