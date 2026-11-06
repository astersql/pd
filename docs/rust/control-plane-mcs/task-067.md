# 任务67：元存储事件流

2026-11-06，基准 `9ceedb1beb0e1f3be359f7a7d3f9eb5186ce3850`。
本项行为已完成；[共享原始回执](../server-api/acceptance/task-16-metastorage.json.gz)
保存确切argv/退出码、所有red/green日志、原始RPC/状态、输入/源码/二进制摘要及
Go/模块/plan.md守卫。[阶段汇总](../server-api/acceptance/task-16.md)区分
本项通过与额外完整server包的8项未通过；后者交验收22/23/24，不计通过。
同源Go oracle、kvproto ZIP、protoc和官方etcd/etcdctl五项摘要已重算匹配。
没有Go构建/测试、源码/模块改写、make或failpoint注入；现成Go仅运行服务及只读build-info检查。

MetaStorage.Watch直接拉取canonical MetaWatch，不创建转发worker；
丢弃响应即释放后端流，宿主取消返回正常EOF。范围watch在领域层定义：
精确键、闭开范围、NUL from-key、inclusive revision和非正revision从当前开始。
内存模型与真实etcd共同验证，etcd使用原生范围选择。
Entry增加真实version（内存更新递增、删除后从1开始，etcd读取原字段），
protobuf只存在于服务适配。prevKV只保留Go返回的key/value。

8个真实流请求覆盖点watch创建/更新/删除、历史重放、bounded/from-key、
compaction错误响应后Unknown终态、空键deadline、取消后重连和WAL重启重放。
每侧12条原始回执（不是12个独立测试），并有辅助Get验证删除/重建/重启状态。
压缩revision和消息逐侧对照实际提交/压缩边界，未归一化掉差异；
Go与Rust两侧均重启真实WAL，Rust重启外部官方etcd，仅证明过渡后端。

除最初Unimplemented red外，历史header扩展得到Go响应header在独立提交revision，
Rust错误使用最后事件游标（差1）的真实red；新增MetaWatch.header_revision保留
原始后端header，重放游标继续使用最后事件+1。progress只推进内部游标，不输出空消息。
空键最初误期望错误，Go实际保持pending；记录该oracle探索超时，不计产品red。
随后明确300ms deadline得到真实差异：Go等待至Cancelled/Timeout expired，
Rust提前返回错误帧。修复为在首次事件/错误前不返回HTTP/2 headers，
空键保持pending至取消；最终同一断言通过，无放宽状态码或删除原始差异。

mcs_067_单元、两后端范围/version/header契约通过；三项双实现最终全部通过。
宿主关闭EOF及共享runtime资源由68的真实网络测试进一步验证。
