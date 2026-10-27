# schedule_admin_shuffle_region.rs

`serve(&mut ShuffleRegionScheduler, &StorageEndpoint, &ClusterPaths, Request<Body>)`
是 scheduler 局部 HTTP 适配；宿主负责挂载、独占访问和 HEAD wire 层处理。
不创建 runtime、不安装全局路由，不复制领域调度实现。

只读 Go `shuffle_region.go` 将 ServeHTTP 委托到 `shuffle_region_config.go`，
后者使用 Gorilla mux：GET /list 与 GET /roles 读取角色，POST /roles 更新。
未知路径404，方法不匹配405，无 Allow；清理路径301，保留查询；路由匹配已解码路径。
GET 保留 null、空数组和重复角色，按 render v1.0.1 输出缩进JSON、UTF-8 charset。
POST 成功返回纯文本 Config is updated.；非法角色400纯文本；持久化失败500纯文本。
JSON错误保留 input 错误结构、Offset 和 tidb-error-code；请求体I/O错误为internal。

领域 `update_optional_roles` 保留 null 与 [] 的区别，验证所有字符串后保存候选配置，
仅保存成功才发布；原有 update_roles 委托同一事务路径。范围保持原样，失败不会污染
内存或持久化。外部I/O可注入故障，其余使用真实 MemoryStore/core/placement/operator。

来源为只读源码及已有测试推导，用户已授权替代缺失Go运行oracle。
没有执行Go或声称运行差分。固定场景、字节/header规则、随机时间控制与验证日志见
[任务151](../../docs/rust/schedule-execution/task-151.md)。
