# 批量拆分管理响应

`Handler::split_regions` 先校验 coordinator，再一次性解码全部 hex keys，保留输入顺序、
重复键与空键。错误为 `SplitAdminError::Handler` / `Hex`，复用现有 hex 解析器。
此入口返回拥有状态的 `SplitRegions`；它与 canonical splitter 一样由宿主推进，
并非一个内部创建运行时、线程或轮询任务的同步 RPC。

宿主在 Ready 阶段通过 `Handler::start_split_attempt` 使用 coordinator 的真实
RegionSplitter/controller/checker，派发返回的 `SplitSubmissions`，保留其中类型化
构建/准入错误；随后以单调毫秒时钟、取消标志和当前 canonical `RegionsInfo`
调用 `advance`。Watching 的 100ms tick、60s 超时、inclusive retry limit、指数退避
及取消后的最终 sleep 均委托既有 `SplitRetry`，没有复制业务逻辑。宿主必须持续推进
Backoff；提前丢弃状态不会生成响应，也不会撤销已提交的操作。

只有 Done 阶段的 `response()` 返回 `SplitRegionsResponse`，其它阶段返回 None。
百分比只计入已扫描到的 region start keys，操作准入不证明拆分成功。ID 由 canonical
实现按集合去重并升序输出，规范化 Go map 顺序，除此以外不忽略字段。管理层不写
MetaStore；底层规则测试使用真实 MemoryStore/StorageEndpoint，不引入假存储故障。

Go `handler.go::SplitRegions` 与 `region_splitter.go::SplitRegions` 是只读来源。
空输入提前返回 `(0,nil)`，JSON 为 `{"processed-percentage":0,"regions-id":null}`；
非空输入的零观察结果为 `regions-id:[]`。因此 ID 字段用 `Option<Vec<u64>>` 区分两种
形状。serde 字段名只描述响应表示，不依赖 HTTP/server/protobuf；路由装配在范围外。
Go 的 `splitResponses` failpoint 是测试覆盖口，不作为生产 API 移植。

用户授权以源码推导加 Rust 真实行为/故障验证替代缺失的 Go 运行 oracle。本任务没有
运行 Go 或声称运行差分。[验证证据](../../../docs/rust/schedule-execution/task-126.md)
记录来源、正常/边界/取消/重试/准入故障及同测试对错误实现的红灯。
