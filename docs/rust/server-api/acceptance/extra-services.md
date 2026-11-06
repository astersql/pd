# 额外gRPC服务注册与责任交接

2026-11-06，审计基准 `128c50050748fac7820b48039759d77a522929ec`。
**责任计划、MetaStorage前置及classic RM RU子集已有证据，整体协议验收尚未完成。** 最初只读审计之后，
用户已授权实施；MetaStorage见下表65–68，classic RM阶段见[本轮回执](task-16.md#classic-rm-ru前置阶段)。
其它服务继续保留各自缺口，不代表42方法均通过。
检查记录及恢复条件见[task-16.md](task-16.md)。

固定输入：[wire-inventory.md](../wire-inventory.md)第80–121行、
[coverage.json](coverage.json)原始行及准入kvproto
`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
42个稳定ID与原行相等；从准入ZIP读取的42方法无多项/漏项。
保持原清单历史列及ID不变；本表补充其泛化的
“控制面131/138/148及各服务原任务”责任字段，今后审计须联读本表，
不能把原泛化字段当作已逐方法分配或已验收。

## 注册条件与已知证据

| 服务 | Go注册条件 | 当前Rust及证据边界 |
| --- | --- | --- |
| Keyspace | classic始终注册 | 同条件已注册；[131](../../control-plane-mcs/task-131.md)和[验收09](task-09.md)交付manager共享、读写、初始/后续watch及WAL子集；LookupKeyspace双方保留Unimplemented，不是功能完成；流重连/取消/compaction/EOF仍须精确关联回执 |
| Diagnostics | classic始终注册 | 同条件已注册；[14](task-14.md)41配对日志场景，[15](task-15.md)21配对机器信息场景；ServerInfo只准入macOS amd64/CGO=0，退出进程时SearchLog传输错误差异交22；不将历史记录计为本轮执行 |
| ResourceManager | fallback=true由registry装配真实RM；false装配classic proxy | classic按fallback条件注册本地RM或代理；已有12项Go/Rust进程回执、四种PD/RM组合及缓存primary观察，见[阶段证据](task-16.md)。完整TLS、follower故障及148继续实施，不销项100/101/138 |
| MetaStorage | registry始终注册，含classic | 已有canonical四方法及同一listener接线；[65](../../control-plane-mcs/task-065.md)/[66](../../control-plane-mcs/task-066.md)/[67](../../control-plane-mcs/task-067.md)/[68](../../control-plane-mcs/task-068.md)交付当前进程/嵌入、故障/流/WAL证据，148继续完整双模式/TLS组合，不扩大为整体16完成 |
| TSO | 独立MCS注册，classic不注册该服务 | canonical TsoServiceHost及三个方法已存在；[验收11](task-11.md)/[13](task-13.md)只交付已消费子集，不能替代所有方法/TLS/取消的双模式回执 |
| Router | 独立MCS注册，classic不注册该服务 | canonical七方法Service、RouterHost和services router入口已接通；[阶段证据](task-16.md)覆盖Go上游的空/真实缓存查询、错误与EOF，不能替代完整145/148、TLS或配置验收；86/87/137/141仍按各自范围交付 |
| Scheduling | 独立MCS注册，classic不注册该服务 | canonical Rust服务缺失；119–122实现，139嵌入，143独立，147恢复，148差分 |
| Deadlock / TraceRecordPubSub / Namespace | descriptor传递导入，当前Go注册链未见注册 | 仅核对未注册边界，不虚构领域实现。Namespace历史“独立MCS”描述无注册源码支持，不据此新增服务；本轮未执行负向RPC |

Go来源为 `server/server.go:316–341`及五个MCS的
`server/grpc_service.go::RegisterGRPCService`；Rust入口为
`pkg/server/lib.rs::router_with_options`及
`pkg/mcs/tso/server/tso_service_host.rs:113`。
Go Router源码的ScanRegions helper不在固定descriptor中，不能增加为注册方法。

## 逐方法唯一实现拥有者

本表“实现拥有者”只分配方法语义；装配、差分是不同责任层，不是第二实现。
classic未注册的7项只分配审计责任，不分配新领域实现。
实际实现任务继续原线性顺序，不以验收16/25完成为前置。

| 稳定来源ID | 固定服务/方法 | 唯一实现拥有者 |
| --- | --- | --- |
| `wire:e17b42dd6f886f0372095932` | `deadlock.Deadlock/GetWaitForEntries` | 验收16：仅未注册边界 |
| `wire:6fb6a00aa58b90dd21a15ba8` | `deadlock.Deadlock/Detect` | 验收16：仅未注册边界 |
| `wire:ef8b5be6c5113d55d2215d9b` | `resource_manager.ResourceManager/ListResourceGroups` | 控制面100 |
| `wire:3b03fab280d7325da6448677` | `resource_manager.ResourceManager/GetResourceGroup` | 控制面100 |
| `wire:939c1d3fb566aa23bde5c591` | `resource_manager.ResourceManager/AddResourceGroup` | 控制面100 |
| `wire:e5d9dfcc1a114a11264e9005` | `resource_manager.ResourceManager/ModifyResourceGroup` | 控制面100 |
| `wire:315f48e34233558345694e1e` | `resource_manager.ResourceManager/DeleteResourceGroup` | 控制面100 |
| `wire:41fd52349110370dc146081a` | `resource_manager.ResourceManager/AcquireTokenBuckets` | 控制面101 |
| `wire:6760157a800da9106e2ada99` | `tracepb.TraceRecordPubSub/Subscribe` | 验收16：仅未注册边界 |
| `wire:f35d1f2e444550e4b90b62e1` | `keyspacepb.Namespace/CreateNamespace` | 验收16：仅未注册边界 |
| `wire:6a0cea2b1388b7b75a8de838` | `keyspacepb.Namespace/LoadNamespace` | 验收16：仅未注册边界 |
| `wire:3a168d8e446608339598c85c` | `keyspacepb.Namespace/GetAllNamespaces` | 验收16：仅未注册边界 |
| `wire:26898258c7377b580446c567` | `keyspacepb.Namespace/UpdateNamespaceState` | 验收16：仅未注册边界 |
| `wire:1396be1dde54de95c636a380` | `keyspacepb.Keyspace/LoadKeyspace` | 控制面131 |
| `wire:e4aa723a4c35186d20e39c06` | `keyspacepb.Keyspace/LookupKeyspace` | 控制面131 |
| `wire:bab45497756ebf705ecbbe3d` | `keyspacepb.Keyspace/LoadKeyspaceByID` | 控制面131 |
| `wire:c9c01aef0ae87c0da31bf16d` | `keyspacepb.Keyspace/WatchKeyspaces` | 控制面131 |
| `wire:ddd38cf64f9369c1c5e32c65` | `keyspacepb.Keyspace/UpdateKeyspaceState` | 控制面131 |
| `wire:da597dfb2c06f6829f427ed3` | `keyspacepb.Keyspace/GetAllKeyspaces` | 控制面131 |
| `wire:a5a03ba0ba4c676c25e3177d` | `diagnosticspb.Diagnostics/search_log` | 验收14 |
| `wire:652c16ee3bcb0a4e873e87ce` | `diagnosticspb.Diagnostics/server_info` | 验收15 |
| `wire:6a6b90a86873c0331a84283e` | `tsopb.TSO/Tso` | 控制面78 |
| `wire:ce8a3004621529e648a92abc` | `tsopb.TSO/FindGroupByKeyspaceID` | 控制面79 |
| `wire:e3d43f40ba5eaf5e8a0f9825` | `tsopb.TSO/GetMinTS` | 控制面79 |
| `wire:699593e8cf2e419ac96a432c` | `schedulingpb.Scheduling/StoreHeartbeat` | 控制面120 |
| `wire:d13584abfe3daadf8819e464` | `schedulingpb.Scheduling/RegionHeartbeat` | 控制面119 |
| `wire:b8afc494f38f3276acbc8ef0` | `schedulingpb.Scheduling/SplitRegions` | 控制面122 |
| `wire:eb4079b3088f75f87c50eb7b` | `schedulingpb.Scheduling/ScatterRegions` | 控制面122 |
| `wire:06c11b199fc8271de7383e69` | `schedulingpb.Scheduling/GetOperator` | 控制面122 |
| `wire:66a98a89c2c80ac5573cba5b` | `schedulingpb.Scheduling/AskBatchSplit` | 控制面121 |
| `wire:24be74d67ce37728d2eaca9f` | `schedulingpb.Scheduling/RegionBuckets` | 控制面120 |
| `wire:0883781e75d62818a67168f5` | `meta_storagepb.MetaStorage/Watch` | 控制面67 |
| `wire:b211befe98fd66909e97212f` | `meta_storagepb.MetaStorage/Get` | 控制面65 |
| `wire:0cea8d5001f0baf5449a6d48` | `meta_storagepb.MetaStorage/Put` | 控制面66 |
| `wire:453882faaa41bd44f75abe16` | `meta_storagepb.MetaStorage/Delete` | 控制面66 |
| `wire:97cfa63b5dc6f8c106cb0e92` | `routerpb.Router/GetRegion` | 控制面86 |
| `wire:05048320c392d22119efc567` | `routerpb.Router/GetPrevRegion` | 控制面86 |
| `wire:d524cc7c25febe7c5c67c433` | `routerpb.Router/GetRegionByID` | 控制面86 |
| `wire:3cec09d7dd369a9064f4d33a` | `routerpb.Router/QueryRegion` | 控制面86 |
| `wire:be76ac8ee8c178f2158a3546` | `routerpb.Router/BatchScanRegions` | 控制面87 |
| `wire:66bd9cd4443003641b407cf2` | `routerpb.Router/GetStore` | 控制面86 |
| `wire:737f1ca1a6e7d55493cfe47b` | `routerpb.Router/GetAllStores` | 控制面86 |

## RM注册与代理补充实施计划

以下均为**已写入编号任务的待实施要求**，原任务状态未改为完成：

- [100](../../../../.plans/2026-10-29-pd-rust-控制面与MCS/22-资源管理/100-暴露资源组gRPC管理.md)：五个元数据方法同时覆盖真实RM与classic proxy。
  List/Get走RM primary；Add/Modify/Delete先校验参数，按forwarded-host和PD leader路由，
  本地写复用PDMetaOnly manager。定义唯一发现/连接池资源供101复用。
  增补server独立适配测试、错误码/消息、连接重建及真实持久副作用。
- [101](../../../../.plans/2026-10-29-pd-rust-控制面与MCS/22-资源管理/101-暴露令牌申请流.md)：AcquireTokenBuckets真实RM及classic proxy；
  复用100公共连接，覆盖双向消息、CloseSend/最后响应/EOF、两端取消和断连清理，
  不重放消费/令牌请求，不复制令牌算法。
- [138](../../../../.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/138-装配嵌入资源管理服务.md)：唯一拥有classic fallback条件装配；
  true注册真实服务，false注册100/101的同一个代理服务，并保持PD元数据manager初始化；
  同listener只注册一个ResourceManager，验证双实例、启动失败和关闭资源边界。
- [148](../../../../.plans/2026-10-29-pd-rust-控制面与MCS/35-主题验收/148-回归双部署模式协议.md)：唯一拥有最终双部署差分交付；
  消费42个wire ID，覆盖fallback两分支、非leader/TLS拒绝、适用流故障、
  原始响应与metadata revision/余额等副作用。缺项逐行保留，不批量销项。

未重写领域或修改只读plan.md。保留历史task-002覆盖压缩回执不变；
[控制面coverage补充](../../control-plane-mcs/coverage.md#验收16补充classic-rm代理责任2026-11-06)
使历史未收录的proxy来源获得当前明确责任。

## 独立RM增量证据（2026-11-07）

六个RM方法现在也由`pkg/mcs/resourcemanager/server/service_host.rs`在独立listener
提供；`services resource-manager`不再是缺失入口。现成Go PD为双方共享的
元数据上游，Go/Rust sidecar对照覆盖注册/participant ID、五个管理方法、token流、
实时设置/删除及保留流关闭。三个管理写方法按独立服务策略拒绝；设置事件不重复
增加token delta。详见[阶段证据](task-16.md#rm配额协调与独立宿主阶段)。

上述仍不是classic fallback=false代理、TLS、watch compaction或148整体验收，
42个wire ID与唯一拥有任务保持原分配，不能批量标记100/101/138完成。

## Classic RM代理增量（2026-11-07）

fallback条件装配与五加一代理现已有运行实现，见
[代理阶段证据](task-16.md#classic-rm代理与连接池阶段)。宿主明确选择metadata-only
写角色；四种PD/RM语言组合已对半关闭和protobuf响应做实际比较。连接池集中于
canonical MCS公共包。原任务继续保留，尚缺完整primary缓存、follower/TLS故障矩阵
及其他服务装配；此增量不改42个wire ID及唯一方法拥有者。

## Scheduling注册增量（2026-11-07）

七个Scheduling方法已有canonical实现与独立入口，复用现有ClusterHost领域能力。
[当前阶段证据](task-16.md#scheduling注册心跳角色与pd-id所有权阶段进行中)包含真实
Go/Rust心跳、错误/EOF、空请求与PD-owned分裂ID对照，以及空闲调度器不消耗ID、
不持久化PD region元数据的副作用验证。119–123/139/143/147/148尚未整体验收。
全MCS公共Diagnostics/health仍需装配，不能把业务七方法等同完整listener注册。

## 公共辅助服务注册增量（2026-11-07）

四种MCS的listener均须同时提供Diagnostics和grpc health；仅注册业务descriptor
不满足Go的StartGRPCAndHTTPServers。Diagnostics与classic PD共用canonical实现，
health Check/List/Watch采用固定grpc-go descriptor，独立于本表42个kvproto wire ID。
[当前阶段证据](task-16.md#公共mcs-diagnostics与health注册阶段进行中)保留四种MCS
初始全部Unimplemented的真实差分，不改变Diagnostics原14/15及装配62/88/123/148责任。

## MCS前端TLS/配置增量（2026-11-07）

四种MCS共用配置文件/非空参数合并和宿主日志入口，TLS沿用同一认证内核的MCS策略，
有CA无CN限制时客户端证书可选，单个CN也限制。120次真实Go/Rust认证探测、RM无CA
系统根验证、参数合并与controller TOML启动类型对照已通过；详见
[阶段说明](task-16.md#mcs前端tls与共用配置入口阶段进行中)。
这不关闭完整SDK HTTPS backend、TSO HTTPS transition、配置/租约及148领域故障矩阵。

## MCS原生SDK后端传输增量（2026-11-07）

四种进程共用后端传输工厂，原生etcd SDK通过现有delegate内核连接HTTP或HTTPS，
保留每端点独立健康探测与宿主取消。真实RM双向TLS注册red及存储契约见
[阶段说明](task-16.md#mcs-https后端与原生sdk传输阶段进行中)。
不因此关闭TSO HTTPS transition、各领域完整故障差分及148责任。

## TSO HTTPS completion增量（2026-11-07）

TSO独立宿主接受调用者的TLS策略，完成split/merge时共用从metrics提取的请求级
HTTP驱动，保留取消、超时及响应流资源所有权；旧无策略入口对HTTPS仍失败关闭。
[阶段证据](task-16.md#tso-https交接与公共http驱动阶段进行中)区分TLS传输专项、
metrics Rust回归与完整分组交接差分，不据此关闭148。

## MCS名称与primary lease增量（2026-11-07）

角色默认name和配置primary lease已有实际Go/Rust租约/registry对照，注册lease独立5，
Router不选primary；controller表只由RM解码。参见
[阶段记录](task-16.md#mcs名称与primary租约配置阶段进行中)。
未bootstrap的Scheduling初始化/primary生命周期仍需专项验证，完整配置和148未关闭。
