# HTTP、配置、日志与指标回归

逐行目录保存 HTTP、配置、指标和命令清单的 759 个原始行及其拥有者。
本轮结果及原始证据见任务回执。
[逐行目录](../../../../tests/rust-compat/fixtures/task-101-catalog.json)、
[HTTP 来源](../wire-inventory.md)、[配置、指标和命令来源](../surface-inventory.md)、
[任务 23 回执](task-23.md)。

| 当前回放 | 来源 ID | 核验内容 |
| --- | --- | --- |
| `domain-gc-http` | HTTP-161、162、197 | 旧服务安全点列表和删除、键空间 GC 状态读取、真实 etcd 记录与重启 |
| `domain-keyspace` | HTTP-179–185 | 创建、状态与配置、查找、分页、转发、事务失败和重启 |
| `domain-maintenance` | HTTP-186–189、M-037 | 成功、冲突和错误响应。活动 gauge 置 1，冲突保留原值，删除置 0 |
| `domain-meta-service-group` | HTTP-190–192 | 配置、状态和地址装饰，持久化错误与 WAL 重启 |
| `domain-mcs-discovery` | HTTP-193–195 | 成员与 primary 查询、真实 TSO 驱逐、旧流拒绝和 WAL |
| `domain-tso-directory` | HTTP-198–204 | 目录写入、分配、优先级与回读、共享 manager 和持久目录 |
| `domain-tso-transition` | HTTP-205–209 | 拆合与回滚、跨组发号、PATCH 路由边界、持久提交足迹 |
| `http-v2-boundary` | 拒绝边界 | 真实 v2 差分中的未知 MCS 服务。额外宿主检查验证无副作用、错误方法、PATCH 路由及未读请求体 |

旁路观察器保存实际抓到的 HELP、TYPE、标签、直方图桶和原始样本。
聚合校验按本轮真实到达的 shared family 计算。维护指标使用每个业务操作后的同步抓取，
因此删除归零和冲突不产生额外标签都能在原始报告中逐次核对。
GC 的宿主关闭和删除 store 的测试单独验证旧标签消失。事务指标在真实 etcd 暂停、
停止及 WAL 恢复后验证成功和失败计数。

只有已登记的业务 case 均退出 0，且报告集合与当前执行目录逐项一致时，才产出
`HTTP_REGRESSION_REPORTS` 路径索引。显式 ignored 指标复核读取该轮索引。
未登记业务入口或只保留后续依赖的来源行，沿用逐行目录的准入范围。
