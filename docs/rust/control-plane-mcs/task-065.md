# 任务65：元存储范围读取

2026-11-06，基准 `9ceedb1beb0e1f3be359f7a7d3f9eb5186ce3850`。
本项行为已完成；[共享原始回执](../server-api/acceptance/task-16-metastorage.json.gz)
保存确切argv/退出码、所有red/green日志、原始RPC/状态、输入/源码/二进制摘要及
Go/模块/plan.md守卫。[阶段汇总](../server-api/acceptance/task-16.md)区分
本项通过与额外完整server包的8项未通过；后者交验收22/23/24，不计通过。
同源Go oracle、kvproto ZIP、protoc和官方etcd/etcdctl五项摘要已重算匹配。
没有Go构建/测试、源码/模块改写、make或failpoint注入；现成Go仅运行服务及只读build-info检查。

canonical `astersql-pd-mcs-metastorage-server` 实现MetaStorage.Get，消费宿主注入的
同一MetaStore及每次请求的serving状态；服务适配不创建存储、运行时或领域副本。
精确键、闭开范围、NUL from-key、limit/count/more及历史revision均映射领域契约。
新增后端无关read_counted，默认一致快照模型及真实etcd原生count通过同一契约；
旧read路径复用同一实现，未以全量读取实现生产etcd计数。

Get只返回key/value，其余KV元数据保持Go的0值；请求header不校验cluster id。
非serving返回Unavailable/not leader，存储错误在成功RPC的UNKNOWN header内返回。
14个真实配对包含点/缺失/范围/limit/负limit、历史/负revision、from-key、逆向范围、
空key、future/compacted、忽略请求cluster header及真实第二节点拒绝。
响应cluster id由实际PD members验证，revision由独立后端前后快照约束；
历史值及总count独立断言，未把不相等的revision/字段直接删掉。

初始真实red：Go返回alpha，Rust未注册而Unimplemented，0通过/1失败/0忽略、exit101。
最终双实现3测试中的本项通过，14组配对；单元mcs_065_通过。
包级4通过，storage36、election19、runtime106通过；workspace check/metadata/fmt check exit0。
完整workspace test按验收24集中运行，双部署/TLS组合由148继续验收。
