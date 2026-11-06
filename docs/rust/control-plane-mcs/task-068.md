# 任务68：元存储宿主装配

2026-11-06，基准 `9ceedb1beb0e1f3be359f7a7d3f9eb5186ce3850`。
本项行为已完成；[共享原始回执](../server-api/acceptance/task-16-metastorage.json.gz)
保存确切argv/退出码、所有red/green日志、原始RPC/状态、输入/源码/二进制摘要及
Go/模块/plan.md守卫。[阶段汇总](../server-api/acceptance/task-16.md)区分
本项通过与额外完整server包的8项未通过；后者交验收22/23/24，不计通过。
同源Go oracle、kvproto ZIP、protoc和官方etcd/etcdctl五项摘要已重算匹配。
没有Go构建/测试、源码/模块改写、make或failpoint注入；现成Go仅运行服务及只读build-info检查。

在现有classic listener与PD/Keyspace/Diagnostics同一tonic路由注册MetaStorage，
对应Go总是安装MetaStorage registry服务的条件。窄桥接
`pkg/server/grpc_metastorage.rs`读取原PdServices的identity、healthy及discovery_store，
复用同一个MetaStore和宿主取消令牌；canonical服务不反向依赖runtime。
没有平行BaseServer、私有runtime、全局信号或第二份元数据状态。
旧registry初始化回调的可见性由现有宿主ready/每请求admission提供，未复制管理内核。

mcs_068_shared_host_state_and_stream_shutdown在调用方同一Tokio runtime启动两个
真实listener节点，Put回读传入的同一后端并核验revision，follower Get被拒绝；
打开历史流后关闭leader，流正常EOF，第二节点成为leader并读取原值。
父取消令牌未取消，关闭两个实例后宿主runtime仍能执行任务。
默认二进制使用同一服务路由，三项现成Go/Rust真实进程差分覆盖四个RPC；
Go嵌入etcd与Rust外部官方etcd两侧WAL重启后读值及重放保持。

专项1通过；runtime完整包106通过/0失败/38既有忽略。
其中原AskBatchSplit故障夹具被后台预取128个ID干扰的失败已保存并局部修复：
只在夹具中禁用自动scheduler及延迟checker巡检，不用halt-scheduling阻断手工RPC，
保留原真实后端故障/取消/不写入断言，专项与完整包随后通过。
完整server包额外检查的8项未通过仍如实保留，不将该包标为通过；
MetaStorage整体双模式/TLS矩阵由148接收，不据本项销项其他MCS或原生后端。
