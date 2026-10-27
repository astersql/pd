# 慢趋势驱逐配置 HTTP 适配

`serve` 使用宿主独占借用的 canonical `SlowTrendScheduler`、`StorageEndpoint` 和
`ClusterPaths`，处理相对路径 POST `/config`、GET `/list`。不安装完整 server/api
路由；宿主负责挂载、同步和线上 HEAD body 抑制。领域实现复用
[slow_trend_recover.rs](../schedule/schedulers/slow_trend_recover.rs)，没有复制配置状态。

只读 Go `evictSlowTrendHandler.updateConfig` 将 JSON 解码为 map，要求精确小写
`recovery-duration` 数值；缺失、null、其它类型为400 JSON字符串
`invalid argument for 'recovery-duration'`。未知字段（包括驱逐列表）忽略，重复字段取最后值。
数字先经过 float64，再转 uint64，保留小数截断和2^53附近精度损失。
负数采用相邻适配器的有符号转换再转 uint64 约定；Go 浮点到 uint64 的负数及
超界结果有平台相关性，不宣称其跨平台运行一致性，也没有新增范围校验。
固定语法/顶层类型错误为400/input，请求读取失败为500/internal，部分成功读取的
JSON不能被应用。错误栈仅检查结构及非空，不将Rust栈伪称为Go栈。
任意损坏JSON的全部诊断、超出float64范围的数字以及Gorilla路径清理/重定向
不在固定案例的一致性声明中。

`listConfig` 调用 Go `clone`，只复制RecoverySec，故 `evict-by-trend-stores` 固定为
`null`，即使真实驱逐集合非空；保留字段顺序、缩进和无末尾换行。未知路径404明文，
错误方法405空body且无Allow。列表和拒绝路径都不读body、不访问存储。

成功更新调用已有 `update_recovery`，保存完整领域配置后发布新值，响应200和
`"Config updated."`。与Go锁内修改再失败回滚的可观察状态一致，写失败返回500 JSON
错误字符串。候选、捕获时钟及驱逐列表保持；成功保存能重建配置，失败保存保留旧值。
恢复阈值真正影响下一轮调度；共享构建器仍生成batch=3的Urgent多目标领导者操作。

按用户授权采用只读Go源码/测试推导及真实Rust行为、故障验证；没有Go运行差分。
[任务153证据](../../docs/rust/schedule-execution/task-153.md)保存provenance、fixture、
红绿测试和workspace检查，Go及模块文件保持只读。
