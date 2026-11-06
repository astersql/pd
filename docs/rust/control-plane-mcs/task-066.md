# 任务66：元存储写入删除

2026-11-06，基准 `9ceedb1beb0e1f3be359f7a7d3f9eb5186ce3850`。
本项行为已完成；[共享原始回执](../server-api/acceptance/task-16-metastorage.json.gz)
保存确切argv/退出码、所有red/green日志、原始RPC/状态、输入/源码/二进制摘要及
Go/模块/plan.md守卫。[阶段汇总](../server-api/acceptance/task-16.md)区分
本项通过与额外完整server包的8项未通过；后者交验收22/23/24，不计通过。
同源Go oracle、kvproto ZIP、protoc和官方etcd/etcdctl五项摘要已重算匹配。
没有Go构建/测试、源码/模块改写、make或failpoint注入；现成Go仅运行服务及只读build-info检查。

复用canonical MetaStore的原子有序事务，在同一提交中先读前值再Put/Delete，
不会用两次独立操作拼接prevKV。Go源码只返回前值key/value，元数据保持0；
Delete不转交range_end、响应deleted为默认0，均保留并通过真实Go验证。

11个双实现配对包含新建/覆盖有无prevKV、lease绑定/撤销、缺失及负lease、
空key、删除范围字段被忽略、缺失键和无prevKV删除。
回读真实后端验证值、mod_revision及lease；失败不产生数据，lease撤销后数据消失，
删除a保留邻键b。原始RPC及各Put后的独立物理状态保存在共享回执。

初始真实red：Go完成写删，Rust Put为Unimplemented，0通过/1失败、exit101。
实现后双实现中的本项通过，11组配对；mcs_066_单元覆盖原子前值、
无效lease不覆盖旧值、删除及取消后的准入。包级4测试及相邻存储/宿主回归通过。
没有无条件重试失败写入，未把外部etcd WAL扩大为原生后端耐久性认证。
