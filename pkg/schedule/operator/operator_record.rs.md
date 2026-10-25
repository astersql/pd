# Operator 取消、历史与输出

`operator_record.rs` 扩展唯一的 `Operator`，复用状态图和真实 `OpStep`。
`CancelReason` 保存14个来源字符串；`cancel_with_reasons`只采用第一个原因，并以
`AdditionalInfo::set_if_absent`原子保存。即使终态拒绝取消，首次原因也会保存；先前
原因（包括空字符串）不被覆盖。无参数 `cancel` 与前置 API 兼容。`replace` 仍只
允许 STARTED → REPLACED，所有终态转换只能成功一次。Operator 源码没有完成回调；
这里不增加回调协议，FinishedCounters 的消费者属于 Controller。

`sync(&mut self, other)` 只复制 other 的 timeout，同时互写 region id 作为关联 merge
信息。独占借用保证不会在运行时无同步改写 timeout。scheduler kind 优先 admin、
split-scatter，然后取最低有效 bit；未知 bit 也按原始位图处理。

`history_at` 先按步骤顺序生成 leader moves，再将 AddPeer/AddLearner 与 RemovePeer
按各自出现顺序配对生成 region moves。数量不匹配时丢弃未配对项；不依赖成功状态或
current step。`history` 提供系统单调时间，确定性调用者可传入同一观察时间。

`record` 固定 finish_time 和 duration，保留对原 Operator 的借用。成功记录从 STARTED
计算；非成功且至少完成一步时，从上一步首次完成时间计算；时长保留符号并按 Go
有符号纳秒范围饱和。未启动的零时间保留为 None，宿主墙上时间适配器须调用
`with_zero_start_duration` 给出 Go `finish.Sub(time.Time{})` 的饱和值。未解析时
`render/json_string` 返回类型化 `UnreachedStart`，不会填造零时长。

库内使用 Instant，不安装全局墙上时间或时区。`render` 接收宿主时间格式化函数，
Some 为观察到的时刻，None 为 Go 零时间；测试仅将时间文本归一化成 TIME/ZERO，
其他字段精确核对。`OpStep::Display` 委托既有步骤，输出顺序及后缀保持来源行为。
`json_string` 保留 Go MarshalJSON 直接用双引号包围文本的行为（包含引号时会是
非法 JSON）；没有暗中修复此来源边界。需要可靠 API JSON 时用 `to_json_object`，
kind/status 为数值，nil epoch 为 null，epoch 零字段省略，timeout 为 Go duration
字符串。与来源一样，输出前进行 success/timeout 检查，可能触发状态更新。

本层没有存储、网络、私有运行时或新依赖。Go 仅只读源码契约推导，没有 Go 运行
或跨语言运行差分。测试、固定 fixture、变异红测及验证见
[task-025](../../../docs/rust/schedule-execution/task-025.md)。
