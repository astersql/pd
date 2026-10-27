# 热点均衡配置 HTTP

生产入口 `serve(&mut HotRegionConfig, &StorageEndpoint, &ClusterPaths, Request<Body>)`
接受 scheduler 相对路径；宿主负责同步和挂载。模块不创建 runtime、全局路由或
状态副本。复用 `HotRegionConfig` 的验证、有效视图、发布与持久化策略。

GET `/list` 返回有效参数，能力通过 `set_support` 注入；不输出 disabled，none
方向省略。POST `/config` 合并原始参数，方法不匹配405空体，无Allow；未知路径404。
成功更新、相同值、未知字段分别返回纯文本；JSON/读取错误500，领域验证400并
返回含 `PD:scheduler:ErrSchedulerConfig` 的JSON字符串。保存失败仍200且保留
新配置；reload恢复旧持久化值。POST不修改宿主的版本能力标记。

依据用户明确授权，契约来自只读 `hot_region.go`/`hot_region_config.go` 及相邻测试
推导，不主张Go运行差分。顶层字段顺序和重复键保留；scalar null忽略，数组null
交给领域空列表，GET按能力恢复有效优先级。方法/路径匹配保持scheduler局部语义；
Gorilla路径规范化重定向与完整server/api装配不在此接口。

证据及覆盖范围见 [task-144](../../docs/rust/schedule-execution/task-144.md)。
JSON比较只归一化键序、缩进、尾换行和等值数值表示；I/O异常来自真实读取流和
MemoryStore契约边界。时间/随机固定在测试输入，不宣称跨语言随机分布等价。
