# 任务131：接通键空间领域服务

2026-11-06，基准 `59e50389c`。状态：已完成。本轮实现与验收09一起验证；完整命令、源码摘要、原始失败/通过日志和物理状态保存于
[验收09回执](../server-api/acceptance/task-09.json.gz)，入口和结果见[验收09](../server-api/acceptance/task-09.md)。

只读来源：`server/server.go`的键空间分配器/manager构造与bootstrap，
`server/keyspace_service.go`的LoadKeyspace、LoadKeyspaceByID、GetAllKeyspaces、UpdateKeyspaceState及WatchKeyspaces，
`server/apiv2/handlers/keyspace.go`的七条注册及其调用。
既有36–41领域证据和当前公开API已核对；没有重新实现创建、配置、状态迁移和回滚算法。

宿主每个任期拥有一个KeyspaceManager、键空间专用namespace/step100分配器、配置及分割资源。
GC接收同一个Arc；HTTP与Keyspace gRPC经RoleServices获取同一个任期manager。
classic保留Go的nil分组manager，API模式通过显式builder选项注入唯一分组manager；CLI只在宿主边界解析既有PD_SERVICE_MODE=api约定。
这不认证其余MCS/API-mode服务。MetaStore和scheduler的RegionsInfo、labeler均复用原宿主资源。
KeyspaceLabeler把规则写入/删除交给现有scheduler队列，分割等待和失败清理由canonical keyspace领域完成。
启动恢复只恢复领域资源/分组索引，期间续租并响应取消；失败清理cluster/registry。
首次实际Bootstrap成功后才初始化保留空间并由任期tracker驱动配置中的预分配；普通重启保持原标签bytes/revision，领域bootstrap错误按Go记录后不覆盖已成功的集群bootstrap。

HTTP创建交给任期TaskTracker持有。调用方断开后取消token仍驱动领域完成标签和元数据回滚，
retire关闭tracker并等待工作退出。真实断连先证实原路径遗留DISABLED元数据，再证实修复后清理完成。
WatchKeyspaces通过领域KeyspaceWatch复用现有ReconciledWatch和存储codec；transport只转换protobuf。
LookupKeyspace仍按Go未实现方法返回Unimplemented，不将它计入兼容能力。

验证：`mcs_131_`真实双节点任期测试1通过；同一任期Arc稳定、follower无manager、旧manager读写受fencing拒绝，
接任节点恢复同一etcd元数据。验收09的classic/API双进程矩阵验证HTTP创建→gRPC按ID读取、
gRPC状态变更→HTTP读取、watch初始及后续消息、真实第二节点拒绝/转发、配置预分配及PD+etcd WAL重启。
`mcs_132_`2通过；keyspace `mcs_03`61通过；原v2边界2通过、1既有ignored。
`cargo test --locked -p astersql-pd-runtime -- --test-threads=4`包级104通过、0失败、38既有ignored；验收09专用4项均未忽略。
metadata、workspace check、fmt check、Python语法和diff check均通过，全部最终命令exit0。
完整workspace test按本次用户调度集中验收24，未用编译成功代替上述行为证据。

外部etcd仅证明过渡适配与当前WAL重载；原生生产后端的durability仍归验收17–18。
Go源码/模块1101项首尾不变。实际Cargo槽位和最终检查结果以验收09回执为准。
