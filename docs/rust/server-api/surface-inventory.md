# 配置、指标、命令与安全入口契约清单

基准 HEAD：`ca5ae0ce726d54df173fe56f6046f08c20425b32`。任务1、2清单已读取，文档依赖满足。此处仅盘点事实，不解除任何行为准入。

任务17补记：运行时从恢复的 `schedulers-v2`（C-099）及等待operator上限（C-082）装配既有Coordinator，保留完整动态配置与StoreConfig；证据见 [task-017](task-017.md)。通用动态配置API、完整策略指标与安全表面仍归各owner。

## 证据口径与每行验收规则

下列表格的源码列是 Go 静态事实；默认表达式保留原文与控制条件，不把未指定值一律解释为零值。完整默认/废弃处理块附后，表达式引用的常量按原文冻结。源码不是运行差分。每行以类别加来源路径及行号作为唯一案例 ID；metric 以构造位置去重，不按指标名去重（不同进程可能同名）。

| 事实来源 | 已有冻结输出 | 本次实际运行 |
| --- | --- | --- |
| 下表原字节来源/符号与 manifest | `server/config/fixtures/nested_config_go.json` 仅嵌套配置案例，非全量默认输出；主题6日志见 task-001 | 只读扫描、哈希、链接/owner/覆盖核验；0行为测试 |

每行的共同准入：任务4只读 oracle；任务5–7共享状态和角色（涉及服务时）；对应主题8/9领域必须先验收。每行所需测试：与同名字段/flag/指标/安全入口一一对应的 Go/Rust 成功、非法输入、优先级、取消和实际副作用差分；配置另测缺省/显式零/未知/废弃冲突、持久化与热更新回滚；指标另测 HELP/TYPE/label顺序、注册冲突、采集触发及关闭清理；安全另测拒绝访问、身份/证书更新、审计一次性及限流释放。

每行实际运行证据均缺新差分；预期位置 `docs/rust/server-api/task-NNN.md`（owner编号对应；多个owner分别验收），主题8/9证据由来源领域计划提供。无新证据不得自动完成。现有 Rust 路径只表示候选可复用 API，不能视为行为等价。

## Rust 实际入口与缺口

| 表面 | 当前 Rust | owner与限制 |
| --- | --- | --- |
| 配置值/解码/默认/废弃/持久化 | [server/config/lib.rs](../../../server/config/lib.rs)；同目录 config_decode/config_defaults/deprecated_config/nested_config/middleware_config/persist_schedule 及独立测试 | 47–50/90：库已存在，CLI与完整服务热更新接线待验收 |
| 独立进程参数 | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs) | 90/91：手写参数解析；要求外部etcd；部分Go元数据flag被忽略；help/退出码未等价 |
| listener/宿主 Registry | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs)；[pkg/server/http.rs](../../../pkg/server/http.rs) | 82–87/94–96：当前http MVP与宿主Registry；不能推断TLS/认证/指标全量完成 |
| 协议/路由 | [pkg/server/grpc.rs](../../../pkg/server/grpc.rs)；[wire-inventory](wire-inventory.md) | 方法/路由归属复用任务2，无证据保持待验收 |

## 配置字段（嵌套结构名限定，JSON/TOML原文）

| ID/来源 | 结构.字段与类型/tags | default/adjust直接引用（条件见附录） | Rust候选 | owner |
| --- | --- | --- | --- | --- |
| C-001 [client/resource_group/controller/config.go:76](../../../client/resource_group/controller/config.go#L76) | TokenRPCParams.WaitRetryInterval Duration &#96;toml:"wait-retry-interval" json:"wait-retry-interval"&#96; | L141: if c.WaitRetryInterval.Duration == 0 { ; L142: c.WaitRetryInterval = NewDuration(defaultWaitRetryInterval) ; L145: if int(c.LTBTokenRPCMaxDelay.Duration) != int(c.WaitRetryInterval.Duration)*c.WaitRetryTimes { ; L146: c.WaitRetryTimes = int(c.LTBTokenRPCMaxDelay.Duration / c.WaitRetryInterval.Duration) ; L162: WaitRetryInterval: NewDuration(defaultWaitRetryInterval), ; L244: WaitRetryInterval:        config.WaitRetryInterval.Duration, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-002 [client/resource_group/controller/config.go:79](../../../client/resource_group/controller/config.go#L79) | TokenRPCParams.WaitRetryTimes int &#96;toml:"wait-retry-times" json:"wait-retry-times"&#96; | L145: if int(c.LTBTokenRPCMaxDelay.Duration) != int(c.WaitRetryInterval.Duration)*c.WaitRetryTimes { ; L146: c.WaitRetryTimes = int(c.LTBTokenRPCMaxDelay.Duration / c.WaitRetryInterval.Duration) ; L163: WaitRetryTimes:    defaultWaitRetryTimes, ; L245: WaitRetryTimes:           config.WaitRetryTimes, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-003 [client/resource_group/controller/config.go:84](../../../client/resource_group/controller/config.go#L84) | LocalBucketConfig.TokenRPCParams &#96;toml:"token-rpc-params" json:"token-rpc-params"&#96; | L161: TokenRPCParams: TokenRPCParams{ | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-004 [client/resource_group/controller/config.go:91](../../../client/resource_group/controller/config.go#L91) | BaseConfig.DegradedModeWaitDuration Duration &#96;toml:"degraded-mode-wait-duration" json:"degraded-mode-wait-duration"&#96; | L154: DegradedModeWaitDuration: NewDuration(defaultDegradedModeWaitDuration), ; L246: DegradedModeWaitDuration: config.DegradedModeWaitDuration.Duration, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-005 [client/resource_group/controller/config.go:94](../../../client/resource_group/controller/config.go#L94) | BaseConfig.LTBMaxWaitDuration Duration &#96;toml:"ltb-max-wait-duration" json:"ltb-max-wait-duration"&#96; | L138: if c.LTBMaxWaitDuration.Duration == 0 { ; L139: c.LTBMaxWaitDuration = NewDuration(defaultMaxWaitDuration) ; L157: LTBMaxWaitDuration:       NewDuration(defaultMaxWaitDuration), ; L243: LTBMaxWaitDuration:       config.LTBMaxWaitDuration.Duration, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-006 [client/resource_group/controller/config.go:97](../../../client/resource_group/controller/config.go#L97) | BaseConfig.LTBTokenRPCMaxDelay Duration &#96;toml:"ltb-token-rpc-max-delay" json:"ltb-token-rpc-max-delay"&#96; | L145: if int(c.LTBTokenRPCMaxDelay.Duration) != int(c.WaitRetryInterval.Duration)*c.WaitRetryTimes { ; L146: c.WaitRetryTimes = int(c.LTBTokenRPCMaxDelay.Duration / c.WaitRetryInterval.Duration) ; L158: LTBTokenRPCMaxDelay:      NewDuration(defaultLTBTokenRPCMaxDelay), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-007 [client/resource_group/controller/config.go:101](../../../client/resource_group/controller/config.go#L101) | BaseConfig.RequestUnit RequestUnitConfig &#96;toml:"request-unit" json:"request-unit"&#96; | L155: RequestUnit:              DefaultRequestUnitConfig(), ; L236: ReadBaseCost:             RequestUnit(config.RequestUnit.ReadBaseCost), ; L237: ReadPerBatchBaseCost:     RequestUnit(config.RequestUnit.ReadPerBatchBaseCost), ; L238: ReadBytesCost:            RequestUnit(config.RequestUnit.ReadCostPerByte), ; L239: WriteBaseCost:            RequestUnit(config.RequestUnit.WriteBaseCost), ; L240: WritePerBatchBaseCost:    RequestUnit(config.RequestUnit.WritePerBatchBaseCost), ; L241: WriteBytesCost:           RequestUnit(config.RequestUnit.WriteCostPerByte), ; L242: CPUMsCost:                RequestUnit(config.RequestUnit.CPUMsCost), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-008 [client/resource_group/controller/config.go:104](../../../client/resource_group/controller/config.go#L104) | BaseConfig.EnableControllerTraceLog bool &#96;toml:"enable-controller-trace-log" json:"enable-controller-trace-log,string"&#96; | L156: EnableControllerTraceLog: false, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-009 [client/resource_group/controller/config.go:107](../../../client/resource_group/controller/config.go#L107) | BaseConfig.RUVersionPolicy *RUVersionPolicy &#96;toml:"ru-version-policy" json:"ru-version-policy,omitempty"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-010 [client/resource_group/controller/config.go:125](../../../client/resource_group/controller/config.go#L125) | RUVersionPolicy.Default   RUVersion            &#96;json:"default"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-011 [client/resource_group/controller/config.go:126](../../../client/resource_group/controller/config.go#L126) | RUVersionPolicy.Overrides map[uint32]RUVersion &#96;json:"overrides,omitempty"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-012 [client/resource_group/controller/config.go:174](../../../client/resource_group/controller/config.go#L174) | RequestUnitConfig.ReadBaseCost float64 &#96;toml:"read-base-cost" json:"read-base-cost"&#96; | L194: ReadBaseCost:          defaultReadBaseCost, ; L236: ReadBaseCost:             RequestUnit(config.RequestUnit.ReadBaseCost), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-013 [client/resource_group/controller/config.go:176](../../../client/resource_group/controller/config.go#L176) | RequestUnitConfig.ReadPerBatchBaseCost float64 &#96;toml:"read-per-batch-base-cost" json:"read-per-batch-base-cost"&#96; | L195: ReadPerBatchBaseCost:  defaultReadPerBatchBaseCost, ; L237: ReadPerBatchBaseCost:     RequestUnit(config.RequestUnit.ReadPerBatchBaseCost), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-014 [client/resource_group/controller/config.go:178](../../../client/resource_group/controller/config.go#L178) | RequestUnitConfig.ReadCostPerByte float64 &#96;toml:"read-cost-per-byte" json:"read-cost-per-byte"&#96; | L196: ReadCostPerByte:       defaultReadCostPerByte, ; L238: ReadBytesCost:            RequestUnit(config.RequestUnit.ReadCostPerByte), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-015 [client/resource_group/controller/config.go:181](../../../client/resource_group/controller/config.go#L181) | RequestUnitConfig.WriteBaseCost float64 &#96;toml:"write-base-cost" json:"write-base-cost"&#96; | L197: WriteBaseCost:         defaultWriteBaseCost, ; L239: WriteBaseCost:            RequestUnit(config.RequestUnit.WriteBaseCost), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-016 [client/resource_group/controller/config.go:183](../../../client/resource_group/controller/config.go#L183) | RequestUnitConfig.WritePerBatchBaseCost float64 &#96;toml:"write-per-batch-base-cost" json:"write-per-batch-base-cost"&#96; | L198: WritePerBatchBaseCost: defaultWritePerBatchBaseCost, ; L240: WritePerBatchBaseCost:    RequestUnit(config.RequestUnit.WritePerBatchBaseCost), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-017 [client/resource_group/controller/config.go:185](../../../client/resource_group/controller/config.go#L185) | RequestUnitConfig.WriteCostPerByte float64 &#96;toml:"write-cost-per-byte" json:"write-cost-per-byte"&#96; | L199: WriteCostPerByte:      defaultWriteCostPerByte, ; L241: WriteBytesCost:           RequestUnit(config.RequestUnit.WriteCostPerByte), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-018 [client/resource_group/controller/config.go:188](../../../client/resource_group/controller/config.go#L188) | RequestUnitConfig.CPUMsCost float64 &#96;toml:"read-cpu-ms-cost" json:"read-cpu-ms-cost"&#96; | L200: CPUMsCost:             defaultCPUMsCost, ; L242: CPUMsCost:                RequestUnit(config.RequestUnit.CPUMsCost), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-019 [pkg/encryption/config.go:43](../../../pkg/encryption/config.go#L43) | Config.DataEncryptionMethod string &#96;toml:"data-encryption-method" json:"data-encryption-method"&#96; | L52: if len(c.DataEncryptionMethod) == 0 { ; L53: c.DataEncryptionMethod = defaultDataEncryptionMethod | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-020 [pkg/encryption/config.go:45](../../../pkg/encryption/config.go#L45) | Config.DataKeyRotationPeriod typeutil.Duration &#96;toml:"data-key-rotation-period" json:"data-key-rotation-period"&#96; | L58: if c.DataKeyRotationPeriod.Duration == 0 { ; L65: c.DataKeyRotationPeriod.Duration = duration | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-021 [pkg/encryption/config.go:47](../../../pkg/encryption/config.go#L47) | Config.MasterKey MasterKeyConfig &#96;toml:"master-key" json:"master-key"&#96; | L71: if len(c.MasterKey.Type) == 0 { ; L72: c.MasterKey.Type = masterKeyTypePlaintext ; L110: KeyId:    c.MasterKey.KmsKeyID, ; L111: Region:   c.MasterKey.KmsRegion, ; L112: Endpoint: c.MasterKey.KmsEndpoint, ; L120: Path: c.MasterKey.FilePath, ; L126: "unrecognized encryption master key type: %s", c.MasterKey.Type) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-022 [pkg/encryption/config.go:133](../../../pkg/encryption/config.go#L133) | MasterKeyConfig.Type string &#96;toml:"type" json:"type"&#96; | L71: if len(c.MasterKey.Type) == 0 { ; L72: c.MasterKey.Type = masterKeyTypePlaintext ; L126: "unrecognized encryption master key type: %s", c.MasterKey.Type) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-023 [pkg/encryption/config.go:142](../../../pkg/encryption/config.go#L142) | MasterKeyKMSConfig.KmsKeyID string &#96;toml:"key-id" json:"key-id"&#96; | L110: KeyId:    c.MasterKey.KmsKeyID, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-024 [pkg/encryption/config.go:144](../../../pkg/encryption/config.go#L144) | MasterKeyKMSConfig.KmsRegion string &#96;toml:"region" json:"region"&#96; | L111: Region:   c.MasterKey.KmsRegion, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-025 [pkg/encryption/config.go:146](../../../pkg/encryption/config.go#L146) | MasterKeyKMSConfig.KmsEndpoint string &#96;toml:"endpoint" json:"endpoint"&#96; | L112: Endpoint: c.MasterKey.KmsEndpoint, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-026 [pkg/encryption/config.go:152](../../../pkg/encryption/config.go#L152) | MasterKeyFileConfig.FilePath string &#96;toml:"path" json:"path"&#96; | L120: Path: c.MasterKey.FilePath, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-027 [pkg/mcs/resourcemanager/server/config.go:68](../../../pkg/mcs/resourcemanager/server/config.go#L68) | Config.BackendEndpoints    string &#96;toml:"backend-endpoints" json:"backend-endpoints"&#96; | L295: configutil.AdjustCommandLineString(flagSet, &c.BackendEndpoints, "backend-endpoints") ; L317: configutil.AdjustString(&c.BackendEndpoints, defaultBackendEndpoints) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-028 [pkg/mcs/resourcemanager/server/config.go:69](../../../pkg/mcs/resourcemanager/server/config.go#L69) | Config.ListenAddr          string &#96;toml:"listen-addr" json:"listen-addr"&#96; | L296: configutil.AdjustCommandLineString(flagSet, &c.ListenAddr, "listen-addr") ; L318: configutil.AdjustString(&c.ListenAddr, defaultListenAddr) ; L319: configutil.AdjustString(&c.AdvertiseListenAddr, c.ListenAddr) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-029 [pkg/mcs/resourcemanager/server/config.go:70](../../../pkg/mcs/resourcemanager/server/config.go#L70) | Config.AdvertiseListenAddr string &#96;toml:"advertise-listen-addr" json:"advertise-listen-addr"&#96; | L297: configutil.AdjustCommandLineString(flagSet, &c.AdvertiseListenAddr, "advertise-listen-addr") ; L319: configutil.AdjustString(&c.AdvertiseListenAddr, c.ListenAddr) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-030 [pkg/mcs/resourcemanager/server/config.go:71](../../../pkg/mcs/resourcemanager/server/config.go#L71) | Config.Name                string &#96;toml:"name" json:"name"&#96; | L288: configutil.AdjustCommandLineString(flagSet, &c.Name, "name") ; L309: if c.Name == "" { ; L314: configutil.AdjustString(&c.Name, fmt.Sprintf("%s-%s", defaultName, hostname)) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-031 [pkg/mcs/resourcemanager/server/config.go:73](../../../pkg/mcs/resourcemanager/server/config.go#L73) | Config.Metric metricutil.MetricConfig &#96;toml:"metric" json:"metric"&#96; | L291: configutil.AdjustCommandLineString(flagSet, &c.Metric.PushAddress, "metrics-addr") | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-032 [pkg/mcs/resourcemanager/server/config.go:76](../../../pkg/mcs/resourcemanager/server/config.go#L76) | Config.Log      log.Config &#96;toml:"log" json:"log"&#96; | L289: configutil.AdjustCommandLineString(flagSet, &c.Log.Level, "log-level") ; L290: configutil.AdjustCommandLineString(flagSet, &c.Log.File.Filename, "log-file") ; L334: c.Log.DisableErrorVerbose = constant.DefaultDisableErrorVerbose ; L336: configutil.AdjustString(&c.Log.Format, constant.DefaultLogFormat) ; L337: configutil.AdjustString(&c.Log.Level, constant.DefaultLogLevel) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-033 [pkg/mcs/resourcemanager/server/config.go:80](../../../pkg/mcs/resourcemanager/server/config.go#L80) | Config.Security configutil.SecurityConfig &#96;toml:"security" json:"security"&#96; | L292: configutil.AdjustCommandLineString(flagSet, &c.Security.CAPath, "cacert") ; L293: configutil.AdjustCommandLineString(flagSet, &c.Security.CertPath, "cert") ; L294: configutil.AdjustCommandLineString(flagSet, &c.Security.KeyPath, "key") ; L322: if err := c.Security.Encryption.Adjust(); err != nil { | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-034 [pkg/mcs/resourcemanager/server/config.go:89](../../../pkg/mcs/resourcemanager/server/config.go#L89) | Config.LeaderLease int64 &#96;toml:"lease" json:"lease"&#96; | L327: configutil.AdjustInt64(&c.LeaderLease, constant.DefaultLease) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-035 [pkg/mcs/resourcemanager/server/config.go:91](../../../pkg/mcs/resourcemanager/server/config.go#L91) | Config.Controller ControllerConfig &#96;toml:"controller" json:"controller"&#96; | L326: c.Controller.Adjust(configMetaData.Child("controller")) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-036 [pkg/mcs/resourcemanager/server/config.go:93](../../../pkg/mcs/resourcemanager/server/config.go#L93) | Config.Metering config.MeteringConfig &#96;toml:"metering" json:"metering"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-037 [pkg/mcs/resourcemanager/server/config.go:111](../../../pkg/mcs/resourcemanager/server/config.go#L111) | RUVersionPolicy.Default   RUVersion            &#96;json:"default"&#96; | L133: if p.Default <= 0 { ; L149: clone := &RUVersionPolicy{Default: p.Default} | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-038 [pkg/mcs/resourcemanager/server/config.go:112](../../../pkg/mcs/resourcemanager/server/config.go#L112) | RUVersionPolicy.Overrides map[uint32]RUVersion &#96;json:"overrides,omitempty"&#96; | L119: if version, ok := p.Overrides[keyspaceID]; ok { ; L136: for ks, v := range p.Overrides { ; L150: if p.Overrides != nil { ; L151: clone.Overrides = make(map[uint32]RUVersion, len(p.Overrides)) ; L152: for keyspaceID, version := range p.Overrides { ; L153: clone.Overrides[keyspaceID] = version | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-039 [pkg/mcs/resourcemanager/server/config.go:162](../../../pkg/mcs/resourcemanager/server/config.go#L162) | ControllerConfig.DegradedModeWaitDuration typeutil.Duration &#96;toml:"degraded-mode-wait-duration" json:"degraded-mode-wait-duration"&#96; | L200: configutil.AdjustDuration(&rmc.DegradedModeWaitDuration, defaultDegradedModeWaitDuration) ; L213: configutil.AdjustDuration(&rmc.DegradedModeWaitDuration, time.Second) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-040 [pkg/mcs/resourcemanager/server/config.go:165](../../../pkg/mcs/resourcemanager/server/config.go#L165) | ControllerConfig.LTBMaxWaitDuration typeutil.Duration &#96;toml:"ltb-max-wait-duration" json:"ltb-max-wait-duration"&#96; | L203: configutil.AdjustDuration(&rmc.LTBMaxWaitDuration, defaultMaxWaitDuration) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-041 [pkg/mcs/resourcemanager/server/config.go:168](../../../pkg/mcs/resourcemanager/server/config.go#L168) | ControllerConfig.LTBTokenRPCMaxDelay typeutil.Duration &#96;toml:"ltb-token-rpc-max-delay" json:"ltb-token-rpc-max-delay"&#96; | L206: configutil.AdjustDuration(&rmc.LTBTokenRPCMaxDelay, defaultLTBTokenRPCMaxDelay) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-042 [pkg/mcs/resourcemanager/server/config.go:172](../../../pkg/mcs/resourcemanager/server/config.go#L172) | ControllerConfig.RequestUnit RequestUnitConfig &#96;toml:"request-unit" json:"request-unit"&#96; | L198: rmc.RequestUnit.Adjust(meta.Child("request-unit")) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-043 [pkg/mcs/resourcemanager/server/config.go:175](../../../pkg/mcs/resourcemanager/server/config.go#L175) | ControllerConfig.EnableControllerTraceLog bool &#96;toml:"enable-controller-trace-log" json:"enable-controller-trace-log,string"&#96; | L214: configutil.AdjustBool(&rmc.EnableControllerTraceLog, true) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-044 [pkg/mcs/resourcemanager/server/config.go:178](../../../pkg/mcs/resourcemanager/server/config.go#L178) | ControllerConfig.PushMetricsAddress string &#96;toml:"push-metrics-address" json:"push-metrics-address"&#96; | L208: if rmc.PushMetricsAddress != "" && rmc.PushMetricsInterval.Duration <= 0 { ; L210: rmc.PushMetricsAddress = "" | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-045 [pkg/mcs/resourcemanager/server/config.go:181](../../../pkg/mcs/resourcemanager/server/config.go#L181) | ControllerConfig.PushMetricsInterval typeutil.Duration &#96;toml:"push-metrics-interval" json:"push-metrics-interval"&#96; | L208: if rmc.PushMetricsAddress != "" && rmc.PushMetricsInterval.Duration <= 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-046 [pkg/mcs/resourcemanager/server/config.go:183](../../../pkg/mcs/resourcemanager/server/config.go#L183) | ControllerConfig.RUVersionPolicy *RUVersionPolicy &#96;toml:"ru-version-policy" json:"ru-version-policy,omitempty"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-047 [pkg/mcs/resourcemanager/server/config.go:223](../../../pkg/mcs/resourcemanager/server/config.go#L223) | RequestUnitConfig.ReadBaseCost float64 &#96;toml:"read-base-cost" json:"read-base-cost"&#96; | L246: configutil.AdjustFloat64(&ruc.ReadBaseCost, defaultReadBaseCost) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-048 [pkg/mcs/resourcemanager/server/config.go:225](../../../pkg/mcs/resourcemanager/server/config.go#L225) | RequestUnitConfig.ReadPerBatchBaseCost float64 &#96;toml:"read-per-batch-base-cost" json:"read-per-batch-base-cost"&#96; | L249: configutil.AdjustFloat64(&ruc.ReadPerBatchBaseCost, defaultReadPerBatchBaseCost) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-049 [pkg/mcs/resourcemanager/server/config.go:227](../../../pkg/mcs/resourcemanager/server/config.go#L227) | RequestUnitConfig.ReadCostPerByte float64 &#96;toml:"read-cost-per-byte" json:"read-cost-per-byte"&#96; | L252: configutil.AdjustFloat64(&ruc.ReadCostPerByte, defaultReadCostPerByte) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-050 [pkg/mcs/resourcemanager/server/config.go:230](../../../pkg/mcs/resourcemanager/server/config.go#L230) | RequestUnitConfig.WriteBaseCost float64 &#96;toml:"write-base-cost" json:"write-base-cost"&#96; | L255: configutil.AdjustFloat64(&ruc.WriteBaseCost, defaultWriteBaseCost) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-051 [pkg/mcs/resourcemanager/server/config.go:232](../../../pkg/mcs/resourcemanager/server/config.go#L232) | RequestUnitConfig.WritePerBatchBaseCost float64 &#96;toml:"write-per-batch-base-cost" json:"write-per-batch-base-cost"&#96; | L258: configutil.AdjustFloat64(&ruc.WritePerBatchBaseCost, defaultWritePerBatchBaseCost) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-052 [pkg/mcs/resourcemanager/server/config.go:234](../../../pkg/mcs/resourcemanager/server/config.go#L234) | RequestUnitConfig.WriteCostPerByte float64 &#96;toml:"write-cost-per-byte" json:"write-cost-per-byte"&#96; | L261: configutil.AdjustFloat64(&ruc.WriteCostPerByte, defaultWriteCostPerByte) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-053 [pkg/mcs/resourcemanager/server/config.go:237](../../../pkg/mcs/resourcemanager/server/config.go#L237) | RequestUnitConfig.CPUMsCost float64 &#96;toml:"read-cpu-ms-cost" json:"read-cpu-ms-cost"&#96; | L264: configutil.AdjustFloat64(&ruc.CPUMsCost, defaultCPUMsCost) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 主题8；78/102边界 |
| C-054 [pkg/schedule/config/config.go:189](../../../pkg/schedule/config/config.go#L189) | ScheduleConfig.MaxSnapshotCount    uint64 &#96;toml:"max-snapshot-count" json:"max-snapshot-count"&#96; | L369: configutil.AdjustUint64(&c.MaxSnapshotCount, defaultMaxSnapshotCount) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-055 [pkg/schedule/config/config.go:190](../../../pkg/schedule/config/config.go#L190) | ScheduleConfig.MaxPendingPeerCount uint64 &#96;toml:"max-pending-peer-count" json:"max-pending-peer-count"&#96; | L372: configutil.AdjustUint64(&c.MaxPendingPeerCount, defaultMaxPendingPeerCount) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-056 [pkg/schedule/config/config.go:194](../../../pkg/schedule/config/config.go#L194) | ScheduleConfig.MaxMergeRegionSize uint64 &#96;toml:"max-merge-region-size" json:"max-merge-region-size"&#96; | L375: configutil.AdjustUint64(&c.MaxMergeRegionSize, defaultMaxMergeRegionSize) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-057 [pkg/schedule/config/config.go:195](../../../pkg/schedule/config/config.go#L195) | ScheduleConfig.MaxMergeRegionKeys uint64 &#96;toml:"max-merge-region-keys" json:"max-merge-region-keys"&#96; | L563: if keys := c.MaxMergeRegionKeys; keys != 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-058 [pkg/schedule/config/config.go:197](../../../pkg/schedule/config/config.go#L197) | ScheduleConfig.SplitMergeInterval typeutil.Duration &#96;toml:"split-merge-interval" json:"split-merge-interval"&#96; | L380: configutil.AdjustDuration(&c.SplitMergeInterval, DefaultSplitMergeInterval) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-059 [pkg/schedule/config/config.go:199](../../../pkg/schedule/config/config.go#L199) | ScheduleConfig.SwitchWitnessInterval typeutil.Duration &#96;toml:"switch-witness-interval" json:"switch-witness-interval"&#96; | L381: configutil.AdjustDuration(&c.SwitchWitnessInterval, defaultSwitchWitnessInterval) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-060 [pkg/schedule/config/config.go:201](../../../pkg/schedule/config/config.go#L201) | ScheduleConfig.EnableOneWayMerge bool &#96;toml:"enable-one-way-merge" json:"enable-one-way-merge,string"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-061 [pkg/schedule/config/config.go:204](../../../pkg/schedule/config/config.go#L204) | ScheduleConfig.EnableCrossTableMerge bool &#96;toml:"enable-cross-table-merge" json:"enable-cross-table-merge,string"&#96; | L448: c.EnableCrossTableMerge = defaultEnableCrossTableMerge | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-062 [pkg/schedule/config/config.go:206](../../../pkg/schedule/config/config.go#L206) | ScheduleConfig.PatrolRegionInterval typeutil.Duration &#96;toml:"patrol-region-interval" json:"patrol-region-interval"&#96; | L382: configutil.AdjustDuration(&c.PatrolRegionInterval, defaultPatrolRegionInterval) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-063 [pkg/schedule/config/config.go:209](../../../pkg/schedule/config/config.go#L209) | ScheduleConfig.MaxStoreDownTime typeutil.Duration &#96;toml:"max-store-down-time" json:"max-store-down-time"&#96; | L383: configutil.AdjustDuration(&c.MaxStoreDownTime, defaultMaxStoreDownTime) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-064 [pkg/schedule/config/config.go:212](../../../pkg/schedule/config/config.go#L212) | ScheduleConfig.MaxStorePreparingTime typeutil.Duration &#96;toml:"max-store-preparing-time" json:"max-store-preparing-time"&#96; | L386: configutil.AdjustDuration(&c.MaxStorePreparingTime, defaultMaxStorePreparingTime) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-065 [pkg/schedule/config/config.go:214](../../../pkg/schedule/config/config.go#L214) | ScheduleConfig.LeaderScheduleLimit uint64 &#96;toml:"leader-schedule-limit" json:"leader-schedule-limit"&#96; | L390: configutil.AdjustUint64(&c.LeaderScheduleLimit, defaultLeaderScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-066 [pkg/schedule/config/config.go:216](../../../pkg/schedule/config/config.go#L216) | ScheduleConfig.LeaderSchedulePolicy string &#96;toml:"leader-schedule-policy" json:"leader-schedule-policy"&#96; | L423: configutil.AdjustString(&c.LeaderSchedulePolicy, defaultLeaderSchedulePolicy) ; L701: if c.LeaderSchedulePolicy != "count" && c.LeaderSchedulePolicy != "size" { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-067 [pkg/schedule/config/config.go:218](../../../pkg/schedule/config/config.go#L218) | ScheduleConfig.RegionScheduleLimit uint64 &#96;toml:"region-schedule-limit" json:"region-schedule-limit"&#96; | L393: configutil.AdjustUint64(&c.RegionScheduleLimit, defaultRegionScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-068 [pkg/schedule/config/config.go:220](../../../pkg/schedule/config/config.go#L220) | ScheduleConfig.WitnessScheduleLimit uint64 &#96;toml:"witness-schedule-limit" json:"witness-schedule-limit"&#96; | L396: configutil.AdjustUint64(&c.WitnessScheduleLimit, defaultWitnessScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-069 [pkg/schedule/config/config.go:222](../../../pkg/schedule/config/config.go#L222) | ScheduleConfig.ReplicaScheduleLimit uint64 &#96;toml:"replica-schedule-limit" json:"replica-schedule-limit"&#96; | L399: configutil.AdjustUint64(&c.ReplicaScheduleLimit, defaultReplicaScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-070 [pkg/schedule/config/config.go:224](../../../pkg/schedule/config/config.go#L224) | ScheduleConfig.MergeScheduleLimit uint64 &#96;toml:"merge-schedule-limit" json:"merge-schedule-limit"&#96; | L402: configutil.AdjustUint64(&c.MergeScheduleLimit, defaultMergeScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-071 [pkg/schedule/config/config.go:226](../../../pkg/schedule/config/config.go#L226) | ScheduleConfig.HotRegionScheduleLimit uint64 &#96;toml:"hot-region-schedule-limit" json:"hot-region-schedule-limit"&#96; | L405: configutil.AdjustUint64(&c.HotRegionScheduleLimit, defaultHotRegionScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-072 [pkg/schedule/config/config.go:228](../../../pkg/schedule/config/config.go#L228) | ScheduleConfig.AffinityScheduleLimit uint64 &#96;toml:"affinity-schedule-limit" json:"affinity-schedule-limit"&#96; | L408: configutil.AdjustUint64(&c.AffinityScheduleLimit, defaultAffinityScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-073 [pkg/schedule/config/config.go:230](../../../pkg/schedule/config/config.go#L230) | ScheduleConfig.SplitScatterScheduleLimit uint64 &#96;toml:"split-scatter-schedule-limit" json:"split-scatter-schedule-limit"&#96; | L411: configutil.AdjustUint64(&c.SplitScatterScheduleLimit, defaultSplitScatterScheduleLimit) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-074 [pkg/schedule/config/config.go:234](../../../pkg/schedule/config/config.go#L234) | ScheduleConfig.HotRegionCacheHitsThreshold uint64 &#96;toml:"hot-region-cache-hits-threshold" json:"hot-region-cache-hits-threshold"&#96; | L414: configutil.AdjustUint64(&c.HotRegionCacheHitsThreshold, defaultHotRegionCacheHitsThreshold) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-075 [pkg/schedule/config/config.go:237](../../../pkg/schedule/config/config.go#L237) | ScheduleConfig.StoreBalanceRate float64 &#96;toml:"store-balance-rate" json:"store-balance-rate,omitempty"&#96; | L514: if c.StoreBalanceRate == 0 { ; L517: defaultStoreLimit := StoreLimitConfig{AddPeer: c.StoreBalanceRate, RemovePeer: c.StoreBalanceRate} ; L526: c.StoreBalanceRate = 0 ; L531: if c.StoreBalanceRate != 0 { ; L532: defaultStoreLimit.AddPeer = c.StoreBalanceRate ; L533: defaultStoreLimit.RemovePeer = c.StoreBalanceRate ; L547: c.StoreBalanceRate = 0 ; L737: if c.StoreBalanceRate != 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-076 [pkg/schedule/config/config.go:239](../../../pkg/schedule/config/config.go#L239) | ScheduleConfig.DefaultStoreLimit StoreLimitConfig &#96;toml:"default-store-limit" json:"default-store-limit"&#96; | L503: configutil.AdjustFloat64(&c.DefaultStoreLimit.AddPeer, defaultStoreLimit.AddPeer) ; L506: configutil.AdjustFloat64(&c.DefaultStoreLimit.RemovePeer, defaultStoreLimit.RemovePeer) ; L509: configutil.AdjustFloat64(&c.DefaultStoreLimit.TransferLeaderIn, defaultStoreLimit.TransferLeaderIn) ; L519: c.DefaultStoreLimit.AddPeer = defaultStoreLimit.AddPeer ; L522: c.DefaultStoreLimit.RemovePeer = defaultStoreLimit.RemovePeer ; L536: c.DefaultStoreLimit.AddPeer = defaultStoreLimit.AddPeer ; L539: c.DefaultStoreLimit.RemovePeer = defaultStoreLimit.RemovePeer ; L542: c.DefaultStoreLimit.TransferLeaderIn = defaultStoreLimit.TransferLeaderIn ; L611: limit = c.DefaultStoreLimit ; L641: c.applyDeprecatedFlagMigration(fields.DefaultStoreLimit.AddPeer != nil, ; L642: fields.DefaultStoreLimit.RemovePeer != nil, fields.DefaultStoreLimit.TransferLeaderIn != nil) ; L647: limit := StoreLimitConfig{TransferLeaderIn: c.DefaultStoreLimit.TransferLeaderIn} | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-077 [pkg/schedule/config/config.go:241](../../../pkg/schedule/config/config.go#L241) | ScheduleConfig.StoreLimit map[uint64]StoreLimitConfig &#96;toml:"store-limit" json:"store-limit"&#96; | L354: if c.StoreLimit != nil { ; L355: storeLimit = make(map[uint64]StoreLimitConfig, len(c.StoreLimit)) ; L356: for k, v := range c.StoreLimit { ; L361: cfg.StoreLimit = storeLimit ; L482: if c.StoreLimit == nil { ; L483: c.StoreLimit = make(map[uint64]StoreLimitConfig) ; L605: if c.StoreLimit == nil && fields.StoreLimit != nil { ; L606: c.StoreLimit = make(map[uint64]StoreLimitConfig, len(fields.StoreLimit)) ; L608: for storeID, data := range fields.StoreLimit { ; L609: limit, ok := c.StoreLimit[storeID] ; L617: c.StoreLimit[storeID] = limit ; L644: for storeID, data := range fields.StoreLimit { ; L651: c.StoreLimit[storeID] = limit ; L678: for storeID, limit := range c.StoreLimit { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-078 [pkg/schedule/config/config.go:243](../../../pkg/schedule/config/config.go#L243) | ScheduleConfig.TolerantSizeRatio float64 &#96;toml:"tolerant-size-ratio" json:"tolerant-size-ratio"&#96; | L417: configutil.AdjustFloat64(&c.TolerantSizeRatio, defaultTolerantSizeRatio) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-079 [pkg/schedule/config/config.go:252](../../../pkg/schedule/config/config.go#L252) | ScheduleConfig.LowSpaceRatio float64 &#96;toml:"low-space-ratio" json:"low-space-ratio"&#96; | L450: configutil.AdjustFloat64(&c.LowSpaceRatio, defaultLowSpaceRatio) ; L698: if c.LowSpaceRatio <= c.HighSpaceRatio { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-080 [pkg/schedule/config/config.go:255](../../../pkg/schedule/config/config.go#L255) | ScheduleConfig.HighSpaceRatio float64 &#96;toml:"high-space-ratio" json:"high-space-ratio"&#96; | L451: configutil.AdjustFloat64(&c.HighSpaceRatio, defaultHighSpaceRatio) ; L698: if c.LowSpaceRatio <= c.HighSpaceRatio { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-081 [pkg/schedule/config/config.go:257](../../../pkg/schedule/config/config.go#L257) | ScheduleConfig.RegionScoreFormulaVersion string &#96;toml:"region-score-formula-version" json:"region-score-formula-version"&#96; | L462: configutil.AdjustString(&c.RegionScoreFormulaVersion, defaultRegionScoreFormulaVersion) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-082 [pkg/schedule/config/config.go:259](../../../pkg/schedule/config/config.go#L259) | ScheduleConfig.SchedulerMaxWaitingOperator uint64 &#96;toml:"scheduler-max-waiting-operator" json:"scheduler-max-waiting-operator"&#96; | L420: configutil.AdjustUint64(&c.SchedulerMaxWaitingOperator, defaultSchedulerMaxWaitingOperator) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-083 [pkg/schedule/config/config.go:262](../../../pkg/schedule/config/config.go#L262) | ScheduleConfig.DisableLearner bool &#96;toml:"disable-raft-learner" json:"disable-raft-learner,string,omitempty"&#96; | L657: c.DisableLearner = false | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-084 [pkg/schedule/config/config.go:266](../../../pkg/schedule/config/config.go#L266) | ScheduleConfig.DisableRemoveDownReplica bool &#96;toml:"disable-remove-down-replica" json:"disable-remove-down-replica,string,omitempty"&#96; | L552: "remove-down-replica":     {&c.DisableRemoveDownReplica, &c.EnableRemoveDownReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-085 [pkg/schedule/config/config.go:270](../../../pkg/schedule/config/config.go#L270) | ScheduleConfig.DisableReplaceOfflineReplica bool &#96;toml:"disable-replace-offline-replica" json:"disable-replace-offline-replica,string,omitempty"&#96; | L553: "replace-offline-replica": {&c.DisableReplaceOfflineReplica, &c.EnableReplaceOfflineReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-086 [pkg/schedule/config/config.go:274](../../../pkg/schedule/config/config.go#L274) | ScheduleConfig.DisableMakeUpReplica bool &#96;toml:"disable-make-up-replica" json:"disable-make-up-replica,string,omitempty"&#96; | L554: "make-up-replica":         {&c.DisableMakeUpReplica, &c.EnableMakeUpReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-087 [pkg/schedule/config/config.go:278](../../../pkg/schedule/config/config.go#L278) | ScheduleConfig.DisableRemoveExtraReplica bool &#96;toml:"disable-remove-extra-replica" json:"disable-remove-extra-replica,string,omitempty"&#96; | L555: "remove-extra-replica":    {&c.DisableRemoveExtraReplica, &c.EnableRemoveExtraReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-088 [pkg/schedule/config/config.go:282](../../../pkg/schedule/config/config.go#L282) | ScheduleConfig.DisableLocationReplacement bool &#96;toml:"disable-location-replacement" json:"disable-location-replacement,string,omitempty"&#96; | L556: "location-replacement":    {&c.DisableLocationReplacement, &c.EnableLocationReplacement}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-089 [pkg/schedule/config/config.go:285](../../../pkg/schedule/config/config.go#L285) | ScheduleConfig.EnableRemoveDownReplica bool &#96;toml:"enable-remove-down-replica" json:"enable-remove-down-replica,string"&#96; | L552: "remove-down-replica":     {&c.DisableRemoveDownReplica, &c.EnableRemoveDownReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-090 [pkg/schedule/config/config.go:287](../../../pkg/schedule/config/config.go#L287) | ScheduleConfig.EnableReplaceOfflineReplica bool &#96;toml:"enable-replace-offline-replica" json:"enable-replace-offline-replica,string"&#96; | L553: "replace-offline-replica": {&c.DisableReplaceOfflineReplica, &c.EnableReplaceOfflineReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-091 [pkg/schedule/config/config.go:289](../../../pkg/schedule/config/config.go#L289) | ScheduleConfig.EnableMakeUpReplica bool &#96;toml:"enable-make-up-replica" json:"enable-make-up-replica,string"&#96; | L554: "make-up-replica":         {&c.DisableMakeUpReplica, &c.EnableMakeUpReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-092 [pkg/schedule/config/config.go:291](../../../pkg/schedule/config/config.go#L291) | ScheduleConfig.EnableRemoveExtraReplica bool &#96;toml:"enable-remove-extra-replica" json:"enable-remove-extra-replica,string"&#96; | L555: "remove-extra-replica":    {&c.DisableRemoveExtraReplica, &c.EnableRemoveExtraReplica}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-093 [pkg/schedule/config/config.go:293](../../../pkg/schedule/config/config.go#L293) | ScheduleConfig.EnableLocationReplacement bool &#96;toml:"enable-location-replacement" json:"enable-location-replacement,string"&#96; | L556: "location-replacement":    {&c.DisableLocationReplacement, &c.EnableLocationReplacement}, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-094 [pkg/schedule/config/config.go:295](../../../pkg/schedule/config/config.go#L295) | ScheduleConfig.EnableDebugMetrics bool &#96;toml:"enable-debug-metrics" json:"enable-debug-metrics,string"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-095 [pkg/schedule/config/config.go:297](../../../pkg/schedule/config/config.go#L297) | ScheduleConfig.EnableJointConsensus bool &#96;toml:"enable-joint-consensus" json:"enable-joint-consensus,string"&#96; | L433: c.EnableJointConsensus = defaultEnableJointConsensus | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-096 [pkg/schedule/config/config.go:300](../../../pkg/schedule/config/config.go#L300) | ScheduleConfig.EnableTiKVSplitRegion bool &#96;toml:"enable-tikv-split-region" json:"enable-tikv-split-region,string"&#96; | L436: c.EnableTiKVSplitRegion = defaultEnableTiKVSplitRegion | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-097 [pkg/schedule/config/config.go:303](../../../pkg/schedule/config/config.go#L303) | ScheduleConfig.EnableHeartbeatBreakdownMetrics bool &#96;toml:"enable-heartbeat-breakdown-metrics" json:"enable-heartbeat-breakdown-metrics,string"&#96; | L440: c.EnableHeartbeatBreakdownMetrics = defaultEnableHeartbeatBreakdownMetrics | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-098 [pkg/schedule/config/config.go:306](../../../pkg/schedule/config/config.go#L306) | ScheduleConfig.EnableHeartbeatConcurrentRunner bool &#96;toml:"enable-heartbeat-concurrent-runner" json:"enable-heartbeat-concurrent-runner,string"&#96; | L444: c.EnableHeartbeatConcurrentRunner = defaultEnableHeartbeatConcurrentRunner | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-099 [pkg/schedule/config/config.go:309](../../../pkg/schedule/config/config.go#L309) | ScheduleConfig.Schedulers SchedulerConfigs &#96;toml:"schedulers" json:"schedulers-v2"&#96; // json v2 is for the sake of compatible upgrade | L352: schedulers := append(c.Schedulers[:0:0], c.Schedulers...) ; L362: cfg.Schedulers = schedulers ; L469: adjustSchedulers(&c.Schedulers, DefaultSchedulers) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-100 [pkg/schedule/config/config.go:312](../../../pkg/schedule/config/config.go#L312) | ScheduleConfig.HotRegionsWriteInterval typeutil.Duration &#96;toml:"hot-regions-write-interval" json:"hot-regions-write-interval"&#96; | L384: configutil.AdjustDuration(&c.HotRegionsWriteInterval, defaultHotRegionsWriteInterval) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-101 [pkg/schedule/config/config.go:315](../../../pkg/schedule/config/config.go#L315) | ScheduleConfig.HotRegionsReservedDays uint64 &#96;toml:"hot-regions-reserved-days" json:"hot-regions-reserved-days"&#96; | L487: configutil.AdjustUint64(&c.HotRegionsReservedDays, defaultHotRegionsReservedDays) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-102 [pkg/schedule/config/config.go:319](../../../pkg/schedule/config/config.go#L319) | ScheduleConfig.MaxMovableHotPeerSize int64 &#96;toml:"max-movable-hot-peer-size" json:"max-movable-hot-peer-size,omitempty"&#96; | L491: configutil.AdjustInt64(&c.MaxMovableHotPeerSize, defaultMaxMovableHotPeerSize) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-103 [pkg/schedule/config/config.go:322](../../../pkg/schedule/config/config.go#L322) | ScheduleConfig.EnableDiagnostic bool &#96;toml:"enable-diagnostic" json:"enable-diagnostic,string"&#96; | L453: c.EnableDiagnostic = defaultEnableDiagnostic | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-104 [pkg/schedule/config/config.go:325](../../../pkg/schedule/config/config.go#L325) | ScheduleConfig.EnableWitness bool &#96;toml:"enable-witness" json:"enable-witness,string"&#96; | L457: c.EnableWitness = defaultEnableWitness | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-105 [pkg/schedule/config/config.go:329](../../../pkg/schedule/config/config.go#L329) | ScheduleConfig.SlowStoreEvictingAffectedStoreRatioThreshold float64 &#96;toml:"slow-store-evicting-affected-store-ratio-threshold" json:"slow-store-evicting-affected-store-ratio-threshold,omitempty"&#96; | L495: configutil.AdjustFloat64(&c.SlowStoreEvictingAffectedStoreRatioThreshold, defaultSlowStoreEvictingAffectedStoreRatioThreshold) ; L704: if c.SlowStoreEvictingAffectedStoreRatioThreshold == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-106 [pkg/schedule/config/config.go:334](../../../pkg/schedule/config/config.go#L334) | ScheduleConfig.StoreLimitVersion string &#96;toml:"store-limit-version" json:"store-limit-version,omitempty"&#96; | L426: configutil.AdjustString(&c.StoreLimitVersion, defaultStoreLimitVersion) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-107 [pkg/schedule/config/config.go:338](../../../pkg/schedule/config/config.go#L338) | ScheduleConfig.HaltScheduling bool &#96;toml:"halt-scheduling" json:"halt-scheduling,string,omitempty"&#96; | L466: c.HaltScheduling = defaultHaltScheduling | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-108 [pkg/schedule/config/config.go:341](../../../pkg/schedule/config/config.go#L341) | ScheduleConfig.PatrolRegionWorkerCount int &#96;toml:"patrol-region-worker-count" json:"patrol-region-worker-count"&#96; | L429: configutil.AdjustInt(&c.PatrolRegionWorkerCount, defaultPatrolRegionWorkerCount) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-109 [pkg/schedule/config/config.go:347](../../../pkg/schedule/config/config.go#L347) | ScheduleConfig.MaxAffinityMergeRegionSize uint64 &#96;toml:"max-affinity-merge-region-size" json:"max-affinity-merge-region-size"&#96; | L378: configutil.AdjustUint64(&c.MaxAffinityMergeRegionSize, defaultMaxAffinityMergeRegionSize) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-110 [pkg/schedule/config/config.go:745](../../../pkg/schedule/config/config.go#L745) | StoreLimitConfig.AddPeer    float64 &#96;toml:"add-peer" json:"add-peer"&#96; | L101: DefaultStoreLimit = StoreLimit{AddPeer: 15, RemovePeer: 15, TransferLeaderIn: storelimit.Unlimited} ; L103: DefaultTiFlashStoreLimit = StoreLimit{AddPeer: 30, RemovePeer: 30, TransferLeaderIn: storelimit.Unlimited} ; L150: case storelimit.AddPeer: ; L151: sl.AddPeer = ratePerMin ; L164: case storelimit.AddPeer: ; L503: configutil.AdjustFloat64(&c.DefaultStoreLimit.AddPeer, defaultStoreLimit.AddPeer) ; L517: defaultStoreLimit := StoreLimitConfig{AddPeer: c.StoreBalanceRate, RemovePeer: c.StoreBalanceRate} ; L519: c.DefaultStoreLimit.AddPeer = defaultStoreLimit.AddPeer ; L532: defaultStoreLimit.AddPeer = c.StoreBalanceRate ; L536: c.DefaultStoreLimit.AddPeer = defaultStoreLimit.AddPeer ; L641: c.applyDeprecatedFlagMigration(fields.DefaultStoreLimit.AddPeer != nil, ; L755: case storelimit.AddPeer: ; L756: c.AddPeer = ratePerMin ; L768: AddPeer:          DefaultStoreLimit.GetDefaultStoreLimit(storelimit.AddPeer), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-111 [pkg/schedule/config/config.go:746](../../../pkg/schedule/config/config.go#L746) | StoreLimitConfig.RemovePeer float64 &#96;toml:"remove-peer" json:"remove-peer"&#96; | L101: DefaultStoreLimit = StoreLimit{AddPeer: 15, RemovePeer: 15, TransferLeaderIn: storelimit.Unlimited} ; L103: DefaultTiFlashStoreLimit = StoreLimit{AddPeer: 30, RemovePeer: 30, TransferLeaderIn: storelimit.Unlimited} ; L152: case storelimit.RemovePeer: ; L153: sl.RemovePeer = ratePerMin ; L166: case storelimit.RemovePeer: ; L506: configutil.AdjustFloat64(&c.DefaultStoreLimit.RemovePeer, defaultStoreLimit.RemovePeer) ; L517: defaultStoreLimit := StoreLimitConfig{AddPeer: c.StoreBalanceRate, RemovePeer: c.StoreBalanceRate} ; L522: c.DefaultStoreLimit.RemovePeer = defaultStoreLimit.RemovePeer ; L533: defaultStoreLimit.RemovePeer = c.StoreBalanceRate ; L539: c.DefaultStoreLimit.RemovePeer = defaultStoreLimit.RemovePeer ; L642: fields.DefaultStoreLimit.RemovePeer != nil, fields.DefaultStoreLimit.TransferLeaderIn != nil) ; L757: case storelimit.RemovePeer: ; L758: c.RemovePeer = ratePerMin ; L769: RemovePeer:       DefaultStoreLimit.GetDefaultStoreLimit(storelimit.RemovePeer), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-112 [pkg/schedule/config/config.go:749](../../../pkg/schedule/config/config.go#L749) | StoreLimitConfig.TransferLeaderIn float64 &#96;toml:"transfer-leader-in" json:"transfer-leader-in"&#96; | L101: DefaultStoreLimit = StoreLimit{AddPeer: 15, RemovePeer: 15, TransferLeaderIn: storelimit.Unlimited} ; L103: DefaultTiFlashStoreLimit = StoreLimit{AddPeer: 30, RemovePeer: 30, TransferLeaderIn: storelimit.Unlimited} ; L154: case storelimit.TransferLeaderIn: ; L155: sl.TransferLeaderIn = ratePerMin ; L168: case storelimit.TransferLeaderIn: ; L509: configutil.AdjustFloat64(&c.DefaultStoreLimit.TransferLeaderIn, defaultStoreLimit.TransferLeaderIn) ; L542: c.DefaultStoreLimit.TransferLeaderIn = defaultStoreLimit.TransferLeaderIn ; L642: fields.DefaultStoreLimit.RemovePeer != nil, fields.DefaultStoreLimit.TransferLeaderIn != nil) ; L647: limit := StoreLimitConfig{TransferLeaderIn: c.DefaultStoreLimit.TransferLeaderIn} ; L759: case storelimit.TransferLeaderIn: ; L760: c.TransferLeaderIn = ratePerMin ; L770: TransferLeaderIn: DefaultStoreLimit.GetDefaultStoreLimit(storelimit.TransferLeaderIn), | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-113 [pkg/schedule/config/config.go:784](../../../pkg/schedule/config/config.go#L784) | SchedulerConfig.Type        string   &#96;toml:"type" json:"type"&#96; | L796: {Type: types.SchedulerTypeCompatibleMap[types.BalanceRegionScheduler]}, ; L797: {Type: types.SchedulerTypeCompatibleMap[types.BalanceLeaderScheduler]}, ; L798: {Type: types.SchedulerTypeCompatibleMap[types.BalanceHotRegionScheduler]}, ; L799: {Type: types.SchedulerTypeCompatibleMap[types.EvictSlowStoreScheduler]}, ; L802: defaultSchedulers = append(defaultSchedulers, SchedulerConfig{Type: types.SchedulerTypeCompatibleMap[types.EvictStoppingStoreScheduler]}) ; L810: if typ == c.Type { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-114 [pkg/schedule/config/config.go:785](../../../pkg/schedule/config/config.go#L785) | SchedulerConfig.Args        []string &#96;toml:"args" json:"args"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-115 [pkg/schedule/config/config.go:786](../../../pkg/schedule/config/config.go#L786) | SchedulerConfig.Disable     bool     &#96;toml:"disable" json:"disable"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-116 [pkg/schedule/config/config.go:787](../../../pkg/schedule/config/config.go#L787) | SchedulerConfig.ArgsPayload string   &#96;toml:"args-payload" json:"args-payload"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-117 [pkg/schedule/config/config.go:821](../../../pkg/schedule/config/config.go#L821) | ReplicationConfig.MaxReplicas uint64 &#96;toml:"max-replicas" json:"max-replicas"&#96; | L877: configutil.AdjustUint64(&c.MaxReplicas, DefaultMaxReplicas) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 49 |
| C-118 [pkg/schedule/config/config.go:827](../../../pkg/schedule/config/config.go#L827) | ReplicationConfig.LocationLabels typeutil.StringSlice &#96;toml:"location-labels" json:"location-labels"&#96; | L850: locationLabels := append(c.LocationLabels[:0:0], c.LocationLabels...) ; L852: cfg.LocationLabels = locationLabels ; L859: for _, label := range c.LocationLabels { ; L885: c.LocationLabels = defaultLocationLabels | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 49 |
| C-119 [pkg/schedule/config/config.go:829](../../../pkg/schedule/config/config.go#L829) | ReplicationConfig.StrictlyMatchLabel bool &#96;toml:"strictly-match-label" json:"strictly-match-label,string"&#96; | L882: c.StrictlyMatchLabel = defaultStrictlyMatchLabel | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 49 |
| C-120 [pkg/schedule/config/config.go:832](../../../pkg/schedule/config/config.go#L832) | ReplicationConfig.EnablePlacementRules bool &#96;toml:"enable-placement-rules" json:"enable-placement-rules,string"&#96; | L879: c.EnablePlacementRules = defaultEnablePlacementRules | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 49 |
| C-121 [pkg/schedule/config/config.go:835](../../../pkg/schedule/config/config.go#L835) | ReplicationConfig.EnablePlacementRulesCache bool &#96;toml:"enable-placement-rules-cache" json:"enable-placement-rules-cache,string"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 49 |
| C-122 [pkg/schedule/config/config.go:845](../../../pkg/schedule/config/config.go#L845) | ReplicationConfig.IsolationLevel string &#96;toml:"isolation-level" json:"isolation-level"&#96; | L865: if !foundIsolationLevel && label == c.IsolationLevel { ; L869: if c.IsolationLevel != "" && !foundIsolationLevel { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 49 |
| C-123 [pkg/schedule/config/store_config.go:47](../../../pkg/schedule/config/store_config.go#L47) | StoreConfig.Coprocessor &#96;json:"coprocessor"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-124 [pkg/schedule/config/store_config.go:48](../../../pkg/schedule/config/store_config.go#L48) | StoreConfig.Storage     &#96;json:"storage"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-125 [pkg/schedule/config/store_config.go:49](../../../pkg/schedule/config/store_config.go#L49) | StoreConfig.ReadPool    ReadPool &#96;json:"readpool"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-126 [pkg/schedule/config/store_config.go:51](../../../pkg/schedule/config/store_config.go#L51) | StoreConfig.RegionMaxSizeMB    uint64 &#96;json:"-"&#96; | L90: c.RegionMaxSizeMB = typeutil.ParseMBFromText(c.RegionMaxSize, defaultRegionMaxSize) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-127 [pkg/schedule/config/store_config.go:52](../../../pkg/schedule/config/store_config.go#L52) | StoreConfig.RegionSplitSizeMB  uint64 &#96;json:"-"&#96; | L91: c.RegionSplitSizeMB = typeutil.ParseMBFromText(c.RegionSplitSize, defaultRegionSplitSize) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-128 [pkg/schedule/config/store_config.go:53](../../../pkg/schedule/config/store_config.go#L53) | StoreConfig.RegionBucketSizeMB uint64 &#96;json:"-"&#96; | L92: c.RegionBucketSizeMB = typeutil.ParseMBFromText(c.RegionBucketSize, defaultBucketSize) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-129 [pkg/schedule/config/store_config.go:58](../../../pkg/schedule/config/store_config.go#L58) | Storage.Engine string &#96;json:"engine"&#96; | L156: return c.Engine == RaftstoreV2 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-130 [pkg/schedule/config/store_config.go:63](../../../pkg/schedule/config/store_config.go#L63) | ReadPool.Unified UnifiedReadPool &#96;json:"unified"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-131 [pkg/schedule/config/store_config.go:68](../../../pkg/schedule/config/store_config.go#L68) | UnifiedReadPool.MaxThreadCount uint64 &#96;json:"max-thread-count"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-132 [pkg/schedule/config/store_config.go:75](../../../pkg/schedule/config/store_config.go#L75) | Coprocessor.RegionMaxSize string &#96;json:"region-max-size"&#96; | L90: c.RegionMaxSizeMB = typeutil.ParseMBFromText(c.RegionMaxSize, defaultRegionMaxSize) ; L113: if c == nil \|\| len(c.RegionMaxSize) == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-133 [pkg/schedule/config/store_config.go:77](../../../pkg/schedule/config/store_config.go#L77) | Coprocessor.RegionSplitSize    string &#96;json:"region-split-size"&#96; | L91: c.RegionSplitSizeMB = typeutil.ParseMBFromText(c.RegionSplitSize, defaultRegionSplitSize) ; L121: if c == nil \|\| len(c.RegionSplitSize) == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-134 [pkg/schedule/config/store_config.go:78](../../../pkg/schedule/config/store_config.go#L78) | Coprocessor.RegionMaxKeys      int    &#96;json:"region-max-keys"&#96; | L137: if c == nil \|\| c.RegionMaxKeys == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-135 [pkg/schedule/config/store_config.go:79](../../../pkg/schedule/config/store_config.go#L79) | Coprocessor.RegionSplitKeys    int    &#96;json:"region-split-keys"&#96; | L129: if c == nil \|\| c.RegionSplitKeys == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-136 [pkg/schedule/config/store_config.go:80](../../../pkg/schedule/config/store_config.go#L80) | Coprocessor.EnableRegionBucket bool   &#96;json:"enable-region-bucket"&#96; | L172: c.EnableRegionBucket = enabled ; L177: if c == nil \|\| !c.EnableRegionBucket { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-137 [pkg/schedule/config/store_config.go:81](../../../pkg/schedule/config/store_config.go#L81) | Coprocessor.RegionBucketSize   string &#96;json:"region-bucket-size"&#96; | L92: c.RegionBucketSizeMB = typeutil.ParseMBFromText(c.RegionBucketSize, defaultBucketSize) ; L180: if len(c.RegionBucketSize) == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 48 |
| C-138 [pkg/utils/configutil/configutil.go:89](../../../pkg/utils/configutil/configutil.go#L89) | SecurityConfig.RedactInfoLog logutil.RedactInfoLogType &#96;toml:"redact-info-log" json:"redact-info-log"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 90/91/92 |
| C-139 [pkg/utils/configutil/configutil.go:90](../../../pkg/utils/configutil/configutil.go#L90) | SecurityConfig.Encryption    encryption.Config         &#96;toml:"encryption" json:"encryption"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 90/91/92 |
| C-140 [pkg/utils/grpcutil/grpcutil.go:52](../../../pkg/utils/grpcutil/grpcutil.go#L52) | TLSConfig.CAPath string &#96;toml:"cacert-path" json:"cacert-path"&#96; | L74: TrustedCAFile: s.CAPath, ; L124: TrustedCAFile: s.CAPath, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 90/91/92 |
| C-141 [pkg/utils/grpcutil/grpcutil.go:54](../../../pkg/utils/grpcutil/grpcutil.go#L54) | TLSConfig.CertPath string &#96;toml:"cert-path" json:"cert-path"&#96; | L67: if len(s.CertPath) == 0 && len(s.KeyPath) == 0 { ; L72: CertFile:      s.CertPath, ; L117: if len(s.CertPath) == 0 && len(s.KeyPath) == 0 { ; L122: CertFile:      s.CertPath, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 90/91/92 |
| C-142 [pkg/utils/grpcutil/grpcutil.go:56](../../../pkg/utils/grpcutil/grpcutil.go#L56) | TLSConfig.KeyPath string &#96;toml:"key-path" json:"key-path"&#96; | L67: if len(s.CertPath) == 0 && len(s.KeyPath) == 0 { ; L73: KeyFile:       s.KeyPath, ; L117: if len(s.CertPath) == 0 && len(s.KeyPath) == 0 { ; L123: KeyFile:       s.KeyPath, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 90/91/92 |
| C-143 [pkg/utils/grpcutil/grpcutil.go:58](../../../pkg/utils/grpcutil/grpcutil.go#L58) | TLSConfig.CertAllowedCNs []string &#96;toml:"cert-allowed-cn" json:"cert-allowed-cn"&#96; | L75: AllowedCNs:    s.CertAllowedCNs, ; L125: AllowedCNs:    s.CertAllowedCNs, | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 90/91/92 |
| C-144 [pkg/utils/metricutil/metricutil.go:41](../../../pkg/utils/metricutil/metricutil.go#L41) | MetricConfig.PushJob      string            &#96;toml:"job" json:"job"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 15/86/101 |
| C-145 [pkg/utils/metricutil/metricutil.go:42](../../../pkg/utils/metricutil/metricutil.go#L42) | MetricConfig.PushAddress  string            &#96;toml:"address" json:"address"&#96; | L97: if cfg.PushInterval.Duration == zeroDuration \|\| len(cfg.PushAddress) == 0 { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 15/86/101 |
| C-146 [pkg/utils/metricutil/metricutil.go:43](../../../pkg/utils/metricutil/metricutil.go#L43) | MetricConfig.PushInterval typeutil.Duration &#96;toml:"interval" json:"interval"&#96; | L97: if cfg.PushInterval.Duration == zeroDuration \|\| len(cfg.PushAddress) == 0 { ; L104: interval := cfg.PushInterval.Duration | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 15/86/101 |
| C-147 [server/config/config.go:51](../../../server/config/config.go#L51) | Config.ClientUrls          string &#96;toml:"client-urls" json:"client-urls"&#96; | L318: configutil.AdjustCommandLineString(flagSet, &c.ClientUrls, "client-urls") ; L378: configutil.AdjustString(&c.ClientUrls, defaultClientUrls) ; L379: configutil.AdjustString(&c.AdvertiseClientUrls, c.ClientUrls) ; L766: cfg.ListenClientUrls, err = parseUrls(c.ClientUrls) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-148 [server/config/config.go:52](../../../server/config/config.go#L52) | Config.PeerUrls            string &#96;toml:"peer-urls" json:"peer-urls"&#96; | L320: configutil.AdjustCommandLineString(flagSet, &c.PeerUrls, "peer-urls") ; L380: configutil.AdjustString(&c.PeerUrls, defaultPeerUrls) ; L381: configutil.AdjustString(&c.AdvertisePeerUrls, c.PeerUrls) ; L756: cfg.ListenPeerUrls, err = parseUrls(c.PeerUrls) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-149 [server/config/config.go:53](../../../server/config/config.go#L53) | Config.AdvertiseClientUrls string &#96;toml:"advertise-client-urls" json:"advertise-client-urls"&#96; | L319: configutil.AdjustCommandLineString(flagSet, &c.AdvertiseClientUrls, "advertise-client-urls") ; L379: configutil.AdjustString(&c.AdvertiseClientUrls, c.ClientUrls) ; L771: cfg.AdvertiseClientUrls, err = parseUrls(c.AdvertiseClientUrls) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-150 [server/config/config.go:54](../../../server/config/config.go#L54) | Config.AdvertisePeerUrls   string &#96;toml:"advertise-peer-urls" json:"advertise-peer-urls"&#96; | L321: configutil.AdjustCommandLineString(flagSet, &c.AdvertisePeerUrls, "advertise-peer-urls") ; L381: configutil.AdjustString(&c.AdvertisePeerUrls, c.PeerUrls) ; L387: items := strings.Split(c.AdvertisePeerUrls, ",") ; L761: cfg.AdvertisePeerUrls, err = parseUrls(c.AdvertisePeerUrls) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-151 [server/config/config.go:56](../../../server/config/config.go#L56) | Config.Name              string &#96;toml:"name" json:"name"&#96; | L316: configutil.AdjustCommandLineString(flagSet, &c.Name, "name") ; L364: if c.Name == "" { ; L369: configutil.AdjustString(&c.Name, fmt.Sprintf("%s-%s", defaultName, hostname)) ; L371: configutil.AdjustString(&c.DataDir, fmt.Sprintf("default.%s", c.Name)) ; L391: c.InitialCluster += fmt.Sprintf("%s%s=%s", sep, c.Name, item) ; L441: configutil.AdjustString(&c.Metric.PushJob, c.Name) ; L720: cfg.Name = c.Name | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-152 [server/config/config.go:57](../../../server/config/config.go#L57) | Config.DataDir           string &#96;toml:"data-dir" json:"data-dir"&#96; | L317: configutil.AdjustCommandLineString(flagSet, &c.DataDir, "data-dir") ; L338: dataDir, err := filepath.Abs(c.DataDir) ; L371: configutil.AdjustString(&c.DataDir, fmt.Sprintf("default.%s", c.Name)) ; L372: configutil.AdjustPath(&c.DataDir) ; L721: cfg.Dir = c.DataDir | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-153 [server/config/config.go:58](../../../server/config/config.go#L58) | Config.ForceNewCluster   bool   &#96;json:"force-new-cluster"&#96; | L328: configutil.AdjustCommandLineBool(flagSet, &c.ForceNewCluster, "force-new-cluster") ; L750: cfg.ForceNewCluster = c.ForceNewCluster | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-154 [server/config/config.go:59](../../../server/config/config.go#L59) | Config.EnableGRPCGateway bool   &#96;json:"enable-grpc-gateway"&#96; | L463: c.EnableGRPCGateway = defaultEnableGRPCGateway ; L752: cfg.EnableGRPCGateway = c.EnableGRPCGateway | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-155 [server/config/config.go:61](../../../server/config/config.go#L61) | Config.InitialCluster      string &#96;toml:"initial-cluster" json:"initial-cluster"&#96; | L322: configutil.AdjustCommandLineString(flagSet, &c.InitialCluster, "initial-cluster") ; L335: if c.Join != "" && c.InitialCluster != "" { ; L384: if len(c.InitialCluster) == 0 { ; L391: c.InitialCluster += fmt.Sprintf("%s%s=%s", sep, c.Name, item) ; L723: cfg.InitialCluster = c.InitialCluster | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-156 [server/config/config.go:62](../../../server/config/config.go#L62) | Config.InitialClusterState string &#96;toml:"initial-cluster-state" json:"initial-cluster-state"&#96; | L396: configutil.AdjustString(&c.InitialClusterState, defaultInitialClusterState) ; L724: cfg.ClusterState = c.InitialClusterState | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-157 [server/config/config.go:63](../../../server/config/config.go#L63) | Config.InitialClusterToken string &#96;toml:"initial-cluster-token" json:"initial-cluster-token"&#96; | L397: configutil.AdjustString(&c.InitialClusterToken, defaultInitialClusterToken) ; L725: cfg.InitialClusterToken = c.InitialClusterToken | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-158 [server/config/config.go:66](../../../server/config/config.go#L66) | Config.Join string &#96;toml:"join" json:"join"&#96; | L323: configutil.AdjustCommandLineString(flagSet, &c.Join, "join") ; L335: if c.Join != "" && c.InitialCluster != "" { ; L404: if _, err := parseUrls(c.Join); err != nil { ; L405: return errors.Errorf("failed to parse join addr:%s, err:%v", c.Join, err) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-159 [server/config/config.go:72](../../../server/config/config.go#L72) | Config.LeaderLease int64 &#96;toml:"lease" json:"lease"&#96; | L412: configutil.AdjustInt64(&c.LeaderLease, defaultLeaderLease) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-160 [server/config/config.go:75](../../../server/config/config.go#L75) | Config.Log log.Config &#96;toml:"log" json:"log"&#96; | L296: if c.Log.File.Filename == "" { ; L297: c.Log.File.Filename = c.LogFileDeprecated ; L303: if c.Log.Level == "" { ; L304: c.Log.Level = c.LogLevelDeprecated ; L314: configutil.AdjustCommandLineString(flagSet, &c.Log.Level, "log-level") ; L315: configutil.AdjustCommandLineString(flagSet, &c.Log.File.Filename, "log-file") ; L342: logFile, err := filepath.Abs(c.Log.File.Filename) ; L487: c.Log.DisableErrorVerbose = defaultDisableErrorVerbose ; L489: configutil.AdjustString(&c.Log.Format, defaultLogFormat) ; L490: configutil.AdjustString(&c.Log.Level, defaultLogLevel) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-161 [server/config/config.go:78](../../../server/config/config.go#L78) | Config.LogFileDeprecated  string &#96;toml:"log-file" json:"log-file,omitempty"&#96; | L293: if c.LogFileDeprecated != "" { ; L297: c.Log.File.Filename = c.LogFileDeprecated | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-162 [server/config/config.go:79](../../../server/config/config.go#L79) | Config.LogLevelDeprecated string &#96;toml:"log-level" json:"log-level,omitempty"&#96; | L300: if c.LogLevelDeprecated != "" { ; L304: c.Log.Level = c.LogLevelDeprecated | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-163 [server/config/config.go:85](../../../server/config/config.go#L85) | Config.MaxConcurrentTSOProxyStreamings int &#96;toml:"max-concurrent-tso-proxy-streamings" json:"max-concurrent-tso-proxy-streamings"&#96; | L409: configutil.AdjustInt(&c.MaxConcurrentTSOProxyStreamings, defaultMaxConcurrentTSOProxyStreamings) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-164 [server/config/config.go:88](../../../server/config/config.go#L88) | Config.TSOProxyRecvFromClientTimeout typeutil.Duration &#96;toml:"tso-proxy-recv-from-client-timeout" json:"tso-proxy-recv-from-client-timeout"&#96; | L410: configutil.AdjustDuration(&c.TSOProxyRecvFromClientTimeout, defaultTSOProxyRecvFromClientTimeout) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-165 [server/config/config.go:91](../../../server/config/config.go#L91) | Config.TSOSaveInterval typeutil.Duration &#96;toml:"tso-save-interval" json:"tso-save-interval"&#96; | L413: configutil.AdjustDuration(&c.TSOSaveInterval, DefaultTSOSaveInterval) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-166 [server/config/config.go:98](../../../server/config/config.go#L98) | Config.TSOUpdatePhysicalInterval typeutil.Duration &#96;toml:"tso-update-physical-interval" json:"tso-update-physical-interval"&#96; | L414: configutil.AdjustDuration(&c.TSOUpdatePhysicalInterval, defaultTSOUpdatePhysicalInterval) ; L417: c.TSOUpdatePhysicalInterval.Duration = MaxTSOUpdatePhysicalInterval ; L419: c.TSOUpdatePhysicalInterval.Duration = minTSOUpdatePhysicalInterval ; L421: if c.TSOUpdatePhysicalInterval.Duration != defaultTSOUpdatePhysicalInterval { | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-167 [server/config/config.go:101](../../../server/config/config.go#L101) | Config.EnableLocalTSO bool &#96;toml:"enable-local-tso" json:"enable-local-tso"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-168 [server/config/config.go:103](../../../server/config/config.go#L103) | Config.Metric metricutil.MetricConfig &#96;toml:"metric" json:"metric"&#96; | L324: configutil.AdjustCommandLineString(flagSet, &c.Metric.PushAddress, "metrics-addr") ; L382: configutil.AdjustDuration(&c.Metric.PushInterval, defaultMetricsPushInterval) ; L441: configutil.AdjustString(&c.Metric.PushJob, c.Name) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-169 [server/config/config.go:105](../../../server/config/config.go#L105) | Config.Schedule sc.ScheduleConfig &#96;toml:"schedule" json:"schedule"&#96; | L443: if err := c.Schedule.Adjust(configMetaData.Child("schedule"), reloading); err != nil { | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-170 [server/config/config.go:107](../../../server/config/config.go#L107) | Config.Replication sc.ReplicationConfig &#96;toml:"replication" json:"replication"&#96; | L446: if err := c.Replication.Adjust(configMetaData.Child("replication")); err != nil { | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 49；主题8领域 |
| C-171 [server/config/config.go:109](../../../server/config/config.go#L109) | Config.PDServerCfg PDServerConfig &#96;toml:"pd-server" json:"pd-server"&#96; | L450: if err := c.PDServerCfg.adjust(configMetaData.Child("pd-server")); err != nil { | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 47/90 |
| C-172 [server/config/config.go:111](../../../server/config/config.go#L111) | Config.ClusterVersion semver.Version &#96;toml:"cluster-version" json:"cluster-version"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-173 [server/config/config.go:116](../../../server/config/config.go#L116) | Config.Labels map[string]string &#96;toml:"labels" json:"labels"&#96; | L426: if c.Labels == nil { ; L427: c.Labels = make(map[string]string) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-174 [server/config/config.go:120](../../../server/config/config.go#L120) | Config.QuotaBackendBytes typeutil.ByteSize &#96;toml:"quota-backend-bytes" json:"quota-backend-bytes"&#96; | L433: c.QuotaBackendBytes = defaultQuotaBackendBytes ; L733: cfg.QuotaBackendBytes = int64(c.QuotaBackendBytes) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-175 [server/config/config.go:122](../../../server/config/config.go#L122) | Config.AutoCompactionMode string &#96;toml:"auto-compaction-mode" json:"auto-compaction-mode"&#96; | L430: configutil.AdjustString(&c.AutoCompactionMode, defaultCompactionMode) ; L731: cfg.AutoCompactionMode = c.AutoCompactionMode | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-176 [server/config/config.go:129](../../../server/config/config.go#L129) | Config.AutoCompactionRetention string &#96;toml:"auto-compaction-retention" json:"auto-compaction-retention-v2"&#96; | L431: configutil.AdjustString(&c.AutoCompactionRetention, defaultAutoCompactionRetention) ; L732: cfg.AutoCompactionRetention = c.AutoCompactionRetention | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-177 [server/config/config.go:132](../../../server/config/config.go#L132) | Config.TickInterval typeutil.Duration &#96;toml:"tick-interval" json:"tick-interval"&#96; | L438: configutil.AdjustDuration(&c.TickInterval, defaultTickInterval) ; L729: cfg.TickMs = uint(c.TickInterval.Duration / time.Millisecond) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-178 [server/config/config.go:134](../../../server/config/config.go#L134) | Config.ElectionInterval typeutil.Duration &#96;toml:"election-interval" json:"election-interval"&#96; | L439: configutil.AdjustDuration(&c.ElectionInterval, defaultElectionInterval) ; L730: cfg.ElectionMs = uint(c.ElectionInterval.Duration / time.Millisecond) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-179 [server/config/config.go:139](../../../server/config/config.go#L139) | Config.PreVote bool &#96;toml:"enable-prevote" json:"enable-prevote"&#96; | L460: c.PreVote = true ; L727: cfg.PreVote = c.PreVote | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-180 [server/config/config.go:141](../../../server/config/config.go#L141) | Config.MaxRequestBytes uint &#96;toml:"max-request-bytes" json:"max-request-bytes"&#96; | L436: c.MaxRequestBytes = defaultMaxRequestBytes ; L734: cfg.MaxRequestBytes = c.MaxRequestBytes | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-181 [server/config/config.go:143](../../../server/config/config.go#L143) | Config.Security configutil.SecurityConfig &#96;toml:"security" json:"security"&#96; | L325: configutil.AdjustCommandLineString(flagSet, &c.Security.CAPath, "cacert") ; L326: configutil.AdjustCommandLineString(flagSet, &c.Security.CertPath, "cert") ; L327: configutil.AdjustCommandLineString(flagSet, &c.Security.KeyPath, "key") ; L476: if err := c.Security.Encryption.Adjust(); err != nil { ; L736: cfg.ClientTLSInfo.ClientCertAuth = len(c.Security.CAPath) != 0 ; L737: cfg.ClientTLSInfo.TrustedCAFile = c.Security.CAPath ; L738: cfg.ClientTLSInfo.CertFile = c.Security.CertPath ; L739: cfg.ClientTLSInfo.KeyFile = c.Security.KeyPath ; L743: cfg.ClientTLSInfo.AllowedCNs = c.Security.CertAllowedCNs ; L745: cfg.PeerTLSInfo.ClientCertAuth = len(c.Security.CAPath) != 0 ; L746: cfg.PeerTLSInfo.TrustedCAFile = c.Security.CAPath ; L747: cfg.PeerTLSInfo.CertFile = c.Security.CertPath ; L748: cfg.PeerTLSInfo.KeyFile = c.Security.KeyPath ; L749: cfg.PeerTLSInfo.AllowedCNs = c.Security.CertAllowedCNs | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 82–85 |
| C-182 [server/config/config.go:145](../../../server/config/config.go#L145) | Config.LabelProperty LabelPropertyConfig &#96;toml:"label-property" json:"label-property"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-183 [server/config/config.go:148](../../../server/config/config.go#L148) | Config.WarningMsgs []string &#96;json:"-"&#96; | L295: c.WarningMsgs = append(c.WarningMsgs, msg) ; L302: c.WarningMsgs = append(c.WarningMsgs, msg) ; L309: c.WarningMsgs = append(c.WarningMsgs, msg) ; L361: c.WarningMsgs = append(c.WarningMsgs, err.Error()) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-184 [server/config/config.go:150](../../../server/config/config.go#L150) | Config.DisableStrictReconfigCheck bool &#96;json:"-"&#96; | L728: cfg.StrictReconfigCheck = !c.DisableStrictReconfigCheck | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 47/90 |
| C-185 [server/config/config.go:152](../../../server/config/config.go#L152) | Config.HeartbeatStreamBindInterval typeutil.Duration &#96;json:"-"&#96; | L455: configutil.AdjustDuration(&c.HeartbeatStreamBindInterval, defaultHeartbeatStreamRebindInterval) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-186 [server/config/config.go:153](../../../server/config/config.go#L153) | Config.LeaderPriorityCheckInterval typeutil.Duration &#96;json:"-"&#96; | L457: configutil.AdjustDuration(&c.LeaderPriorityCheckInterval, defaultLeaderPriorityCheckInterval) | [server/config/config_decode.rs](../../../server/config/config_decode.rs)（领域/TLS类型需适配） | 47/90 |
| C-187 [server/config/config.go:155](../../../server/config/config.go#L155) | Config.Logger   *zap.Logger        &#96;json:"-"&#96; | L751: cfg.ZapLoggerBuilder = embed.NewZapCoreLoggerBuilder(c.Logger, c.Logger.Core(), c.LogProps.Syncer) ; L753: cfg.Logger = "zap" | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 47/90 |
| C-188 [server/config/config.go:156](../../../server/config/config.go#L156) | Config.LogProps *log.ZapProperties &#96;json:"-"&#96; | L751: cfg.ZapLoggerBuilder = embed.NewZapCoreLoggerBuilder(c.Logger, c.Logger.Core(), c.LogProps.Syncer) | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 47/90 |
| C-189 [server/config/config.go:158](../../../server/config/config.go#L158) | Config.Dashboard DashboardConfig &#96;toml:"dashboard" json:"dashboard"&#96; | L466: c.Dashboard.adjust(configMetaData.Child("dashboard")) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-190 [server/config/config.go:160](../../../server/config/config.go#L160) | Config.ReplicationMode ReplicationModeConfig &#96;toml:"replication-mode" json:"replication-mode"&#96; | L468: c.ReplicationMode.adjust(configMetaData.Child("replication-mode")) ; L829: if !meta.IsDefined("replication-mode") \|\| NormalizeReplicationMode(c.ReplicationMode) == "" { ; L830: c.ReplicationMode = "majority" | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 49；主题8领域 |
| C-191 [server/config/config.go:162](../../../server/config/config.go#L162) | Config.Keyspace KeyspaceConfig &#96;toml:"keyspace" json:"keyspace"&#96; | L470: if err := c.Keyspace.adjust(configMetaData.Child("keyspace")); err != nil { | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-192 [server/config/config.go:164](../../../server/config/config.go#L164) | Config.Microservice MicroserviceConfig &#96;toml:"micro-service" json:"micro-service"&#96; | L474: c.Microservice.adjust(configMetaData.Child("micro-service")) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-193 [server/config/config.go:166](../../../server/config/config.go#L166) | Config.Controller rm.ControllerConfig &#96;toml:"controller" json:"controller"&#96; | L480: c.Controller.Adjust(configMetaData.Child("controller")) | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-194 [server/config/config.go:168](../../../server/config/config.go#L168) | Config.Metering config.MeteringConfig &#96;toml:"metering" json:"metering"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-195 [server/config/config.go:511](../../../server/config/config.go#L511) | PDServerConfig.UseRegionStorage bool &#96;toml:"use-region-storage" json:"use-region-storage,string"&#96; | L553: c.UseRegionStorage = defaultUseRegionStorage | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-196 [server/config/config.go:513](../../../server/config/config.go#L513) | PDServerConfig.MaxResetTSGap typeutil.Duration &#96;toml:"max-gap-reset-ts" json:"max-gap-reset-ts"&#96; | L551: configutil.AdjustDuration(&c.MaxResetTSGap, defaultMaxResetTSGap) | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-197 [server/config/config.go:516](../../../server/config/config.go#L516) | PDServerConfig.KeyType string &#96;toml:"key-type" json:"key-type"&#96; | L556: c.KeyType = defaultKeyType ; L635: if c.KeyType != "table" && c.KeyType != "raw" && c.KeyType != "txn" { | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-198 [server/config/config.go:518](../../../server/config/config.go#L518) | PDServerConfig.RuntimeServices typeutil.StringSlice &#96;toml:"runtime-services" json:"runtime-services"&#96; | L559: c.RuntimeServices = defaultRuntimeServices ; L616: runtimeServices := append(c.RuntimeServices[:0:0], c.RuntimeServices...) ; L618: cfg.RuntimeServices = runtimeServices | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-199 [server/config/config.go:521](../../../server/config/config.go#L521) | PDServerConfig.MetricStorage string &#96;toml:"metric-storage" json:"metric-storage"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-200 [server/config/config.go:523](../../../server/config/config.go#L523) | PDServerConfig.DashboardAddress string &#96;toml:"dashboard-address" json:"dashboard-address"&#96; | L562: c.DashboardAddress = defaultDashboardAddress ; L631: if err := ValidateURLWithScheme(c.DashboardAddress); err != nil { | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-201 [server/config/config.go:525](../../../server/config/config.go#L525) | PDServerConfig.FlowRoundByDigit int &#96;toml:"flow-round-by-digit" json:"flow-round-by-digit"&#96; | L565: configutil.AdjustInt(&c.FlowRoundByDigit, defaultFlowRoundByDigit) | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-202 [server/config/config.go:527](../../../server/config/config.go#L527) | PDServerConfig.MinResolvedTSPersistenceInterval typeutil.Duration &#96;toml:"min-resolved-ts-persistence-interval" json:"min-resolved-ts-persistence-interval"&#96; | L568: configutil.AdjustDuration(&c.MinResolvedTSPersistenceInterval, DefaultMinResolvedTSPersistenceInterval) | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-203 [server/config/config.go:529](../../../server/config/config.go#L529) | PDServerConfig.ServerMemoryLimit float64 &#96;toml:"server-memory-limit" json:"server-memory-limit"&#96; | L571: configutil.AdjustFloat64(&c.ServerMemoryLimit, defaultServerMemoryLimit) ; L574: c.ServerMemoryLimit = minServerMemoryLimit ; L576: c.ServerMemoryLimit = maxServerMemoryLimit | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-204 [server/config/config.go:531](../../../server/config/config.go#L531) | PDServerConfig.ServerMemoryLimitGCTrigger float64 &#96;toml:"server-memory-limit-gc-trigger" json:"server-memory-limit-gc-trigger"&#96; | L579: configutil.AdjustFloat64(&c.ServerMemoryLimitGCTrigger, defaultServerMemoryLimitGCTrigger) ; L582: c.ServerMemoryLimitGCTrigger = minServerMemoryLimitGCTrigger ; L584: c.ServerMemoryLimitGCTrigger = maxServerMemoryLimitGCTrigger | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-205 [server/config/config.go:533](../../../server/config/config.go#L533) | PDServerConfig.EnableGOGCTuner bool &#96;toml:"enable-gogc-tuner" json:"enable-gogc-tuner,string"&#96; | L587: c.EnableGOGCTuner = defaultEnableGOGCTuner | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-206 [server/config/config.go:535](../../../server/config/config.go#L535) | PDServerConfig.GCTunerThreshold float64 &#96;toml:"gc-tuner-threshold" json:"gc-tuner-threshold"&#96; | L590: configutil.AdjustFloat64(&c.GCTunerThreshold, defaultGCTunerThreshold) ; L593: c.GCTunerThreshold = minGCTunerThreshold ; L595: c.GCTunerThreshold = maxGCTunerThreshold | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-207 [server/config/config.go:537](../../../server/config/config.go#L537) | PDServerConfig.BlockSafePointV1 bool &#96;toml:"block-safe-point-v1" json:"block-safe-point-v1,string"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-208 [server/config/config.go:539](../../../server/config/config.go#L539) | PDServerConfig.GCKeyspaceIndexLoadConcurrency int &#96;toml:"gc-keyspace-index-load-concurrency" json:"gc-keyspace-index-load-concurrency"&#96; | L546: configutil.AdjustInt(&c.GCKeyspaceIndexLoadConcurrency, DefaultGCKeyspaceIndexLoadConcurrency) | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-209 [server/config/config.go:541](../../../server/config/config.go#L541) | PDServerConfig.GCStateCacheWarmupConcurrency int &#96;toml:"gc-state-cache-warmup-concurrency" json:"gc-state-cache-warmup-concurrency"&#96; | L549: configutil.AdjustInt(&c.GCStateCacheWarmupConcurrency, DefaultGCStateCacheWarmupConcurrency) | [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs)（领域/TLS类型需适配） | 47/90 |
| C-210 [server/config/config.go:667](../../../server/config/config.go#L667) | StoreLabel.Key   string &#96;toml:"key" json:"key"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-211 [server/config/config.go:668](../../../server/config/config.go#L668) | StoreLabel.Value string &#96;toml:"value" json:"value"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-212 [server/config/config.go:781](../../../server/config/config.go#L781) | DashboardConfig.TiDBCAPath            string &#96;toml:"tidb-cacert-path" json:"tidb-cacert-path"&#96; | L794: if (len(c.TiDBCertPath) != 0 && len(c.TiDBKeyPath) != 0) \|\| len(c.TiDBCAPath) != 0 { ; L798: TrustedCAFile: c.TiDBCAPath, | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-213 [server/config/config.go:782](../../../server/config/config.go#L782) | DashboardConfig.TiDBCertPath          string &#96;toml:"tidb-cert-path" json:"tidb-cert-path"&#96; | L794: if (len(c.TiDBCertPath) != 0 && len(c.TiDBKeyPath) != 0) \|\| len(c.TiDBCAPath) != 0 { ; L796: CertFile:      c.TiDBCertPath, | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-214 [server/config/config.go:783](../../../server/config/config.go#L783) | DashboardConfig.TiDBKeyPath           string &#96;toml:"tidb-key-path" json:"tidb-key-path"&#96; | L794: if (len(c.TiDBCertPath) != 0 && len(c.TiDBKeyPath) != 0) \|\| len(c.TiDBCAPath) != 0 { ; L797: KeyFile:       c.TiDBKeyPath, | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-215 [server/config/config.go:784](../../../server/config/config.go#L784) | DashboardConfig.PublicPathPrefix      string &#96;toml:"public-path-prefix" json:"public-path-prefix"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-216 [server/config/config.go:785](../../../server/config/config.go#L785) | DashboardConfig.TempDir               string &#96;toml:"temp-dir" json:"temp-dir"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-217 [server/config/config.go:786](../../../server/config/config.go#L786) | DashboardConfig.InternalProxy         bool   &#96;toml:"internal-proxy" json:"internal-proxy"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-218 [server/config/config.go:787](../../../server/config/config.go#L787) | DashboardConfig.EnableTelemetry       bool   &#96;toml:"enable-telemetry" json:"enable-telemetry"&#96; | L811: c.EnableTelemetry = defaultEnableTelemetry | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-219 [server/config/config.go:788](../../../server/config/config.go#L788) | DashboardConfig.EnableExperimental    bool   &#96;toml:"enable-experimental" json:"enable-experimental"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-220 [server/config/config.go:789](../../../server/config/config.go#L789) | DashboardConfig.DisableCustomPromAddr bool   &#96;toml:"disable-custom-prom-addr" json:"disable-custom-prom-addr"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-221 [server/config/config.go:818](../../../server/config/config.go#L818) | ReplicationModeConfig.ReplicationMode string                      &#96;toml:"replication-mode" json:"replication-mode"&#96; // can be 'dr-auto-sync' or 'majority', default value is 'majority' | L468: c.ReplicationMode.adjust(configMetaData.Child("replication-mode")) ; L829: if !meta.IsDefined("replication-mode") \|\| NormalizeReplicationMode(c.ReplicationMode) == "" { ; L830: c.ReplicationMode = "majority" | [server/config/nested_config.rs](../../../server/config/nested_config.rs)（领域/TLS类型需适配） | 49；主题8领域 |
| C-222 [server/config/config.go:819](../../../server/config/config.go#L819) | ReplicationModeConfig.DRAutoSync      DRAutoSyncReplicationConfig &#96;toml:"dr-auto-sync" json:"dr-auto-sync"&#96;         // used when ReplicationMode is 'dr-auto-sync' | L832: c.DRAutoSync.adjust(meta.Child("dr-auto-sync")) | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 49；主题8领域 |
| C-223 [server/config/config.go:847](../../../server/config/config.go#L847) | DRAutoSyncReplicationConfig.LabelKey           string            &#96;toml:"label-key" json:"label-key"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-224 [server/config/config.go:848](../../../server/config/config.go#L848) | DRAutoSyncReplicationConfig.Primary            string            &#96;toml:"primary" json:"primary"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-225 [server/config/config.go:849](../../../server/config/config.go#L849) | DRAutoSyncReplicationConfig.DR                 string            &#96;toml:"dr" json:"dr"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-226 [server/config/config.go:850](../../../server/config/config.go#L850) | DRAutoSyncReplicationConfig.PrimaryReplicas    int               &#96;toml:"primary-replicas" json:"primary-replicas"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-227 [server/config/config.go:851](../../../server/config/config.go#L851) | DRAutoSyncReplicationConfig.DRReplicas         int               &#96;toml:"dr-replicas" json:"dr-replicas"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-228 [server/config/config.go:852](../../../server/config/config.go#L852) | DRAutoSyncReplicationConfig.WaitStoreTimeout   typeutil.Duration &#96;toml:"wait-store-timeout" json:"wait-store-timeout"&#96; | L859: c.WaitStoreTimeout = typeutil.NewDuration(defaultDRWaitStoreTimeout) | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-229 [server/config/config.go:853](../../../server/config/config.go#L853) | DRAutoSyncReplicationConfig.WaitRecoverTimeout typeutil.Duration &#96;toml:"wait-recover-timeout" json:"wait-recover-timeout"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-230 [server/config/config.go:854](../../../server/config/config.go#L854) | DRAutoSyncReplicationConfig.PauseRegionSplit   bool              &#96;toml:"pause-region-split" json:"pause-region-split,string"&#96; | 本文件无显式调整；见构造/嵌套Adjust附录，未取得运行默认值 | [server/config/replication_config.rs](../../../server/config/replication_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-231 [server/config/config.go:865](../../../server/config/config.go#L865) | MicroserviceConfig.EnableSchedulingFallback  bool &#96;toml:"enable-scheduling-fallback" json:"enable-scheduling-fallback,string"&#96; | L875: c.EnableSchedulingFallback = defaultEnableSchedulingFallback | [server/config/microservice_config.rs](../../../server/config/microservice_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-232 [server/config/config.go:866](../../../server/config/config.go#L866) | MicroserviceConfig.EnableTSODynamicSwitching bool &#96;toml:"enable-tso-dynamic-switching" json:"enable-tso-dynamic-switching,string"&#96; | L878: c.EnableTSODynamicSwitching = defaultEnableTSODynamicSwitching | [server/config/microservice_config.rs](../../../server/config/microservice_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-233 [server/config/config.go:870](../../../server/config/config.go#L870) | MicroserviceConfig.EnableResourceManagerFallback bool &#96;toml:"enable-resource-manager-fallback" json:"enable-resource-manager-fallback,string"&#96; | L881: c.EnableResourceManagerFallback = defaultEnableResourceManagerFallback | [server/config/microservice_config.rs](../../../server/config/microservice_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-234 [server/config/config.go:909](../../../server/config/config.go#L909) | KeyspaceConfig.PreAlloc []string &#96;toml:"pre-alloc" json:"pre-alloc"&#96; | L989: cfg.PreAlloc = append(c.PreAlloc[:0:0], c.PreAlloc...) ; L1001: ret := make([]string, len(c.PreAlloc)) | [server/config/keyspace_config.rs](../../../server/config/keyspace_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-235 [server/config/config.go:911](../../../server/config/config.go#L911) | KeyspaceConfig.WaitRegionSplit bool &#96;toml:"wait-region-split" json:"wait-region-split"&#96; | L935: c.WaitRegionSplit = true | [server/config/keyspace_config.rs](../../../server/config/keyspace_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-236 [server/config/config.go:913](../../../server/config/config.go#L913) | KeyspaceConfig.WaitRegionSplitTimeout typeutil.Duration &#96;toml:"wait-region-split-timeout" json:"wait-region-split-timeout"&#96; | L927: if c.CheckRegionSplitInterval.Duration >= c.WaitRegionSplitTimeout.Duration { ; L938: c.WaitRegionSplitTimeout = typeutil.NewDuration(defaultWaitRegionSplitTimeout) | [server/config/keyspace_config.rs](../../../server/config/keyspace_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-237 [server/config/config.go:915](../../../server/config/config.go#L915) | KeyspaceConfig.CheckRegionSplitInterval typeutil.Duration &#96;toml:"check-region-split-interval" json:"check-region-split-interval"&#96; | L927: if c.CheckRegionSplitInterval.Duration >= c.WaitRegionSplitTimeout.Duration { ; L941: c.CheckRegionSplitInterval = typeutil.NewDuration(defaultCheckRegionSplitInterval) | [server/config/keyspace_config.rs](../../../server/config/keyspace_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-238 [server/config/config.go:918](../../../server/config/config.go#L918) | KeyspaceConfig.MetaServiceGroups map[string]string &#96;toml:"meta-service-groups" json:"meta-service-groups"&#96; | L944: return AdjustMetaServiceGroups(c.MetaServiceGroups) ; L990: if c.MetaServiceGroups != nil { ; L991: cfg.MetaServiceGroups = make(map[string]string, len(c.MetaServiceGroups)) ; L992: for name, endpoint := range c.MetaServiceGroups { ; L993: cfg.MetaServiceGroups[name] = endpoint ; L1023: ret := make(map[string]string, len(c.MetaServiceGroups)) ; L1024: for name, endpoint := range c.MetaServiceGroups { ; L1032: c.MetaServiceGroups = metaServiceGroups | [server/config/keyspace_config.rs](../../../server/config/keyspace_config.rs)（领域/TLS类型需适配） | 47/90 |
| C-239 [server/config/persist_options.go:829](../../../server/config/persist_options.go#L829) | persistedConfig.StoreConfig sc.StoreConfig &#96;json:"store"&#96; | L878: StoreConfig: *o.GetStoreConfig(), ; L903: cfg.StoreConfig.Adjust() | [server/config/lib.rs](../../../server/config/lib.rs)（领域/TLS类型需适配） | 47/90 |
| C-240 [server/config/service_middleware_config.go:27](../../../server/config/service_middleware_config.go#L27) | ServiceMiddlewareConfig.AuditConfig         &#96;json:"audit"&#96; | L46: AuditConfig:         audit, | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-241 [server/config/service_middleware_config.go:28](../../../server/config/service_middleware_config.go#L28) | ServiceMiddlewareConfig.RateLimitConfig     &#96;json:"rate-limit"&#96; | L47: RateLimitConfig:     rateLimit, | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-242 [server/config/service_middleware_config.go:29](../../../server/config/service_middleware_config.go#L29) | ServiceMiddlewareConfig.GRPCRateLimitConfig &#96;json:"grpc-rate-limit"&#96; | L48: GRPCRateLimitConfig: grpcRateLimit, | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-243 [server/config/service_middleware_config.go:62](../../../server/config/service_middleware_config.go#L62) | AuditConfig.EnableAudit bool &#96;json:"enable-audit,string"&#96; | L35: EnableAudit: defaultEnableAuditMiddleware, | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-244 [server/config/service_middleware_config.go:74](../../../server/config/service_middleware_config.go#L74) | RateLimitConfig.EnableRateLimit bool &#96;json:"enable-rate-limit,string"&#96; | L38: EnableRateLimit: defaultEnableRateLimitMiddleware, ; L42: EnableRateLimit: defaultEnableRateLimitMiddleware, | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-245 [server/config/service_middleware_config.go:76](../../../server/config/service_middleware_config.go#L76) | RateLimitConfig.LimiterConfig map[string]ratelimit.DimensionConfig &#96;json:"limiter-config"&#96; | L39: LimiterConfig:   make(map[string]ratelimit.DimensionConfig), ; L43: LimiterConfig:   make(map[string]ratelimit.DimensionConfig), ; L81: m := make(map[string]ratelimit.DimensionConfig, len(c.LimiterConfig)) ; L82: for k, v := range c.LimiterConfig { ; L86: cfg.LimiterConfig = m ; L100: m := make(map[string]ratelimit.DimensionConfig, len(c.LimiterConfig)) ; L101: for k, v := range c.LimiterConfig { ; L105: cfg.LimiterConfig = m | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-246 [server/config/service_middleware_config.go:93](../../../server/config/service_middleware_config.go#L93) | GRPCRateLimitConfig.EnableRateLimit bool &#96;json:"enable-grpc-rate-limit,string"&#96; | L38: EnableRateLimit: defaultEnableRateLimitMiddleware, ; L42: EnableRateLimit: defaultEnableRateLimitMiddleware, | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |
| C-247 [server/config/service_middleware_config.go:95](../../../server/config/service_middleware_config.go#L95) | GRPCRateLimitConfig.LimiterConfig map[string]ratelimit.DimensionConfig &#96;json:"grpc-limiter-config"&#96; | L39: LimiterConfig:   make(map[string]ratelimit.DimensionConfig), ; L43: LimiterConfig:   make(map[string]ratelimit.DimensionConfig), ; L81: m := make(map[string]ratelimit.DimensionConfig, len(c.LimiterConfig)) ; L82: for k, v := range c.LimiterConfig { ; L86: cfg.LimiterConfig = m ; L100: m := make(map[string]ratelimit.DimensionConfig, len(c.LimiterConfig)) ; L101: for k, v := range c.LimiterConfig { ; L105: cfg.LimiterConfig = m | [server/config/middleware_config.rs](../../../server/config/middleware_config.rs)（领域/TLS类型需适配） | 50/79/80/81 |

## 默认值、校验与废弃兼容处理原文

这些块为配置行的条件上下文。符号值、版本/平台条件、显式设置检测和嵌套调用均保留；未将源码执行结果当成冻结输出。

### client/resource_group/controller/config.go — owner 90/91/92

[client/resource_group/controller/config.go:21](../../../client/resource_group/controller/config.go#L21)

```go
const (
	// movingAvgFactor is the weight applied to a new "sample" of RU usage (with one
	// sample per mainLoopUpdateInterval).
	//
	// If we want a factor of 0.5 per second, this should be:
	//
	//	0.5^(1 second / mainLoopUpdateInterval)
	movingAvgFactor                = 0.5
	notifyFraction                 = 0.1
	tokenReserveFraction           = 0.8
	consumptionsReportingThreshold = 100
	extendedReportingPeriodFactor  = 4
	// defaultGroupCleanupInterval is the interval to clean up the deleted resource groups in memory.
	defaultGroupCleanupInterval = 5 * time.Minute
	// defaultGroupStateUpdateInterval is the interval to update the state of the resource groups.
	defaultGroupStateUpdateInterval = 1 * time.Second
	// targetPeriod indicate how long it is expected to cost token when acquiring token.
	// According to the resource control Grafana panel and Prometheus sampling period, the period should be the factor of 15.
	defaultTargetPeriod = 5 * time.Second
	// defaultMaxWaitDuration is the max duration to wait for the token before throwing error.
	defaultMaxWaitDuration = 30 * time.Second
	// defaultLTBTokenRPCMaxDelay is the upper bound of backoff delay for local token bucket RPC.
	defaultLTBTokenRPCMaxDelay = 1 * time.Second
	// defaultWaitRetryTimes is the times to retry when waiting for the token.
	defaultWaitRetryTimes = 20
	// defaultWaitRetryInterval is the interval to retry when waiting for the token.
	defaultWaitRetryInterval = 50 * time.Millisecond
)
```

[client/resource_group/controller/config.go:50](../../../client/resource_group/controller/config.go#L50)

```go
const (

	// 1 RU = 8 storage read requests
	defaultReadBaseCost = 1. / 8 // 0.125
	// 1 RU = 2 storage read batch requests
	defaultReadPerBatchBaseCost = 1. / 2 // 0.5
	// 1 RU = 1 storage write request
	defaultWriteBaseCost = 1
	// 1 RU = 1 storage write batch request
	defaultWritePerBatchBaseCost = 1
	// 1 RU = 64 KiB read bytes
	defaultReadCostPerByte = 1. / (64 * 1024)
	// 1 RU = 1 KiB written bytes
	defaultWriteCostPerByte = 1. / 1024
	// 1 RU = 3 millisecond CPU time
	defaultCPUMsCost = 1. / 3

	// Because the resource manager has not been deployed in microservice mode,
	// do not enable this function.
	defaultDegradedModeWaitDuration = time.Duration(0)
	defaultAvgBatchProportion       = 0.7
)
```

[client/resource_group/controller/config.go:113](../../../client/resource_group/controller/config.go#L113)

```go
const (
	// RUVersionV1 is the default RU calculation version.
	RUVersionV1 RUVersion = 1
	// RUVersionV2 is the new RU calculation version with detailed metrics.
	RUVersionV2 RUVersion = 2
)
```

[client/resource_group/controller/config.go:136](../../../client/resource_group/controller/config.go#L136)

```go
func (c *Config) Adjust() {
	// valid the configuration, TODO: separately add the valid function.
	if c.LTBMaxWaitDuration.Duration == 0 {
		c.LTBMaxWaitDuration = NewDuration(defaultMaxWaitDuration)
	}
	if c.WaitRetryInterval.Duration == 0 {
		c.WaitRetryInterval = NewDuration(defaultWaitRetryInterval)
	}
	// adjust the client settings. calculate the retry times.
	if int(c.LTBTokenRPCMaxDelay.Duration) != int(c.WaitRetryInterval.Duration)*c.WaitRetryTimes {
		c.WaitRetryTimes = int(c.LTBTokenRPCMaxDelay.Duration / c.WaitRetryInterval.Duration)
	}
}
```

[client/resource_group/controller/config.go:151](../../../client/resource_group/controller/config.go#L151)

```go
func DefaultConfig() *Config {
	return &Config{
		BaseConfig: BaseConfig{
			DegradedModeWaitDuration: NewDuration(defaultDegradedModeWaitDuration),
			RequestUnit:              DefaultRequestUnitConfig(),
			EnableControllerTraceLog: false,
			LTBMaxWaitDuration:       NewDuration(defaultMaxWaitDuration),
			LTBTokenRPCMaxDelay:      NewDuration(defaultLTBTokenRPCMaxDelay),
		},
		LocalBucketConfig: LocalBucketConfig{
			TokenRPCParams: TokenRPCParams{
				WaitRetryInterval: NewDuration(defaultWaitRetryInterval),
				WaitRetryTimes:    defaultWaitRetryTimes,
			},
		},
	}
}
```

[client/resource_group/controller/config.go:192](../../../client/resource_group/controller/config.go#L192)

```go
func DefaultRequestUnitConfig() RequestUnitConfig {
	return RequestUnitConfig{
		ReadBaseCost:          defaultReadBaseCost,
		ReadPerBatchBaseCost:  defaultReadPerBatchBaseCost,
		ReadCostPerByte:       defaultReadCostPerByte,
		WriteBaseCost:         defaultWriteBaseCost,
		WritePerBatchBaseCost: defaultWritePerBatchBaseCost,
		WriteCostPerByte:      defaultWriteCostPerByte,
		CPUMsCost:             defaultCPUMsCost,
	}
}
```

[client/resource_group/controller/config.go:227](../../../client/resource_group/controller/config.go#L227)

```go
func DefaultRUConfig() *RUConfig {
	return GenerateRUConfig(
		DefaultConfig(),
	)
}
```

### client/resource_group/controller/global_controller.go — owner 90/91/92

[client/resource_group/controller/global_controller.go:45](../../../client/resource_group/controller/global_controller.go#L45)

```go
const (
	defaultResourceGroupName = "default"
	controllerConfigPath     = "resource_group/controller"
	maxNotificationChanLen   = 200
	needTokensAmplification  = 1.1
	trickleReserveDuration   = 1250 * time.Millisecond
	slowNotifyFilterDuration = 10 * time.Millisecond

	watchRetryInterval = 30 * time.Second

	bigRequestThreshold = 4 * 1024 * 1024 // 4MB -> 16 RRU
)
```

[client/resource_group/controller/global_controller.go:232](../../../client/resource_group/controller/global_controller.go#L232)

```go
func loadServerConfig(ctx context.Context, provider ResourceGroupProvider) (*Config, error) {
	resp, err := provider.Get(ctx, []byte(controllerConfigPath))
	if err != nil {
		return nil, err
	}
	config := DefaultConfig()
	defer config.Adjust()
	kvs := resp.GetKvs()
	if len(kvs) == 0 {
		log.Warn("[resource group controller] server does not save config, load config failed")
		return config, nil
	}
	err = json.Unmarshal(kvs[0].GetValue(), config)
	if err != nil {
		return nil, err
	}
	return config, nil
}
```

[client/resource_group/controller/global_controller.go:259](../../../client/resource_group/controller/global_controller.go#L259)

```go
func (c *ResourceGroupsController) GetRUVersion() RUVersion {
	v := c.ruVersion.Load()
	if v <= 0 {
		return DefaultRUVersion
	}
	return v
}
```

[client/resource_group/controller/global_controller.go:268](../../../client/resource_group/controller/global_controller.go#L268)

```go
func (c *ResourceGroupsController) updateRUVersionFromConfig(config *Config) {
	if config.RUVersionPolicy != nil {
		if v, ok := config.RUVersionPolicy.Overrides[c.keyspaceID]; ok {
			old := c.ruVersion.Swap(v)
			if old != v {
				log.Info("ru version updated from controller config",
					zap.Int32("old", old), zap.Int32("new", v),
					zap.Uint32("keyspace-id", c.keyspaceID))
			}
		} else if config.RUVersionPolicy.Default > 0 {
			old := c.ruVersion.Swap(config.RUVersionPolicy.Default)
			if old != config.RUVersionPolicy.Default {
				log.Info("ru version updated to default from controller config",
					zap.Int32("old", old), zap.Int32("new", config.RUVersionPolicy.Default),
					zap.Uint32("keyspace-id", c.keyspaceID))
			}
		} else {
			c.ruVersion.Store(0)
		}
	} else {
		// No policy in the config means no RU version override is active.
		// Reset the stored value to 0; GetRUVersion() will return DefaultRUVersion
		// so callers always see a valid version even when no policy exists.
		c.ruVersion.Store(0)
	}
}
```

[client/resource_group/controller/global_controller.go:302](../../../client/resource_group/controller/global_controller.go#L302)

```go
func (c *ResourceGroupsController) Start(ctx context.Context) {
	c.loopCtx, c.loopCancel = context.WithCancel(ctx)
	c.wg.Add(1)
	go func() {
		defer c.wg.Done()
		if c.ruConfig.DegradedModeWaitDuration > 0 {
			c.run.responseDeadline = time.NewTimer(c.ruConfig.DegradedModeWaitDuration)
			c.run.responseDeadline.Stop()
			defer c.run.responseDeadline.Stop()
		}
		cleanupTicker := time.NewTicker(defaultGroupCleanupInterval)
		defer cleanupTicker.Stop()
		stateUpdateTicker := time.NewTicker(defaultGroupStateUpdateInterval)
		defer stateUpdateTicker.Stop()
		emergencyTokenAcquisitionTicker := time.NewTicker(defaultTargetPeriod)
		defer emergencyTokenAcquisitionTicker.Stop()

		failpoint.Inject("fastCleanup", func() {
			cleanupTicker.Reset(100 * time.Millisecond)
		})
		failpoint.Inject("acceleratedReportingPeriod", func() {
			stateUpdateTicker.Reset(time.Millisecond * 100)
		})

		resp, err := c.provider.Get(ctx, []byte(controllerConfigPath))
		if err != nil {
			log.Warn("load resource group revision failed", zap.Error(err))
		}
		cfgRevision := resp.GetHeader().GetRevision()

		failpoint.Inject("staleRevision", func(val failpoint.Value) {
			if rev, ok := val.(int); ok {
				cfgRevision = int64(rev)
			}
		})
		var watchMetaChannel, watchConfigChannel chan *metastorage.WatchResponse
		if !c.ruConfig.isSingleGroupByKeyspace {
			// Use WithPrevKV() to get the previous key-value pair when get Delete Event.
			prefix := pd.GroupSettingsPathPrefixBytes(c.keyspaceID)
			watchMetaChannel, err = c.provider.Watch(ctx, prefix, opt.WithPrefix(), opt.WithPrevKV())
			if err != nil {
				log.Warn("watch resource group meta failed", zap.Error(err))
			}
		}

		watchConfigChannel, err = c.provider.Watch(ctx, pd.ControllerConfigPathPrefixBytes, opt.WithRev(cfgRevision))
		if err != nil {
			log.Warn("watch resource group config failed", zap.Error(err))
		}
		watchRetryTimer := time.NewTimer(watchRetryInterval)
		defer watchRetryTimer.Stop()

		for {
			select {
			/* tickers */
			case <-cleanupTicker.C:
				c.cleanUpResourceGroup()
			case <-stateUpdateTicker.C:
				c.executeOnAllGroups((*groupCostController).updateRunState)
				c.executeOnAllGroups((*groupCostController).updateAvgRequestResourcePerSec)
				if len(c.run.currentRequests) == 0 {
					c.collectTokenBucketRequests(c.loopCtx, FromPeriodReport, periodicReport /* select resource groups which should be reported periodically */, notifyMsg{})
				}
			case <-watchRetryTimer.C:
				if !c.ruConfig.isSingleGroupByKeyspace && watchMetaChannel == nil {
					// Use WithPrevKV() to get the previous key-value pair when get Delete Event.
					prefix := pd.GroupSettingsPathPrefixBytes(c.keyspaceID)
					watchMetaChannel, err = c.provider.Watch(ctx, prefix, opt.WithPrefix(), opt.WithPrevKV())
					if err != nil {
						log.Warn("watch resource group meta failed", zap.Error(err))
						watchRetryTimer.Reset(watchRetryInterval)
						failpoint.Inject("watchStreamError", func() {
							watchRetryTimer.Reset(20 * time.Millisecond)
						})
					}
				}
				if watchConfigChannel == nil {
					watchConfigChannel, err = c.provider.Watch(ctx, pd.ControllerConfigPathPrefixBytes, opt.WithRev(cfgRevision))
					if err != nil {
						log.Warn("watch resource group config failed", zap.Error(err))
						watchRetryTimer.Reset(watchRetryInterval)
					}
				}
			case <-emergencyTokenAcquisitionTicker.C:
				c.executeOnAllGroups((*groupCostController).resetEmergencyTokenAcquisition)
			/* channels */
			case <-c.loopCtx.Done():
				metrics.ResourceGroupStatusGauge.Reset()
				c.requestSourceStates.Range(func(_, v any) bool {
					v.(*requestSourceMetricsState).cleanup()
					return true
				})
				return
			case <-c.responseDeadlineCh:
				c.run.inDegradedMode.Store(true)
				c.executeOnAllGroups((*groupCostController).applyDegradedMode)
				log.Warn("[resource group controller] enter degraded mode")
			case resp := <-c.tokenResponseChan:
				if resp != nil {
					c.executeOnAllGroups((*groupCostController).updateRunState)
					c.handleTokenBucketResponse(resp)
				}
				c.run.currentRequests = nil
			case notifyMsg := <-c.lowTokenNotifyChan:
				c.executeOnAllGroups((*groupCostController).updateRunState)
				c.executeOnAllGroups((*groupCostController).updateAvgRequestResourcePerSec)
				c.collectTokenBucketRequests(c.loopCtx, FromLowRU, lowToken /* select low tokens resource group */, notifyMsg)
				if c.IsDegraded() {
					c.executeOnAllGroups((*groupCostController).applyDegradedMode)
				}
			case resp, ok := <-watchMetaChannel:
				failpoint.Inject("disableWatch", func() {
					if c.ruConfig.isSingleGroupByKeyspace {
						panic("disableWatch")
					}
				})
				if !ok {
					watchMetaChannel = nil
					watchRetryTimer.Reset(watchRetryInterval)
					failpoint.Inject("watchStreamError", func() {
						watchRetryTimer.Reset(20 * time.Millisecond)
					})
					continue
				}
				for _, item := range resp.Events {
					group := &rmpb.ResourceGroup{}
					switch item.Type {
					case meta_storagepb.Event_PUT:
						if err = proto.Unmarshal(item.Kv.Value, group); err != nil {
							continue
						}
						name := group.GetName()
						gc, ok := c.loadGroupController(name)
						if !ok {
							continue
						}
						if !gc.tombstone.Load() {
							gc.modifyMeta(group)
							continue
						}
						// If the resource group is marked as tombstone before, re-create the resource group controller.
						newGC, err := newGroupCostController(
							group,
							c.ruConfig,
							c.lowTokenNotifyChan,
							c.tokenBucketUpdateChan,
							c.getOrCreateRequestSourceMetricsState(name),
						)
						if err != nil {
							log.Warn("[resource group controller] re-create resource group cost controller for tombstone failed",
								zap.String("name", name), zap.Error(err))
							continue
						}
						if c.groupsController.CompareAndSwap(name, gc, newGC) {
							log.Info("[resource group controller] re-create resource group cost controller for tombstone",
								zap.String("name", name))
						}
					case meta_storagepb.Event_DELETE:
						// Prev-kv is compacted means there must have been a delete event before this event,
						// which means that this is just a duplicated event, so we can just ignore it.
						if item.PrevKv == nil {
							log.Info("[resource group controller] previous key-value pair has been compacted",
								zap.String("required-key", string(item.Kv.Key)), zap.String("value", string(item.Kv.Value)))
							continue
						}
						if err = proto.Unmarshal(item.PrevKv.Value, group); err != nil {
							continue
						}
						c.tombstoneGroupCostController(group.GetName())
					}
				}
			case resp, ok := <-watchConfigChannel:
				if !ok {
					watchConfigChannel = nil
					watchRetryTimer.Reset(watchRetryInterval)
					failpoint.Inject("watchStreamError", func() {
						watchRetryTimer.Reset(20 * time.Millisecond)
					})
					continue
				}
				if resp.CompactRevision > cfgRevision {
					cfgRevision = resp.CompactRevision
				}
				for _, item := range resp.Events {
					cfgRevision = item.Kv.ModRevision
					config := DefaultConfig()
					if err := json.Unmarshal(item.Kv.Value, config); err != nil {
						continue
					}
					config.Adjust()
					c.ruConfig = GenerateRUConfig(config)

					// Stay compatible with serverless
					for _, opt := range c.opts {
						opt(c)
					}
					copyCfg := *c.ruConfig
					c.safeRuConfig.Store(&copyCfg)
					if enableControllerTraceLog.Load() != config.EnableControllerTraceLog {
						enableControllerTraceLog.Store(config.EnableControllerTraceLog)
					}
					// Update ru version from the controller config RUVersionPolicy.
					c.updateRUVersionFromConfig(config)
					log.Info("load resource controller config after config changed", zap.Reflect("config", config), zap.Reflect("ruConfig", c.ruConfig))
				}
			case gc := <-c.tokenBucketUpdateChan:
				go gc.handleTokenBucketUpdateEvent(c.loopCtx)
			}
		}
	}()
}
```

[client/resource_group/controller/global_controller.go:571](../../../client/resource_group/controller/global_controller.go#L571)

```go
func normalizeGetResourceGroupError(err error) error {
	switch {
	case goerrors.Is(err, context.Canceled):
		return context.Canceled
	case goerrors.Is(err, context.DeadlineExceeded):
		return context.DeadlineExceeded
	default:
		return err
	}
}
```

[client/resource_group/controller/global_controller.go:591](../../../client/resource_group/controller/global_controller.go#L591)

```go
func (c *ResourceGroupsController) shouldUseDegradedResourceGroup(err error) bool {
	if c.degradedRUSettings == nil || err == nil {
		return false
	}
	if isResourceGroupNotFoundError(err) {
		return false
	}
	if goerrors.Is(err, context.Canceled) || goerrors.Is(err, context.DeadlineExceeded) {
		return false
	}
	causeErr := unwrapGetResourceGroupError(err)
	if errs.IsLeaderChange(causeErr) {
		return true
	}
	switch status.Code(causeErr) {
	case codes.Canceled:
		return false
	case codes.Unavailable, codes.DeadlineExceeded:
		return true
	default:
		return false
	}
}
```

[client/resource_group/controller/global_controller.go:681](../../../client/resource_group/controller/global_controller.go#L681)

```go
func (c *ResourceGroupsController) tombstoneGroupCostController(name string) {
	_, ok := c.loadGroupController(name)
	if !ok {
		return
	}
	// The default resource group controller should never be deleted.
	if name == defaultResourceGroupName {
		return
	}
	// Try to get the default group meta first.
	defaultGC, err := c.tryGetResourceGroupController(c.loopCtx, defaultResourceGroupName, false)
	if err != nil || defaultGC == nil {
		log.Warn("[resource group controller] get default resource group meta for tombstone failed",
			zap.String("name", name), zap.Error(err))
		// Directly delete the resource group controller if the default group is not available.
		c.cleanupRequestSourceMetricsState(name)
		c.groupsController.Delete(name)
		return
	}
	// Create a default resource group controller for the tombstone resource group independently.
	gc, err := newGroupCostController(
		defaultGC.getMeta(),
		c.ruConfig,
		c.lowTokenNotifyChan,
		c.tokenBucketUpdateChan,
		c.getOrCreateRequestSourceMetricsState(name),
	)
	if err != nil {
		log.Warn("[resource group controller] create default resource group cost controller for tombstone failed",
			zap.String("name", name), zap.Error(err))
		// Directly delete the resource group controller if the default group controller cannot be created.
		c.cleanupRequestSourceMetricsState(name)
		c.groupsController.Delete(name)
		return
	}
	gc.tombstone.Store(true)
	c.groupsController.Store(name, gc)
	// Its metrics will be deleted in the cleanup process.
	metrics.ResourceGroupStatusGauge.WithLabelValues(name, name).Set(2)
	log.Info("[resource group controller] default resource group controller cost created for tombstone",
		zap.String("name", name))
}
```

[client/resource_group/controller/global_controller.go:758](../../../client/resource_group/controller/global_controller.go#L758)

```go
func (c *ResourceGroupsController) handleTokenBucketResponse(resp []*rmpb.TokenBucketResponse) {
	if c.responseDeadlineCh != nil {
		if c.run.responseDeadline.Stop() {
			select {
			case <-c.run.responseDeadline.C:
			default:
			}
		}
		c.responseDeadlineCh = nil
	}
	c.run.inDegradedMode.Store(false)
	for _, res := range resp {
		name := res.GetResourceGroupName()
		gc, ok := c.loadGroupController(name)
		if !ok {
			log.Warn("[resource group controller] a non-existent resource group was found when handle token response", zap.String("name", name))
			continue
		}
		gc.handleTokenBucketResponse(res)
	}
}
```

[client/resource_group/controller/global_controller.go:797](../../../client/resource_group/controller/global_controller.go#L797)

```go
func (c *ResourceGroupsController) sendTokenBucketRequests(ctx context.Context, requests []*rmpb.TokenBucketRequest, source string, notifyMsg notifyMsg) {
	now := time.Now()
	req := &rmpb.TokenBucketsRequest{
		Requests:              requests,
		TargetRequestPeriodMs: uint64(defaultTargetPeriod / time.Millisecond),
		ClientUniqueId:        c.clientUniqueID,
	}
	if c.ruConfig.DegradedModeWaitDuration > 0 && c.responseDeadlineCh == nil {
		c.run.responseDeadline.Reset(c.ruConfig.DegradedModeWaitDuration)
		c.responseDeadlineCh = c.run.responseDeadline.C
	}
	go func() {
		logControllerTrace("[resource group controller] send token bucket request", zap.Time("now", now), zap.Any("req", req.Requests), zap.String("source", source))
		resp, err := c.provider.AcquireTokenBuckets(ctx, req)
		latency := time.Since(now)
		if err != nil {
			// Don't log any errors caused by the stopper canceling the context.
			if !errors.ErrorEqual(err, context.Canceled) {
				log.Warn("[resource group controller] token bucket rpc error", zap.Error(err))
			}
			resp = nil
			metrics.FailedTokenRequestDuration.Observe(latency.Seconds())
		} else {
			metrics.SuccessfulTokenRequestDuration.Observe(latency.Seconds())
		}
		if !notifyMsg.startTime.IsZero() && time.Since(notifyMsg.startTime) > slowNotifyFilterDuration {
			log.Warn("[resource group controller] slow token bucket request", zap.String("source", source), zap.Duration("cost", time.Since(notifyMsg.startTime)))
		}
		logControllerTrace("[resource group controller] token bucket response", zap.Time("now", time.Now()), zap.Any("resp", resp), zap.String("source", source), zap.Duration("latency", latency))
		select {
		case c.tokenResponseChan <- resp:
		case <-ctx.Done():
		}
	}()
}
```

[client/resource_group/controller/global_controller.go:879](../../../client/resource_group/controller/global_controller.go#L879)

```go
func (c *ResourceGroupsController) checkBackgroundSettings(ctx context.Context, bg *rmpb.BackgroundSettings, requestResource string) bool {
	// fallback to default resource group.
	if bg == nil {
		gc, err := c.tryGetResourceGroupController(ctx, defaultResourceGroupName, false)
		if err != nil {
			return false
		}
		bg = gc.getMeta().BackgroundSettings
	}

	if bg == nil || len(requestResource) == 0 || len(bg.JobTypes) == 0 {
		return false
	}

	if idx := strings.LastIndex(requestResource, "_"); idx != -1 {
		return slices.Contains(bg.JobTypes, requestResource[idx+1:])
	}

	return false
}
```

### client/resource_group/controller/group_controller.go — owner 90/91/92

[client/resource_group/controller/group_controller.go:361](../../../client/resource_group/controller/group_controller.go#L361)

```go
func (gc *groupCostController) initRunState() {
	now := time.Now()
	gc.run.now = now
	gc.run.lastRequestTime = now.Add(-defaultTargetPeriod)
	gc.run.targetPeriod = defaultTargetPeriod
	gc.run.consumption = &rmpb.Consumption{}
	gc.run.lastRequestConsumption = &rmpb.Consumption{SqlLayerCpuTimeMs: getSQLProcessCPUTime(gc.mainCfg.isSingleGroupByKeyspace)}

	isBurstable := true
	cfgFunc := func(tb *rmpb.TokenBucket) tokenBucketReconfigureArgs {
		initialToken := float64(tb.Settings.FillRate)
		cfg := tokenBucketReconfigureArgs{
			newTokens: initialToken,
			newBurst:  tb.Settings.BurstLimit,
			// This is to trigger token requests as soon as resource group start consuming tokens.
			newNotifyThreshold: math.Max(initialToken*tokenReserveFraction, 1),
		}
		if cfg.newBurst >= 0 {
			cfg.newBurst = 0
		}
		if tb.Settings.BurstLimit >= 0 {
			isBurstable = false
		}
		return cfg
	}

	gc.metaLock.RLock()
	defer gc.metaLock.RUnlock()
	limiter := NewLimiterWithCfg(gc.name, now, cfgFunc(gc.meta.RUSettings.RU), gc.lowRUNotifyChan)
	counter := &tokenCounter{
		limiter:     limiter,
		avgRUPerSec: 0,
		avgLastTime: now,
		fillRate:    gc.meta.RUSettings.RU.Settings.FillRate,
	}
	gc.run.requestUnitTokens = counter
	gc.burstable.Store(isBurstable)
}
```

[client/resource_group/controller/group_controller.go:507](../../../client/resource_group/controller/group_controller.go#L507)

```go
func (gc *groupCostController) shouldReportConsumption() bool {
	if !gc.initialRequestCompleted.Load() {
		return true
	}
	timeSinceLastRequest := gc.run.now.Sub(gc.run.lastRequestTime)
	failpoint.Inject("acceleratedReportingPeriod", func() {
		timeSinceLastRequest = extendedReportingPeriodFactor * defaultTargetPeriod
	})
	// Due to `gc.run.lastRequestTime` update operations late in this logic,
	// so `timeSinceLastRequest` is less than defaultGroupStateUpdateInterval a little bit, lead to actual report period is greater than defaultTargetPeriod.
	// Add defaultGroupStateUpdateInterval/2 as duration buffer to avoid it.
	if timeSinceLastRequest+defaultGroupStateUpdateInterval/2 >= defaultTargetPeriod {
		if timeSinceLastRequest >= extendedReportingPeriodFactor*defaultTargetPeriod {
			return true
		}
		if getRUValueFromConsumption(gc.run.consumption)-getRUValueFromConsumption(gc.run.lastRequestConsumption) >= consumptionsReportingThreshold {
			return true
		}
	}
	return false
}
```

[client/resource_group/controller/group_controller.go:541](../../../client/resource_group/controller/group_controller.go#L541)

```go
func (gc *groupCostController) modifyTokenCounter(counter *tokenCounter, bucket *rmpb.TokenBucket, trickleTimeMs int64) {
	granted := bucket.GetTokens()
	if !counter.lastDeadline.IsZero() {
		// If last request came with a trickle duration, we may have RUs that were
		// not made available to the bucket yet; throw them together with the newly
		// granted RUs.
		if since := counter.lastDeadline.Sub(gc.run.now); since > 0 {
			granted += counter.lastRate * since.Seconds()
		}
	}
	initCounterNotify(counter)
	counter.inDegradedMode = false
	var cfg tokenBucketReconfigureArgs
	cfg.newBurst = bucket.GetSettings().GetBurstLimit()
	// When trickleTimeMs equals zero, server has enough tokens and does not need to
	// limit client consume token. So all token is granted to client right now.
	if trickleTimeMs == 0 {
		cfg.newTokens = granted
		cfg.newFillRate = float64(bucket.GetSettings().FillRate)
		counter.lastDeadline = time.Time{}
		cfg.newNotifyThreshold = math.Min(granted+counter.limiter.AvailableTokens(gc.run.now), counter.avgRUPerSec*defaultTargetPeriod.Seconds()) * notifyFraction
		if cfg.newBurst < 0 {
			cfg.newTokens = float64(counter.fillRate)
		}
		gc.isThrottled.Store(false)
	} else {
		// Otherwise the granted token is delivered to the client by fill rate.
		cfg.newTokens = 0
		trickleDuration := time.Duration(trickleTimeMs) * time.Millisecond
		deadline := gc.run.now.Add(trickleDuration)
		cfg.newFillRate = float64(bucket.GetSettings().FillRate) + granted/trickleDuration.Seconds()

		timerDuration := trickleDuration - trickleReserveDuration
		if timerDuration <= 0 {
			timerDuration = (trickleDuration + trickleReserveDuration) / 2
		}
		counter.notify.mu.Lock()
		if counter.notify.setupNotificationTimer != nil {
			counter.notify.setupNotificationTimer.Stop()
		}
		counter.notify.setupNotificationTimer = time.NewTimer(timerDuration)
		counter.notify.setupNotificationCh = counter.notify.setupNotificationTimer.C
		counter.notify.cancelCh = make(chan struct{})
		counter.notify.setupNotificationThreshold = 1
		counter.notify.mu.Unlock()
		counter.lastDeadline = deadline
		gc.isThrottled.Store(true)
		select {
		case gc.tokenBucketUpdateChan <- gc:
		default:
		}
	}

	counter.lastRate = cfg.newFillRate
	counter.limiter.Reconfigure(gc.run.now, cfg, resetLowProcess())
}
```

[client/resource_group/controller/group_controller.go:688](../../../client/resource_group/controller/group_controller.go#L688)

```go
func (gc *groupCostController) acquireTokens(ctx context.Context, delta *rmpb.Consumption, waitDuration *time.Duration, allowDebt bool) (time.Duration, error) {
	gc.metrics.runningKVRequestCounter.Inc()
	defer gc.metrics.runningKVRequestCounter.Dec()
	var (
		err error
		d   time.Duration
	)
retryLoop:
	for range gc.mainCfg.WaitRetryTimes {
		counter := gc.run.requestUnitTokens
		reconfiguredCh := counter.limiter.GetReconfiguredCh()
		now := time.Now()
		var res *Reservation
		if v := getRUValueFromConsumption(delta); v > 0 {
			// record the consume token histogram if enable controller debug mode.
			if enableControllerTraceLog.Load() {
				gc.metrics.consumeTokenHistogram.Observe(v)
			}
			// allow debt for small request or not in throttled. remove tokens directly.
			if allowDebt {
				counter.limiter.RemoveTokens(now, v)
				break retryLoop
			}
			res = counter.limiter.Reserve(ctx, gc.mainCfg.LTBMaxWaitDuration, now, v)
		}
		if d, err = WaitReservations(ctx, now, []*Reservation{res}); err == nil || errs.ErrClientResourceGroupThrottled.NotEqual(err) {
			break retryLoop
		}
		gc.metrics.requestRetryCounter.Inc()
		waitStart := time.Now()
		waitTimer := time.NewTimer(gc.mainCfg.WaitRetryInterval)
		select {
		case <-ctx.Done():
			if !waitTimer.Stop() {
				select {
				case <-waitTimer.C:
				default:
				}
			}
			*waitDuration += time.Since(waitStart)
			return d, ctx.Err()
		case <-reconfiguredCh:
			if !waitTimer.Stop() {
				select {
				case <-waitTimer.C:
				default:
				}
			}
		case <-waitTimer.C:
		}
		*waitDuration += time.Since(waitStart)
	}
	return d, err
}
```

### client/resource_group/controller/limiter.go — owner 90/91/92

[client/resource_group/controller/limiter.go:224](../../../client/resource_group/controller/limiter.go#L224)

```go
func (lim *Limiter) Reserve(ctx context.Context, waitDuration time.Duration, now time.Time, n float64) *Reservation {
	// Check if ctx is already cancelled
	select {
	case <-ctx.Done():
		return &Reservation{
			reserved: false,
			err:      ctx.Err(),
		}
	default:
	}
	// Determine wait limit
	waitLimit := waitDuration
	if deadline, ok := ctx.Deadline(); ok {
		waitLimit = deadline.Sub(now)
	}
	r := lim.reserveN(now, n, waitLimit)
	return &r
}
```

[client/resource_group/controller/limiter.go:272](../../../client/resource_group/controller/limiter.go#L272)

```go
func (lim *Limiter) notify() {
	if lim.isLowProcess {
		return
	}
	lim.notifyThreshold = 0
	lim.isLowProcess = true
	select {
	case lim.lowTokensNotifyChan <- notifyMsg{startTime: time.Now()}:
		if lim.metrics != nil {
			lim.metrics.lowTokenNotifyCounter.Inc()
		}
	default:
	}
}
```

### client/resource_group/controller/model.go — owner 90/91/92

[client/resource_group/controller/model.go:246](../../../client/resource_group/controller/model.go#L246)

```go
func (kc *KVCalculator) initCalculation(detail *RUCalculation) {
	if detail != nil {
		detail.Factors = RUFactorSnapshot{
			ReadBaseCost:          float64(kc.ReadBaseCost),
			ReadPerBatchBaseCost:  float64(kc.ReadPerBatchBaseCost),
			ReadBytesCost:         float64(kc.ReadBytesCost),
			WriteBaseCost:         float64(kc.WriteBaseCost),
			WritePerBatchBaseCost: float64(kc.WritePerBatchBaseCost),
			WriteBytesCost:        float64(kc.WriteBytesCost),
			CPUMsCost:             float64(kc.CPUMsCost),
			BatchProportion:       defaultAvgBatchProportion,
		}
	}
}
```

[client/resource_group/controller/model.go:269](../../../client/resource_group/controller/model.go#L269)

```go
func (kc *KVCalculator) beforeKVRequestWithDetail(consumption *rmpb.Consumption, detail *RUCalculation, req RequestInfo) {
	kc.initCalculation(detail)
	if req.IsWrite() {
		consumption.KvWriteRpcCount += 1
		// Write bytes are knowable in advance, so we can calculate the WRU cost here.
		kc.calculateWriteCostWithDetail(consumption, detail, req)
	} else {
		consumption.KvReadRpcCount += 1
		// Read bytes could not be known before the request is executed,
		// so we only add the base cost here.
		consumption.RRU += float64(kc.ReadBaseCost) + float64(kc.ReadPerBatchBaseCost)*defaultAvgBatchProportion
		if detail != nil {
			detail.Inputs.ReadRPCCount++
		}
		// Paging pre-charge
		if bytesForEst := estimatedReadBytes(req); bytesForEst > 0 {
			consumption.RRU += float64(kc.ReadBytesCost) * float64(bytesForEst)
			if detail != nil {
				detail.Inputs.ReadBytes += float64(bytesForEst)
			}
		}
	}
	if req.AccessLocationType() == AccessCrossZone {
		if req.IsWrite() {
			consumption.WriteCrossAzTrafficBytes += req.RequestSize()
		} else {
			consumption.ReadCrossAzTrafficBytes += req.RequestSize()
		}
	}
}
```

[client/resource_group/controller/model.go:300](../../../client/resource_group/controller/model.go#L300)

```go
func (kc *KVCalculator) calculateWriteCostWithDetail(consumption *rmpb.Consumption, detail *RUCalculation, req RequestInfo) {
	writeBytes := float64(req.WriteBytes())
	consumption.WriteBytes += writeBytes
	// write request cost need consider the replicas, due to write data will be replicate to all replicas.
	replicaNums := uint64(req.ReplicaNumber())
	if replicaNums == 0 {
		replicaNums = 1
	}
	replicas := float64(replicaNums)
	consumption.WRU += (float64(kc.WriteBaseCost) + float64(kc.WritePerBatchBaseCost)*defaultAvgBatchProportion + float64(kc.WriteBytesCost)*writeBytes) * replicas
	if detail != nil {
		detail.Inputs.ReplicaWeightedWriteRPCCount += replicas
		detail.Inputs.ReplicaWeightedWriteBytes += writeBytes * replicas
	}
	// TODO: for a raft group with N replicas, we assume the cross AZ network traffic for raft replication
	// is: writeBytes * (N - 1). This is not accurate, but the deviation should be small enough.
	//
	// NOTE: if the access local type is "unknown", we don't count the write cross AZ traffic,
	// as it's likely this cluster is not cross-AZ deployed.
	if req.AccessLocationType() != AccessUnknown {
		consumption.WriteCrossAzTrafficBytes += req.WriteBytes() * (replicaNums - 1)
	}
}
```

[client/resource_group/controller/model.go:414](../../../client/resource_group/controller/model.go#L414)

```go
func adjustPagingPrechargeRRU(calculators []ResourceCalculator, consumption *rmpb.Consumption, req RequestInfo, sign float64) {
	if consumption == nil {
		return
	}
	for _, calc := range calculators {
		kvCalc, ok := calc.(*KVCalculator)
		if !ok {
			continue
		}
		consumption.RRU += sign * kvCalc.pagingPrechargeRRU(req)
	}
}
```

[client/resource_group/controller/model.go:427](../../../client/resource_group/controller/model.go#L427)

```go
func reportedRequestConsumption(calculators []ResourceCalculator, req RequestInfo, tokenDelta *rmpb.Consumption) *rmpb.Consumption {
	if estimatedReadBytes(req) == 0 {
		return tokenDelta
	}
	reported := cloneConsumption(tokenDelta)
	adjustPagingPrechargeRRU(calculators, reported, req, -1)
	return reported
}
```

[client/resource_group/controller/model.go:436](../../../client/resource_group/controller/model.go#L436)

```go
func reportedResponseConsumption(calculators []ResourceCalculator, req RequestInfo, tokenDelta *rmpb.Consumption) *rmpb.Consumption {
	if estimatedReadBytes(req) == 0 {
		return tokenDelta
	}
	reported := cloneConsumption(tokenDelta)
	adjustPagingPrechargeRRU(calculators, reported, req, 1)
	return reported
}
```

### pkg/encryption/config.go — owner 主题8；78/102边界

[pkg/encryption/config.go:26](../../../pkg/encryption/config.go#L26)

```go
const (
	methodPlaintext = "plaintext"
	methodAes128Ctr = "aes128-ctr"
	methodAes192Ctr = "aes192-ctr"
	methodAes256Ctr = "aes256-ctr"

	masterKeyTypePlaintext = "plaintext"
	masterKeyTypeKMS       = "kms"
	masterKeyTypeFile      = "file"

	defaultDataEncryptionMethod  = methodPlaintext
	defaultDataKeyRotationPeriod = "168h" // 7 days
)
```

[pkg/encryption/config.go:51](../../../pkg/encryption/config.go#L51)

```go
func (c *Config) Adjust() error {
	if len(c.DataEncryptionMethod) == 0 {
		c.DataEncryptionMethod = defaultDataEncryptionMethod
	} else if _, err := c.GetMethod(); err != nil {
		return err
	}

	if c.DataKeyRotationPeriod.Duration == 0 {
		duration, err := time.ParseDuration(defaultDataKeyRotationPeriod)
		if err != nil {
			return errs.ErrEncryptionInvalidConfig.Wrap(err).GenWithStack(
				"fail to parse default value of data-key-rotation-period %s",
				defaultDataKeyRotationPeriod)
		}
		c.DataKeyRotationPeriod.Duration = duration
	} else if c.DataKeyRotationPeriod.Duration < 0 {
		return errs.ErrEncryptionInvalidConfig.GenWithStack(
			"negative data-key-rotation-period %d",
			c.DataKeyRotationPeriod.Duration)
	}
	if len(c.MasterKey.Type) == 0 {
		c.MasterKey.Type = masterKeyTypePlaintext
	} else if _, err := c.GetMasterKeyMeta(); err != nil {
		return err
	}
	return nil
}
```

[pkg/encryption/config.go:80](../../../pkg/encryption/config.go#L80)

```go
func (c *Config) GetMethod() (encryptionpb.EncryptionMethod, error) {
	switch c.DataEncryptionMethod {
	case methodPlaintext:
		return encryptionpb.EncryptionMethod_PLAINTEXT, nil
	case methodAes128Ctr:
		return encryptionpb.EncryptionMethod_AES128_CTR, nil
	case methodAes192Ctr:
		return encryptionpb.EncryptionMethod_AES192_CTR, nil
	case methodAes256Ctr:
		return encryptionpb.EncryptionMethod_AES256_CTR, nil
	default:
		return encryptionpb.EncryptionMethod_UNKNOWN,
			errs.ErrEncryptionInvalidMethod.GenWithStack("unknown method")
	}
}
```

[pkg/encryption/config.go:97](../../../pkg/encryption/config.go#L97)

```go
func (c *Config) GetMasterKeyMeta() (*encryptionpb.MasterKey, error) {
	switch c.MasterKey.Type {
	case masterKeyTypePlaintext:
		return &encryptionpb.MasterKey{
			Backend: &encryptionpb.MasterKey_Plaintext{
				Plaintext: &encryptionpb.MasterKeyPlaintext{},
			},
		}, nil
	case masterKeyTypeKMS:
		return &encryptionpb.MasterKey{
			Backend: &encryptionpb.MasterKey_Kms{
				Kms: &encryptionpb.MasterKeyKms{
					Vendor:   kmsVendorAWS,
					KeyId:    c.MasterKey.KmsKeyID,
					Region:   c.MasterKey.KmsRegion,
					Endpoint: c.MasterKey.KmsEndpoint,
				},
			},
		}, nil
	case masterKeyTypeFile:
		return &encryptionpb.MasterKey{
			Backend: &encryptionpb.MasterKey_File{
				File: &encryptionpb.MasterKeyFile{
					Path: c.MasterKey.FilePath,
				},
			},
		}, nil
	default:
		return nil, errs.ErrEncryptionInvalidConfig.GenWithStack(
			"unrecognized encryption master key type: %s", c.MasterKey.Type)
	}
}
```

### pkg/encryption/crypter.go — owner 主题8；78/102边界

[pkg/encryption/crypter.go:37](../../../pkg/encryption/crypter.go#L37)

```go
func CheckEncryptionMethodSupported(method encryptionpb.EncryptionMethod) error {
	switch method {
	case encryptionpb.EncryptionMethod_AES128_CTR:
		return nil
	case encryptionpb.EncryptionMethod_AES192_CTR:
		return nil
	case encryptionpb.EncryptionMethod_AES256_CTR:
		return nil
	default:
		name, ok := encryptionpb.EncryptionMethod_name[int32(method)]
		if ok {
			return errs.ErrEncryptionInvalidMethod.GenWithStackByArgs(name)
		}
		return errs.ErrEncryptionInvalidMethod.GenWithStackByArgs(int32(method))
	}
}
```

[pkg/encryption/crypter.go:55](../../../pkg/encryption/crypter.go#L55)

```go
func KeyLength(method encryptionpb.EncryptionMethod) (int, error) {
	switch method {
	case encryptionpb.EncryptionMethod_AES128_CTR:
		return 16, nil
	case encryptionpb.EncryptionMethod_AES192_CTR:
		return 24, nil
	case encryptionpb.EncryptionMethod_AES256_CTR:
		return 32, nil
	default:
		name, ok := encryptionpb.EncryptionMethod_name[int32(method)]
		if ok {
			return 0, errs.ErrEncryptionInvalidMethod.GenWithStackByArgs(name)
		}
		return 0, errs.ErrEncryptionInvalidMethod.GenWithStackByArgs(int32(method))
	}
}
```

### pkg/encryption/key_manager.go — owner 主题8；78/102边界

[pkg/encryption/key_manager.go:215](../../../pkg/encryption/key_manager.go#L215)

```go
func (m *Manager) StartBackgroundLoop(ctx context.Context) {
	// Setup key dictionary watcher
	watcher := clientv3.NewWatcher(m.etcdClient)
	defer watcher.Close()
	// Check data key rotation every min(dataKeyRotationPeriod, keyRotationCheckPeriod).
	checkPeriod := m.dataKeyRotationPeriod
	if keyRotationCheckPeriod < checkPeriod {
		checkPeriod = keyRotationCheckPeriod
	}
	ticker := time.NewTicker(checkPeriod)
	defer ticker.Stop()

	for {
		var (
			resp clientv3.WatchResponse
			ok   bool
		)
		rch := watcher.Watch(ctx, EncryptionKeysPath, clientv3.WithRev(m.keysRevision()))

	keyWatchLoop:
		for {
			select {
			case resp, ok = <-rch:
				if !ok || resp.Err() != nil {
					// If chan is closed or canceled, exit watch loop
					// Ref https://etcd.io/docs/v3.4/learning/api/#watch-streams
					break keyWatchLoop
				}
				for _, event := range resp.Events {
					if event.Type != mvccpb.PUT {
						log.Warn("encryption keys is deleted unexpectedly")
						continue
					}
					_, err := m.loadKeysFromKV(event.Kv)
					if err != nil {
						log.Warn("fail to get encryption keys from watcher result", errs.ZapError(err))
					}
				}
				m.helper.eventAfterReloadByWatcher()
			case <-m.helper.tick(ticker):
				m.checkOnTick()
				m.helper.eventAfterTicker()
			}
		}

		select {
		case <-ctx.Done():
			// Server shutdown.
			return
		default:
		}

		if resp.CompactRevision != 0 {
			// meet compacted error
			log.Warn("revision has been compacted, the watcher will watch again",
				zap.Int64("revision", m.keysRevision()),
				zap.Int64("compact-revision", resp.CompactRevision))
		} else {
			// other error
			log.Error("encryption key watcher canceled, the watcher will watch again",
				errs.ZapError(errs.ErrEncryptionKeysWatcher, resp.Err()))
		}

		if err := m.loadKeys(); err != nil {
			log.Error("encryption key reload failed", errs.ZapError(err))
		}
	}
}
```

[pkg/encryption/key_manager.go:531](../../../pkg/encryption/key_manager.go#L531)

```go
func defaultKeyManagerHelper() keyManagerHelper {
	return keyManagerHelper{
		now:                          time.Now,
		tick:                         func(ticker *time.Ticker) <-chan time.Time { return ticker.C },
		newMasterKey:                 NewMasterKey,
		eventAfterReloadByWatcher:    func() {},
		eventAfterTicker:             func() {},
		eventAfterLeaderCheckSuccess: func() {},
		eventSaveKeysFailure:         func() {},
	}
}
```

### pkg/mcs/resourcemanager/server/config.go — owner 主题8；78/102边界

[pkg/mcs/resourcemanager/server/config.go:37](../../../pkg/mcs/resourcemanager/server/config.go#L37)

```go
const (
	defaultName             = "resource manager"
	defaultBackendEndpoints = "http://127.0.0.1:2379"
	defaultListenAddr       = "http://127.0.0.1:3379"

	// 1 RU = 8 storage read requests
	defaultReadBaseCost = 1. / 8
	// 1 RU = 2 storage read batch requests
	defaultReadPerBatchBaseCost = 1. / 2
	// 1 RU = 1 storage write request
	defaultWriteBaseCost = 1
	// 1 RU = 1 storage write batch request
	defaultWritePerBatchBaseCost = 1
	// 1 RU = 64 KiB read bytes
	defaultReadCostPerByte = 1. / (64 * 1024)
	// 1 RU = 1 KiB written bytes
	defaultWriteCostPerByte = 1. / 1024
	// 1 RU = 3 millisecond CPU time
	defaultCPUMsCost = 1. / 3

	// Because the resource manager has not been deployed in microservice mode,
	// do not enable this function.
	defaultDegradedModeWaitDuration = time.Second * 0
	// defaultMaxWaitDuration is the max duration to wait for the token before throwing error.
	defaultMaxWaitDuration = 30 * time.Second
	// defaultLTBTokenRPCMaxDelay is the upper bound of backoff delay for local token bucket RPC.
	defaultLTBTokenRPCMaxDelay = 1 * time.Second
)
```

[pkg/mcs/resourcemanager/server/config.go:99](../../../pkg/mcs/resourcemanager/server/config.go#L99)

```go
const (
	// RUVersionV1 is the default RU calculation version.
	RUVersionV1 RUVersion = 1
	// RUVersionV2 is the new RU calculation version with detailed metrics.
	RUVersionV2 RUVersion = 2
)
```

[pkg/mcs/resourcemanager/server/config.go:115](../../../pkg/mcs/resourcemanager/server/config.go#L115)

```go
func (p *RUVersionPolicy) getKeyspaceRUVersion(keyspaceID uint32) RUVersion {
	if p == nil {
		return DefaultRUVersion
	}
	if version, ok := p.Overrides[keyspaceID]; ok {
		return version
	}
	if p.Default > 0 {
		return p.Default
	}
	return DefaultRUVersion
}
```

[pkg/mcs/resourcemanager/server/config.go:129](../../../pkg/mcs/resourcemanager/server/config.go#L129)

```go
func (p *RUVersionPolicy) validate() error {
	if p == nil {
		return nil
	}
	if p.Default <= 0 {
		return fmt.Errorf("ru-version-policy default must be positive, got %d", p.Default)
	}
	for ks, v := range p.Overrides {
		if v <= 0 {
			return fmt.Errorf("ru-version-policy override for keyspace %d must be positive, got %d", ks, v)
		}
	}
	return nil
}
```

[pkg/mcs/resourcemanager/server/config.go:145](../../../pkg/mcs/resourcemanager/server/config.go#L145)

```go
func (p *RUVersionPolicy) Clone() *RUVersionPolicy {
	if p == nil {
		return nil
	}
	clone := &RUVersionPolicy{Default: p.Default}
	if p.Overrides != nil {
		clone.Overrides = make(map[uint32]RUVersion, len(p.Overrides))
		for keyspaceID, version := range p.Overrides {
			clone.Overrides[keyspaceID] = version
		}
	}
	return clone
}
```

[pkg/mcs/resourcemanager/server/config.go:186](../../../pkg/mcs/resourcemanager/server/config.go#L186)

```go
func (rmc *ControllerConfig) getKeyspaceRUVersion(keyspaceID uint32) RUVersion {
	if rmc == nil {
		return DefaultRUVersion
	}
	return rmc.RUVersionPolicy.getKeyspaceRUVersion(keyspaceID)
}
```

[pkg/mcs/resourcemanager/server/config.go:194](../../../pkg/mcs/resourcemanager/server/config.go#L194)

```go
func (rmc *ControllerConfig) Adjust(meta *configutil.ConfigMetaData) {
	if rmc == nil {
		return
	}
	rmc.RequestUnit.Adjust(meta.Child("request-unit"))
	if !meta.IsDefined("degraded-mode-wait-duration") {
		configutil.AdjustDuration(&rmc.DegradedModeWaitDuration, defaultDegradedModeWaitDuration)
	}
	if !meta.IsDefined("ltb-max-wait-duration") {
		configutil.AdjustDuration(&rmc.LTBMaxWaitDuration, defaultMaxWaitDuration)
	}
	if !meta.IsDefined("ltb-token-rpc-max-delay") {
		configutil.AdjustDuration(&rmc.LTBTokenRPCMaxDelay, defaultLTBTokenRPCMaxDelay)
	}
	if rmc.PushMetricsAddress != "" && rmc.PushMetricsInterval.Duration <= 0 {
		log.Warn("push-metrics-address is set but push-metrics-interval is invalid, metrics push disabled")
		rmc.PushMetricsAddress = ""
	}
	failpoint.Inject("enableDegradedModeAndTraceLog", func() {
		configutil.AdjustDuration(&rmc.DegradedModeWaitDuration, time.Second)
		configutil.AdjustBool(&rmc.EnableControllerTraceLog, true)
	})
}
```

[pkg/mcs/resourcemanager/server/config.go:241](../../../pkg/mcs/resourcemanager/server/config.go#L241)

```go
func (ruc *RequestUnitConfig) Adjust(meta *configutil.ConfigMetaData) {
	if ruc == nil {
		return
	}
	if !meta.IsDefined("read-base-cost") {
		configutil.AdjustFloat64(&ruc.ReadBaseCost, defaultReadBaseCost)
	}
	if !meta.IsDefined("read-per-batch-base-cost") {
		configutil.AdjustFloat64(&ruc.ReadPerBatchBaseCost, defaultReadPerBatchBaseCost)
	}
	if !meta.IsDefined("read-cost-per-byte") {
		configutil.AdjustFloat64(&ruc.ReadCostPerByte, defaultReadCostPerByte)
	}
	if !meta.IsDefined("write-base-cost") {
		configutil.AdjustFloat64(&ruc.WriteBaseCost, defaultWriteBaseCost)
	}
	if !meta.IsDefined("write-per-batch-base-cost") {
		configutil.AdjustFloat64(&ruc.WritePerBatchBaseCost, defaultWritePerBatchBaseCost)
	}
	if !meta.IsDefined("write-cost-per-byte") {
		configutil.AdjustFloat64(&ruc.WriteCostPerByte, defaultWriteCostPerByte)
	}
	if !meta.IsDefined("read-cpu-ms-cost") {
		configutil.AdjustFloat64(&ruc.CPUMsCost, defaultCPUMsCost)
	}
}
```

[pkg/mcs/resourcemanager/server/config.go:274](../../../pkg/mcs/resourcemanager/server/config.go#L274)

```go
func (c *Config) Parse(flagSet *pflag.FlagSet) error {
	// Load config file if specified.
	var (
		meta *toml.MetaData
		err  error
	)
	if configFile, _ := flagSet.GetString("config"); configFile != "" {
		meta, err = configutil.ConfigFromFile(c, configFile)
		if err != nil {
			return err
		}
	}

	// Ignore the error check here
	configutil.AdjustCommandLineString(flagSet, &c.Name, "name")
	configutil.AdjustCommandLineString(flagSet, &c.Log.Level, "log-level")
	configutil.AdjustCommandLineString(flagSet, &c.Log.File.Filename, "log-file")
	configutil.AdjustCommandLineString(flagSet, &c.Metric.PushAddress, "metrics-addr")
	configutil.AdjustCommandLineString(flagSet, &c.Security.CAPath, "cacert")
	configutil.AdjustCommandLineString(flagSet, &c.Security.CertPath, "cert")
	configutil.AdjustCommandLineString(flagSet, &c.Security.KeyPath, "key")
	configutil.AdjustCommandLineString(flagSet, &c.BackendEndpoints, "backend-endpoints")
	configutil.AdjustCommandLineString(flagSet, &c.ListenAddr, "listen-addr")
	configutil.AdjustCommandLineString(flagSet, &c.AdvertiseListenAddr, "advertise-listen-addr")

	return c.Adjust(meta)
}
```

[pkg/mcs/resourcemanager/server/config.go:303](../../../pkg/mcs/resourcemanager/server/config.go#L303)

```go
func (c *Config) Adjust(meta *toml.MetaData) error {
	configMetaData := configutil.NewConfigMetadata(meta)
	if err := configMetaData.CheckUndecoded(); err != nil {
		c.WarningMsgs = append(c.WarningMsgs, err.Error())
	}

	if c.Name == "" {
		hostname, err := os.Hostname()
		if err != nil {
			return err
		}
		configutil.AdjustString(&c.Name, fmt.Sprintf("%s-%s", defaultName, hostname))
	}

	configutil.AdjustString(&c.BackendEndpoints, defaultBackendEndpoints)
	configutil.AdjustString(&c.ListenAddr, defaultListenAddr)
	configutil.AdjustString(&c.AdvertiseListenAddr, c.ListenAddr)

	c.adjustLog(configMetaData.Child("log"))
	if err := c.Security.Encryption.Adjust(); err != nil {
		return err
	}

	c.Controller.Adjust(configMetaData.Child("controller"))
	configutil.AdjustInt64(&c.LeaderLease, constant.DefaultLease)

	return nil
}
```

[pkg/mcs/resourcemanager/server/config.go:332](../../../pkg/mcs/resourcemanager/server/config.go#L332)

```go
func (c *Config) adjustLog(meta *configutil.ConfigMetaData) {
	if !meta.IsDefined("disable-error-verbose") {
		c.Log.DisableErrorVerbose = constant.DefaultDisableErrorVerbose
	}
	configutil.AdjustString(&c.Log.Format, constant.DefaultLogFormat)
	configutil.AdjustString(&c.Log.Level, constant.DefaultLogLevel)
}
```

### pkg/mcs/resourcemanager/server/metrics.go — owner 主题8；78/102边界

[pkg/mcs/resourcemanager/server/metrics.go:26](../../../pkg/mcs/resourcemanager/server/metrics.go#L26)

```go
const (
	namespace                 = "resource_manager"
	serverSubsystem           = "server"
	ruSubsystem               = "resource_unit"
	resourceSubsystem         = "resource"
	resourceGroupNameLabel    = "name"
	typeLabel                 = "type"
	readTypeLabel             = "read"
	writeTypeLabel            = "write"
	backgroundTypeLabel       = "background"
	tiflashTypeLabel          = "ap"
	defaultTypeLabel          = "tp"
	newResourceGroupNameLabel = "resource_group"
	keyspaceNameLabel         = "keyspace_name"
	fillRateLabel             = "fill_rate"
	burstLimitLabel           = "burst_limit"

	// Labels for the config.
	ruPerSecLabel   = "ru_per_sec"
	ruCapacityLabel = "ru_capacity"
	priorityLabel   = "priority"
)
```

[pkg/mcs/resourcemanager/server/metrics.go:297](../../../pkg/mcs/resourcemanager/server/metrics.go#L297)

```go
func (m *metrics) getMaxPerSecTracker(keyspaceID uint32, keyspaceName, groupName string) *maxPerSecCostTracker {
	tracker := m.maxPerSecTrackerMap[trackerKey{keyspaceID, groupName}]
	if tracker == nil {
		tracker = newMaxPerSecCostTracker(keyspaceName, groupName, defaultCollectIntervalSec)
		m.maxPerSecTrackerMap[trackerKey{keyspaceID, groupName}] = tracker
	}
	return tracker
}
```

[pkg/mcs/resourcemanager/server/metrics.go:419](../../../pkg/mcs/resourcemanager/server/metrics.go#L419)

```go
func (m *counterMetrics) add(consumption *rmpb.Consumption, controllerConfig *ControllerConfig, keyspaceID uint32) {
	if consumption == nil {
		return
	}
	// RU info.
	if consumption.RRU > 0 {
		m.RRUMetrics.Add(consumption.RRU)
	}
	if consumption.WRU > 0 {
		m.WRUMetrics.Add(consumption.WRU)
	}
	if activeRU := calculateActiveRU(consumption, controllerConfig, keyspaceID); activeRU > 0 {
		m.ActiveRUMetrics.Add(activeRU)
	}
	// RUv2 info (experimental).
	if consumption.TikvRUV2 > 0 {
		m.TiKVRUV2Metrics.Add(consumption.TikvRUV2)
	}
	if consumption.TidbRUV2 > 0 {
		m.TiDBRUV2Metrics.Add(consumption.TidbRUV2)
	}
	if consumption.TiflashRUV2 > 0 {
		m.TiFlashRUV2Metrics.Add(consumption.TiflashRUV2)
	}
	if ruv2 := consumption.TikvRUV2 + consumption.TidbRUV2 + consumption.TiflashRUV2; ruv2 > 0 {
		m.TotalRUV2Metrics.Add(ruv2)
	}
	// Byte info.
	if consumption.ReadBytes > 0 {
		m.ReadByteMetrics.Add(consumption.ReadBytes)
	}
	if consumption.WriteBytes > 0 {
		m.WriteByteMetrics.Add(consumption.WriteBytes)
	}
	// CPU time info.
	if consumption.TotalCpuTimeMs > 0 {
		if sqlRU := calculateSQLRU(consumption, controllerConfig); sqlRU > 0 {
			m.SQLLayerRUMetrics.Add(sqlRU)
			m.SQLCPUMetrics.Add(consumption.SqlLayerCpuTimeMs)
		}
		// A well-behaved client keeps TotalCpuTimeMs >= SqlLayerCpuTimeMs, but the
		// values are reported by the client and not validated here. Guard against a
		// negative KV CPU time so a malformed report cannot panic prometheus.Counter.Add.
		if kvCPUTimeMs := consumption.TotalCpuTimeMs - consumption.SqlLayerCpuTimeMs; kvCPUTimeMs > 0 {
			m.KvCPUMetrics.Add(kvCPUTimeMs)
		}
	}
	// RPC count info.
	if consumption.KvReadRpcCount > 0 {
		m.ReadRequestCountMetrics.Add(consumption.KvReadRpcCount)
	}
	if consumption.KvWriteRpcCount > 0 {
		m.WriteRequestCountMetrics.Add(consumption.KvWriteRpcCount)
	}
	if consumption.ReadCrossAzTrafficBytes > 0 {
		m.ReadCrossAZTrafficMetrics.Add(float64(consumption.ReadCrossAzTrafficBytes))
	}
	if consumption.WriteCrossAzTrafficBytes > 0 {
		m.WriteCrossAZTrafficMetrics.Add(float64(consumption.WriteCrossAzTrafficBytes))
	}
}
```

[pkg/mcs/resourcemanager/server/metrics.go:503](../../../pkg/mcs/resourcemanager/server/metrics.go#L503)

```go
func (m *gaugeMetrics) setGroup(group *ResourceGroup, keyspaceName string) {
	// skip basic metrics for the default group.
	if group.Name != DefaultResourceGroupName {
		ru := math.Max(group.getRUToken(), 0)
		m.availableRUCounter.Set(ru)
		m.priorityResourceGroupConfigGauge.Set(group.getPriority())
		m.ruPerSecResourceGroupConfigGauge.Set(group.getFillRate(true))
		m.ruCapacityResourceGroupConfigGauge.Set(float64(group.getBurstLimit(true)))
	}

	// Set the override fill rate and burst limit and delete the metrics if the override is not set.
	overrideFillRate := group.getOverrideFillRate()
	if overrideFillRate == -1 {
		overrideSettings.DeleteLabelValues(group.Name, keyspaceName, fillRateLabel)
	} else {
		m.overrideFillRateGauge.Set(overrideFillRate)
	}
	overrideBurstLimit := group.getOverrideBurstLimit()
	if overrideBurstLimit == -1 {
		overrideSettings.DeleteLabelValues(group.Name, keyspaceName, burstLimitLabel)
	} else {
		m.overrideBurstLimitGauge.Set(float64(overrideBurstLimit))
	}
}
```

### pkg/ratelimit/runner.go — owner 90/91/92

[pkg/ratelimit/runner.go:155](../../../pkg/ratelimit/runner.go#L155)

```go
func (cr *ConcurrentRunner) run(ctx context.Context, task *Task, token *TaskToken) {
	start := time.Now()
	select {
	case <-ctx.Done():
		return
	default:
	}
	task.f(ctx)
	if token != nil {
		cr.limiter.ReleaseToken(token)
		cr.processPendingTasks()
	}
	runnerTaskExecutionDuration.WithLabelValues(cr.name, task.name).Observe(time.Since(start).Seconds())
	runnerSucceededTasks.WithLabelValues(cr.name, task.name).Inc()
}
```

[pkg/ratelimit/runner.go:171](../../../pkg/ratelimit/runner.go#L171)

```go
func (cr *ConcurrentRunner) processPendingTasks() {
	cr.pendingMu.Lock()
	defer cr.pendingMu.Unlock()
	if cr.pendingTaskNum() > 0 {
		task := cr.pendingTasks[cr.pendingHead]
		select {
		case cr.taskChan <- task:
			cr.pendingTaskCount[task.name]--
			delete(cr.existTasks, taskID{id: task.id, name: task.name})
			cr.popPendingTask()
		default:
		}
		return
	}
}
```

### pkg/schedule/config/config.go — owner 48

[pkg/schedule/config/config.go:33](../../../pkg/schedule/config/config.go#L33)

```go
const (
	// DefaultMaxReplicas is the default number of replicas for each region.
	DefaultMaxReplicas         = 3
	defaultMaxSnapshotCount    = 64
	defaultMaxPendingPeerCount = 64
	// defaultMaxMergeRegionSize is the default maximum size of region when regions can be merged.
	// After https://github.com/tikv/tikv/issues/17309, the default value is enlarged from 20 to 54,
	// to make it compatible with the default value of region size of tikv.
	defaultMaxMergeRegionSize        = 54
	defaultLeaderScheduleLimit       = 4
	defaultRegionScheduleLimit       = 2048
	defaultWitnessScheduleLimit      = 4
	defaultReplicaScheduleLimit      = 64
	defaultMergeScheduleLimit        = 8
	defaultHotRegionScheduleLimit    = 4
	defaultAffinityScheduleLimit     = 0 // default to disable
	defaultSplitScatterScheduleLimit = 4
	defaultTolerantSizeRatio         = 0
	defaultLowSpaceRatio             = 0.8
	defaultHighSpaceRatio            = 0.7
	// defaultHotRegionCacheHitsThreshold is the low hit number threshold of the
	// hot region.
	defaultHotRegionCacheHitsThreshold = 3
	defaultSchedulerMaxWaitingOperator = 5
	defaultHotRegionsReservedDays      = 7
	// When a slow store affected more than 30% of total stores, it will trigger evicting.
	defaultSlowStoreEvictingAffectedStoreRatioThreshold = 0.3
	defaultMaxMovableHotPeerSize                        = int64(512)

	defaultEnableJointConsensus            = true
	defaultEnableTiKVSplitRegion           = true
	defaultEnableHeartbeatBreakdownMetrics = true
	defaultEnableHeartbeatConcurrentRunner = true
	defaultEnableCrossTableMerge           = true
	defaultEnableDiagnostic                = true
	defaultStrictlyMatchLabel              = false
	defaultEnablePlacementRules            = true
	defaultEnableWitness                   = false
	defaultHaltScheduling                  = false

	defaultRegionScoreFormulaVersion = "v2"
	defaultLeaderSchedulePolicy      = "count"
	defaultStoreLimitVersion         = "v1"
	defaultPatrolRegionWorkerCount   = 1
	maxPatrolRegionWorkerCount       = 8

	// DefaultSplitMergeInterval is the default value of config split merge interval.
	DefaultSplitMergeInterval      = time.Hour
	defaultSwitchWitnessInterval   = time.Hour
	defaultPatrolRegionInterval    = 10 * time.Millisecond
	defaultMaxStoreDownTime        = 30 * time.Minute
	defaultHotRegionsWriteInterval = 10 * time.Minute
	// It means we skip the preparing stage after the 48 hours no matter if the store has finished preparing stage.
	defaultMaxStorePreparingTime = 48 * time.Hour

	// defaultMaxAffinityMergeRegionSize is the default maximum size of affinity region when regions can be merged.
	// It means 256 MB, according https://docs.pingcap.com/tidb/stable/tune-region-performance/#use-region-split-size-to-adjust-region-
	// The maximum size of region maybe be larger than 256 MB, such as 512 MB or 1 GB.
	// Because we will merge affinity region as far as possible, so defaultMaxAffinityMergeRegionSize is larger than defaultMaxMergeRegionSize.
	defaultMaxAffinityMergeRegionSize = 256

	// RegionSizeToKeysRatio is the ratio between region size and region keys.
	RegionSizeToKeysRatio = 10000
)
```

[pkg/schedule/config/config.go:98](../../../pkg/schedule/config/config.go#L98)

```go
var (
	defaultLocationLabels = []string{}
	// DefaultStoreLimit is the default store limit.
	DefaultStoreLimit = StoreLimit{AddPeer: 15, RemovePeer: 15, TransferLeaderIn: storelimit.Unlimited}
	// DefaultTiFlashStoreLimit is the default TiFlash store limit.
	DefaultTiFlashStoreLimit = StoreLimit{AddPeer: 30, RemovePeer: 30, TransferLeaderIn: storelimit.Unlimited}
)
```

[pkg/schedule/config/config.go:107](../../../pkg/schedule/config/config.go#L107)

```go
const (
	// TTLConfigPrefix is the prefix of the config item that needs to set TTL.
	TTLConfigPrefix = "/config/ttl"

	MaxSnapshotCountKey            = "schedule.max-snapshot-count"
	MaxMergeRegionSizeKey          = "schedule.max-merge-region-size"
	MaxPendingPeerCountKey         = "schedule.max-pending-peer-count"
	MaxMergeRegionKeysKey          = "schedule.max-merge-region-keys"
	LeaderScheduleLimitKey         = "schedule.leader-schedule-limit"
	RegionScheduleLimitKey         = "schedule.region-schedule-limit"
	WitnessScheduleLimitKey        = "schedule.witness-schedule-limit"
	ReplicaRescheduleLimitKey      = "schedule.replica-schedule-limit"
	MergeScheduleLimitKey          = "schedule.merge-schedule-limit"
	HotRegionScheduleLimitKey      = "schedule.hot-region-schedule-limit"
	AffinityScheduleLimitKey       = "schedule.affinity-schedule-limit"
	SplitScatterScheduleLimitKey   = "schedule.split-scatter-schedule-limit"
	SchedulerMaxWaitingOperatorKey = "schedule.scheduler-max-waiting-operator"
	EnableLocationReplacement      = "schedule.enable-location-replacement"

	// EnableTiKVSplitRegion is the option to enable tikv split region.
	// it's related to schedule, but it's not an explicit config
	EnableTiKVSplitRegion = "schedule.enable-tikv-split-region"

	DefaultGCInterval = 5 * time.Second
	DefaultTTL        = 5 * time.Minute
)
```

[pkg/schedule/config/config.go:146](../../../pkg/schedule/config/config.go#L146)

```go
func (sl *StoreLimit) SetDefaultStoreLimit(typ storelimit.Type, ratePerMin float64) {
	sl.mu.Lock()
	defer sl.mu.Unlock()
	switch typ {
	case storelimit.AddPeer:
		sl.AddPeer = ratePerMin
	case storelimit.RemovePeer:
		sl.RemovePeer = ratePerMin
	case storelimit.TransferLeaderIn:
		sl.TransferLeaderIn = ratePerMin
	}
}
```

[pkg/schedule/config/config.go:160](../../../pkg/schedule/config/config.go#L160)

```go
func (sl *StoreLimit) GetDefaultStoreLimit(typ storelimit.Type) float64 {
	sl.mu.RLock()
	defer sl.mu.RUnlock()
	switch typ {
	case storelimit.AddPeer:
		return sl.AddPeer
	case storelimit.RemovePeer:
		return sl.RemovePeer
	case storelimit.TransferLeaderIn:
		return sl.TransferLeaderIn
	default:
		panic("invalid type")
	}
}
```

[pkg/schedule/config/config.go:175](../../../pkg/schedule/config/config.go#L175)

```go
func adjustSchedulers(v *SchedulerConfigs, defValue SchedulerConfigs) {
	if len(*v) == 0 {
		// Make a copy to avoid changing DefaultSchedulers unexpectedly.
		// When reloading from storage, the config is passed to json.Unmarshal.
		// Without clone, the DefaultSchedulers could be overwritten.
		*v = append(defValue[:0:0], defValue...)
	}
}
```

[pkg/schedule/config/config.go:367](../../../pkg/schedule/config/config.go#L367)

```go
func (c *ScheduleConfig) Adjust(meta *configutil.ConfigMetaData, reloading bool) error {
	if !meta.IsDefined("max-snapshot-count") {
		configutil.AdjustUint64(&c.MaxSnapshotCount, defaultMaxSnapshotCount)
	}
	if !meta.IsDefined("max-pending-peer-count") {
		configutil.AdjustUint64(&c.MaxPendingPeerCount, defaultMaxPendingPeerCount)
	}
	if !meta.IsDefined("max-merge-region-size") {
		configutil.AdjustUint64(&c.MaxMergeRegionSize, defaultMaxMergeRegionSize)
	}
	if !meta.IsDefined("max-affinity-merge-region-size") {
		configutil.AdjustUint64(&c.MaxAffinityMergeRegionSize, defaultMaxAffinityMergeRegionSize)
	}
	configutil.AdjustDuration(&c.SplitMergeInterval, DefaultSplitMergeInterval)
	configutil.AdjustDuration(&c.SwitchWitnessInterval, defaultSwitchWitnessInterval)
	configutil.AdjustDuration(&c.PatrolRegionInterval, defaultPatrolRegionInterval)
	configutil.AdjustDuration(&c.MaxStoreDownTime, defaultMaxStoreDownTime)
	configutil.AdjustDuration(&c.HotRegionsWriteInterval, defaultHotRegionsWriteInterval)
	if !meta.IsDefined("max-store-preparing-time") {
		configutil.AdjustDuration(&c.MaxStorePreparingTime, defaultMaxStorePreparingTime)
	}

	if !meta.IsDefined("leader-schedule-limit") {
		configutil.AdjustUint64(&c.LeaderScheduleLimit, defaultLeaderScheduleLimit)
	}
	if !meta.IsDefined("region-schedule-limit") {
		configutil.AdjustUint64(&c.RegionScheduleLimit, defaultRegionScheduleLimit)
	}
	if !meta.IsDefined("witness-schedule-limit") {
		configutil.AdjustUint64(&c.WitnessScheduleLimit, defaultWitnessScheduleLimit)
	}
	if !meta.IsDefined("replica-schedule-limit") {
		configutil.AdjustUint64(&c.ReplicaScheduleLimit, defaultReplicaScheduleLimit)
	}
	if !meta.IsDefined("merge-schedule-limit") {
		configutil.AdjustUint64(&c.MergeScheduleLimit, defaultMergeScheduleLimit)
	}
	if !meta.IsDefined("hot-region-schedule-limit") {
		configutil.AdjustUint64(&c.HotRegionScheduleLimit, defaultHotRegionScheduleLimit)
	}
	if !meta.IsDefined("affinity-schedule-limit") {
		configutil.AdjustUint64(&c.AffinityScheduleLimit, defaultAffinityScheduleLimit)
	}
	if !meta.IsDefined("split-scatter-schedule-limit") {
		configutil.AdjustUint64(&c.SplitScatterScheduleLimit, defaultSplitScatterScheduleLimit)
	}
	if !meta.IsDefined("hot-region-cache-hits-threshold") {
		configutil.AdjustUint64(&c.HotRegionCacheHitsThreshold, defaultHotRegionCacheHitsThreshold)
	}
	if !meta.IsDefined("tolerant-size-ratio") {
		configutil.AdjustFloat64(&c.TolerantSizeRatio, defaultTolerantSizeRatio)
	}
	if !meta.IsDefined("scheduler-max-waiting-operator") {
		configutil.AdjustUint64(&c.SchedulerMaxWaitingOperator, defaultSchedulerMaxWaitingOperator)
	}
	if !meta.IsDefined("leader-schedule-policy") {
		configutil.AdjustString(&c.LeaderSchedulePolicy, defaultLeaderSchedulePolicy)
	}
	if !meta.IsDefined("store-limit-version") {
		configutil.AdjustString(&c.StoreLimitVersion, defaultStoreLimitVersion)
	}
	if !meta.IsDefined("patrol-region-worker-count") {
		configutil.AdjustInt(&c.PatrolRegionWorkerCount, defaultPatrolRegionWorkerCount)
	}

	if !meta.IsDefined("enable-joint-consensus") {
		c.EnableJointConsensus = defaultEnableJointConsensus
	}
	if !meta.IsDefined("enable-tikv-split-region") {
		c.EnableTiKVSplitRegion = defaultEnableTiKVSplitRegion
	}

	if !meta.IsDefined("enable-heartbeat-breakdown-metrics") {
		c.EnableHeartbeatBreakdownMetrics = defaultEnableHeartbeatBreakdownMetrics
	}

	if !meta.IsDefined("enable-heartbeat-concurrent-runner") {
		c.EnableHeartbeatConcurrentRunner = defaultEnableHeartbeatConcurrentRunner
	}

	if !meta.IsDefined("enable-cross-table-merge") {
		c.EnableCrossTableMerge = defaultEnableCrossTableMerge
	}
	configutil.AdjustFloat64(&c.LowSpaceRatio, defaultLowSpaceRatio)
	configutil.AdjustFloat64(&c.HighSpaceRatio, defaultHighSpaceRatio)
	if !meta.IsDefined("enable-diagnostic") {
		c.EnableDiagnostic = defaultEnableDiagnostic
	}

	if !meta.IsDefined("enable-witness") {
		c.EnableWitness = defaultEnableWitness
	}

	// new cluster:v2, old cluster:v1
	if !meta.IsDefined("region-score-formula-version") && !reloading {
		configutil.AdjustString(&c.RegionScoreFormulaVersion, defaultRegionScoreFormulaVersion)
	}

	if !meta.IsDefined("halt-scheduling") {
		c.HaltScheduling = defaultHaltScheduling
	}

	adjustSchedulers(&c.Schedulers, DefaultSchedulers)
	defaultStoreLimitMeta := meta.Child("default-store-limit")
	c.migrateStoreBalanceRate(defaultStoreLimitMeta)
	c.adjustDefaultStoreLimit(defaultStoreLimitMeta)

	for k, b := range c.migrateConfigurationMap() {
		v, err := parseDeprecatedFlag(meta, k, *b[0], *b[1])
		if err != nil {
			return err
		}
		*b[0], *b[1] = false, v // reset old flag false to make it ignored when marshal to JSON
	}

	if c.StoreLimit == nil {
		c.StoreLimit = make(map[uint64]StoreLimitConfig)
	}

	if !meta.IsDefined("hot-regions-reserved-days") {
		configutil.AdjustUint64(&c.HotRegionsReservedDays, defaultHotRegionsReservedDays)
	}

	if !meta.IsDefined("max-movable-hot-peer-size") {
		configutil.AdjustInt64(&c.MaxMovableHotPeerSize, defaultMaxMovableHotPeerSize)
	}

	if !meta.IsDefined("slow-store-evicting-affected-store-ratio-threshold") {
		configutil.AdjustFloat64(&c.SlowStoreEvictingAffectedStoreRatioThreshold, defaultSlowStoreEvictingAffectedStoreRatioThreshold)
	}
	return c.Validate()
}
```

[pkg/schedule/config/config.go:500](../../../pkg/schedule/config/config.go#L500)

```go
func (c *ScheduleConfig) adjustDefaultStoreLimit(meta *configutil.ConfigMetaData) {
	defaultStoreLimit := DefaultStoreLimitConfig()
	if !meta.IsDefined("add-peer") {
		configutil.AdjustFloat64(&c.DefaultStoreLimit.AddPeer, defaultStoreLimit.AddPeer)
	}
	if !meta.IsDefined("remove-peer") {
		configutil.AdjustFloat64(&c.DefaultStoreLimit.RemovePeer, defaultStoreLimit.RemovePeer)
	}
	if !meta.IsDefined("transfer-leader-in") {
		configutil.AdjustFloat64(&c.DefaultStoreLimit.TransferLeaderIn, defaultStoreLimit.TransferLeaderIn)
	}
}
```

[pkg/schedule/config/config.go:513](../../../pkg/schedule/config/config.go#L513)

```go
func (c *ScheduleConfig) migrateStoreBalanceRate(defaultStoreLimitMeta *configutil.ConfigMetaData) {
	if c.StoreBalanceRate == 0 {
		return
	}
	defaultStoreLimit := StoreLimitConfig{AddPeer: c.StoreBalanceRate, RemovePeer: c.StoreBalanceRate}
	if !defaultStoreLimitMeta.IsDefined("add-peer") {
		c.DefaultStoreLimit.AddPeer = defaultStoreLimit.AddPeer
	}
	if !defaultStoreLimitMeta.IsDefined("remove-peer") {
		c.DefaultStoreLimit.RemovePeer = defaultStoreLimit.RemovePeer
	}
	DefaultStoreLimit.SetDefaultStoreLimit(storelimit.AddPeer, c.DefaultStoreLimit.AddPeer)
	DefaultStoreLimit.SetDefaultStoreLimit(storelimit.RemovePeer, c.DefaultStoreLimit.RemovePeer)
	c.StoreBalanceRate = 0
}
```

[pkg/schedule/config/config.go:529](../../../pkg/schedule/config/config.go#L529)

```go
func (c *ScheduleConfig) migratePersistedStoreLimit(addPeerDefined, removePeerDefined, transferLeaderInDefined bool) {
	defaultStoreLimit := DefaultStoreLimitConfig()
	if c.StoreBalanceRate != 0 {
		defaultStoreLimit.AddPeer = c.StoreBalanceRate
		defaultStoreLimit.RemovePeer = c.StoreBalanceRate
	}
	if !addPeerDefined {
		c.DefaultStoreLimit.AddPeer = defaultStoreLimit.AddPeer
	}
	if !removePeerDefined {
		c.DefaultStoreLimit.RemovePeer = defaultStoreLimit.RemovePeer
	}
	if !transferLeaderInDefined {
		c.DefaultStoreLimit.TransferLeaderIn = defaultStoreLimit.TransferLeaderIn
	}
	DefaultStoreLimit.SetDefaultStoreLimit(storelimit.AddPeer, c.DefaultStoreLimit.AddPeer)
	DefaultStoreLimit.SetDefaultStoreLimit(storelimit.RemovePeer, c.DefaultStoreLimit.RemovePeer)
	DefaultStoreLimit.SetDefaultStoreLimit(storelimit.TransferLeaderIn, c.DefaultStoreLimit.TransferLeaderIn)
	c.StoreBalanceRate = 0
}
```

[pkg/schedule/config/config.go:575](../../../pkg/schedule/config/config.go#L575)

```go
func parseDeprecatedFlag(meta *configutil.ConfigMetaData, name string, old, new bool) (bool, error) {
	oldName, newName := "disable-"+name, "enable-"+name
	defineOld, defineNew := meta.IsDefined(oldName), meta.IsDefined(newName)
	switch {
	case defineNew && defineOld:
		if new == old {
			return false, errors.Errorf("config item %s and %s(deprecated) are conflict", newName, oldName)
		}
		return new, nil
	case defineNew && !defineOld:
		return new, nil
	case !defineNew && defineOld:
		return !old, nil // use !disable-*
	case !defineNew && !defineOld:
		return true, nil // use default value true
	}
	return false, nil // unreachable.
}
```

[pkg/schedule/config/config.go:596](../../../pkg/schedule/config/config.go#L596)

```go
func (c *ScheduleConfig) UnmarshalJSON(data []byte) error {
	type scheduleConfig ScheduleConfig
	fields := struct {
		*scheduleConfig
		StoreLimit map[uint64]json.RawMessage `json:"store-limit"`
	}{scheduleConfig: (*scheduleConfig)(c)}
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	if c.StoreLimit == nil && fields.StoreLimit != nil {
		c.StoreLimit = make(map[uint64]StoreLimitConfig, len(fields.StoreLimit))
	}
	for storeID, data := range fields.StoreLimit {
		limit, ok := c.StoreLimit[storeID]
		if !ok {
			limit = c.DefaultStoreLimit
		}
		// Decoding into the existing value also preserves fields sent as null.
		if err := json.Unmarshal(data, &limit); err != nil {
			return err
		}
		c.StoreLimit[storeID] = limit
	}
	return nil
}
```

[pkg/schedule/config/config.go:623](../../../pkg/schedule/config/config.go#L623)

```go
func (c *ScheduleConfig) MigrateDeprecatedFlags() {
	c.applyDeprecatedFlagMigration(true, true, true)
}
```

[pkg/schedule/config/config.go:629](../../../pkg/schedule/config/config.go#L629)

```go
func (c *ScheduleConfig) MigrateDeprecatedFlagsFromJSON(data []byte) error {
	var fields struct {
		StoreLimit        map[uint64]json.RawMessage `json:"store-limit"`
		DefaultStoreLimit struct {
			AddPeer          *float64 `json:"add-peer"`
			RemovePeer       *float64 `json:"remove-peer"`
			TransferLeaderIn *float64 `json:"transfer-leader-in"`
		} `json:"default-store-limit"`
	}
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	c.applyDeprecatedFlagMigration(fields.DefaultStoreLimit.AddPeer != nil,
		fields.DefaultStoreLimit.RemovePeer != nil, fields.DefaultStoreLimit.TransferLeaderIn != nil)
	// The migration may have changed the default used during JSON decoding.
	for storeID, data := range fields.StoreLimit {
		// Full legacy snapshots used zero for omitted peer limits. Only the new
		// leader limit needs a migration default; API patches preserve all fields.
		limit := StoreLimitConfig{TransferLeaderIn: c.DefaultStoreLimit.TransferLeaderIn}
		if err := json.Unmarshal(data, &limit); err != nil {
			return err
		}
		c.StoreLimit[storeID] = limit
	}
	return nil
}
```

[pkg/schedule/config/config.go:656](../../../pkg/schedule/config/config.go#L656)

```go
func (c *ScheduleConfig) applyDeprecatedFlagMigration(addPeerDefined, removePeerDefined, transferLeaderInDefined bool) {
	c.DisableLearner = false
	c.migratePersistedStoreLimit(addPeerDefined, removePeerDefined, transferLeaderInDefined)
	for _, b := range c.migrateConfigurationMap() {
		// If old=false (previously disabled), set both old and new to false.
		if *b[0] {
			*b[0], *b[1] = false, false
		}
	}
}
```

[pkg/schedule/config/config.go:668](../../../pkg/schedule/config/config.go#L668)

```go
func (c *ScheduleConfig) Validate() error {
	if !isStoreLimitRateValid(c.DefaultStoreLimit.AddPeer) {
		return errors.New("default-store-limit.add-peer should be finite and non-negative")
	}
	if !isStoreLimitRateValid(c.DefaultStoreLimit.RemovePeer) {
		return errors.New("default-store-limit.remove-peer should be finite and non-negative")
	}
	if !isStoreLimitRateValid(c.DefaultStoreLimit.TransferLeaderIn) {
		return errors.New("default-store-limit.transfer-leader-in should be finite and non-negative")
	}
	for storeID, limit := range c.StoreLimit {
		if !isStoreLimitRateValid(limit.AddPeer) {
			return errors.Errorf("store-limit[%d].add-peer should be finite and non-negative", storeID)
		}
		if !isStoreLimitRateValid(limit.RemovePeer) {
			return errors.Errorf("store-limit[%d].remove-peer should be finite and non-negative", storeID)
		}
		if !isStoreLimitRateValid(limit.TransferLeaderIn) {
			return errors.Errorf("store-limit[%d].transfer-leader-in should be finite and non-negative", storeID)
		}
	}
	if c.TolerantSizeRatio < 0 {
		return errors.New("tolerant-size-ratio should be non-negative")
	}
	if c.LowSpaceRatio < 0 || c.LowSpaceRatio > 1 {
		return errors.New("low-space-ratio should between 0 and 1")
	}
	if c.HighSpaceRatio < 0 || c.HighSpaceRatio > 1 {
		return errors.New("high-space-ratio should between 0 and 1")
	}
	if c.LowSpaceRatio <= c.HighSpaceRatio {
		return errors.New("low-space-ratio should be larger than high-space-ratio")
	}
	if c.LeaderSchedulePolicy != "count" && c.LeaderSchedulePolicy != "size" {
		return errors.Errorf("leader-schedule-policy %v is invalid", c.LeaderSchedulePolicy)
	}
	if c.SlowStoreEvictingAffectedStoreRatioThreshold == 0 {
		return errors.Errorf("slow-store-evicting-affected-store-ratio-threshold is not set")
	}
	if c.PatrolRegionWorkerCount > maxPatrolRegionWorkerCount || c.PatrolRegionWorkerCount < 1 {
		return errors.Errorf("patrol-region-worker-count should be between 1 and %d", maxPatrolRegionWorkerCount)
	}
	return nil
}
```

[pkg/schedule/config/config.go:718](../../../pkg/schedule/config/config.go#L718)

```go
func (c *ScheduleConfig) Deprecated() error {
	if c.DisableLearner {
		return errors.New("disable-raft-learner has already been deprecated")
	}
	if c.DisableRemoveDownReplica {
		return errors.New("disable-remove-down-replica has already been deprecated")
	}
	if c.DisableReplaceOfflineReplica {
		return errors.New("disable-replace-offline-replica has already been deprecated")
	}
	if c.DisableMakeUpReplica {
		return errors.New("disable-make-up-replica has already been deprecated")
	}
	if c.DisableRemoveExtraReplica {
		return errors.New("disable-remove-extra-replica has already been deprecated")
	}
	if c.DisableLocationReplacement {
		return errors.New("disable-location-replacement has already been deprecated")
	}
	if c.StoreBalanceRate != 0 {
		return errors.New("store-balance-rate has already been deprecated")
	}
	return nil
}
```

[pkg/schedule/config/config.go:766](../../../pkg/schedule/config/config.go#L766)

```go
func DefaultStoreLimitConfig() StoreLimitConfig {
	return StoreLimitConfig{
		AddPeer:          DefaultStoreLimit.GetDefaultStoreLimit(storelimit.AddPeer),
		RemovePeer:       DefaultStoreLimit.GetDefaultStoreLimit(storelimit.RemovePeer),
		TransferLeaderIn: DefaultStoreLimit.GetDefaultStoreLimit(storelimit.TransferLeaderIn),
	}
}
```

[pkg/schedule/config/config.go:775](../../../pkg/schedule/config/config.go#L775)

```go
func (c *ScheduleConfig) GetDefaultStoreLimit() StoreLimitConfig {
	return c.DefaultStoreLimit
}
```

[pkg/schedule/config/config.go:808](../../../pkg/schedule/config/config.go#L808)

```go
func IsDefaultScheduler(typ string) bool {
	for _, c := range DefaultSchedulers {
		if typ == c.Type {
			return true
		}
	}
	return false
}
```

[pkg/schedule/config/config.go:857](../../../pkg/schedule/config/config.go#L857)

```go
func (c *ReplicationConfig) Validate() error {
	foundIsolationLevel := false
	for _, label := range c.LocationLabels {
		err := ValidateLabels([]*metapb.StoreLabel{{Key: label}})
		if err != nil {
			return err
		}
		// IsolationLevel should be empty or one of LocationLabels
		if !foundIsolationLevel && label == c.IsolationLevel {
			foundIsolationLevel = true
		}
	}
	if c.IsolationLevel != "" && !foundIsolationLevel {
		return errors.New("isolation-level must be one of location-labels or empty")
	}
	return nil
}
```

[pkg/schedule/config/config.go:876](../../../pkg/schedule/config/config.go#L876)

```go
func (c *ReplicationConfig) Adjust(meta *configutil.ConfigMetaData) error {
	configutil.AdjustUint64(&c.MaxReplicas, DefaultMaxReplicas)
	if !meta.IsDefined("enable-placement-rules") {
		c.EnablePlacementRules = defaultEnablePlacementRules
	}
	if !meta.IsDefined("strictly-match-label") {
		c.StrictlyMatchLabel = defaultStrictlyMatchLabel
	}
	if !meta.IsDefined("location-labels") {
		c.LocationLabels = defaultLocationLabels
	}
	return c.Validate()
}
```

### pkg/schedule/config/store_config.go — owner 48

[pkg/schedule/config/store_config.go:29](../../../pkg/schedule/config/store_config.go#L29)

```go
var (
	// default region max size is 144MB
	defaultRegionMaxSize = uint64(144)
	// default region split size is 96MB
	defaultRegionSplitSize = uint64(96)
	// default bucket size is 96MB
	defaultBucketSize = uint64(96)
	// default region max key is 144000
	defaultRegionMaxKey = uint64(1440000)
	// default region split key is 960000
	defaultRegionSplitKey = uint64(960000)
	// RaftstoreV2 is the v2 raftstore engine mark.
	RaftstoreV2 = "raft-kv2"
)
```

[pkg/schedule/config/store_config.go:85](../../../pkg/schedule/config/store_config.go#L85)

```go
func (c *StoreConfig) Adjust() {
	if c == nil {
		return
	}

	c.RegionMaxSizeMB = typeutil.ParseMBFromText(c.RegionMaxSize, defaultRegionMaxSize)
	c.RegionSplitSizeMB = typeutil.ParseMBFromText(c.RegionSplitSize, defaultRegionSplitSize)
	c.RegionBucketSizeMB = typeutil.ParseMBFromText(c.RegionBucketSize, defaultBucketSize)
}
```

[pkg/schedule/config/store_config.go:112](../../../pkg/schedule/config/store_config.go#L112)

```go
func (c *StoreConfig) GetRegionMaxSize() uint64 {
	if c == nil || len(c.RegionMaxSize) == 0 {
		return defaultRegionMaxSize
	}
	return c.RegionMaxSizeMB
}
```

[pkg/schedule/config/store_config.go:120](../../../pkg/schedule/config/store_config.go#L120)

```go
func (c *StoreConfig) GetRegionSplitSize() uint64 {
	if c == nil || len(c.RegionSplitSize) == 0 {
		return defaultRegionSplitSize
	}
	return c.RegionSplitSizeMB
}
```

[pkg/schedule/config/store_config.go:128](../../../pkg/schedule/config/store_config.go#L128)

```go
func (c *StoreConfig) GetRegionSplitKeys() uint64 {
	if c == nil || c.RegionSplitKeys == 0 {
		return defaultRegionSplitKey
	}
	return uint64(c.RegionSplitKeys)
}
```

[pkg/schedule/config/store_config.go:136](../../../pkg/schedule/config/store_config.go#L136)

```go
func (c *StoreConfig) GetRegionMaxKeys() uint64 {
	if c == nil || c.RegionMaxKeys == 0 {
		return defaultRegionMaxKey
	}
	return uint64(c.RegionMaxKeys)
}
```

[pkg/schedule/config/store_config.go:176](../../../pkg/schedule/config/store_config.go#L176)

```go
func (c *StoreConfig) GetRegionBucketSize() uint64 {
	if c == nil || !c.EnableRegionBucket {
		return 0
	}
	if len(c.RegionBucketSize) == 0 {
		return defaultBucketSize
	}
	return c.RegionBucketSizeMB
}
```

### pkg/schedule/config/util.go — owner 48

[pkg/schedule/config/util.go:33](../../../pkg/schedule/config/util.go#L33)

```go
func validateFormat(s, format string) error {
	isValid, _ := regexp.MatchString(format, s)
	if !isValid {
		return errors.Errorf("%s does not match format %q", s, format)
	}
	return nil
}
```

[pkg/schedule/config/util.go:42](../../../pkg/schedule/config/util.go#L42)

```go
func ValidateLabels(labels []*metapb.StoreLabel) error {
	for _, label := range labels {
		if err := validateFormat(label.Key, keyFormat); err != nil {
			return err
		}
		if err := validateFormat(label.Value, valueFormat); err != nil {
			return err
		}
	}
	return nil
}
```

[pkg/schedule/config/util.go:55](../../../pkg/schedule/config/util.go#L55)

```go
func ValidateLabelKey(key string) error {
	return validateFormat(key, keyFormat)
}
```

### pkg/utils/configutil/configutil.go — owner 90/91/92

[pkg/utils/configutil/configutil.go:112](../../../pkg/utils/configutil/configutil.go#L112)

```go
func AdjustCommandLineString(flagSet *pflag.FlagSet, v *string, name string) {
	if value, _ := flagSet.GetString(name); value != "" {
		*v = value
	}
}
```

[pkg/utils/configutil/configutil.go:119](../../../pkg/utils/configutil/configutil.go#L119)

```go
func AdjustCommandLineBool(flagSet *pflag.FlagSet, v *bool, name string) {
	if value, _ := flagSet.GetBool(name); value {
		*v = value
	}
}
```

[pkg/utils/configutil/configutil.go:126](../../../pkg/utils/configutil/configutil.go#L126)

```go
func AdjustString(v *string, defValue string) {
	if len(*v) == 0 {
		*v = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:133](../../../pkg/utils/configutil/configutil.go#L133)

```go
func AdjustUint64(v *uint64, defValue uint64) {
	if *v == 0 {
		*v = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:140](../../../pkg/utils/configutil/configutil.go#L140)

```go
func AdjustInt64(v *int64, defValue int64) {
	if *v == 0 {
		*v = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:147](../../../pkg/utils/configutil/configutil.go#L147)

```go
func AdjustInt(v *int, defValue int) {
	if *v == 0 {
		*v = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:154](../../../pkg/utils/configutil/configutil.go#L154)

```go
func AdjustFloat64(v *float64, defValue float64) {
	if *v == 0 {
		*v = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:161](../../../pkg/utils/configutil/configutil.go#L161)

```go
func AdjustDuration(v *typeutil.Duration, defValue time.Duration) {
	if v.Duration <= 0 {
		v.Duration = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:168](../../../pkg/utils/configutil/configutil.go#L168)

```go
func AdjustByteSize(v *typeutil.ByteSize, defValue typeutil.ByteSize) {
	if *v == 0 {
		*v = defValue
	}
}
```

[pkg/utils/configutil/configutil.go:175](../../../pkg/utils/configutil/configutil.go#L175)

```go
func AdjustPath(p *string) {
	absPath, err := filepath.Abs(*p)
	if err == nil {
		*p = absPath
	}
}
```

[pkg/utils/configutil/configutil.go:183](../../../pkg/utils/configutil/configutil.go#L183)

```go
func AdjustBool(v *bool, defValue bool) {
	if !*v {
		*v = defValue
	}
}
```

### pkg/utils/grpcutil/grpcutil.go — owner 90/91/92

[pkg/utils/grpcutil/grpcutil.go:151](../../../pkg/utils/grpcutil/grpcutil.go#L151)

```go
func GetClientConn(ctx context.Context, addr string, tlsCfg *tls.Config, do ...grpc.DialOption) (*grpc.ClientConn, error) {
	opt := grpc.WithTransportCredentials(insecure.NewCredentials())
	if tlsCfg != nil {
		creds := credentials.NewTLS(tlsCfg)
		opt = grpc.WithTransportCredentials(creds)
	}
	u, err := url.Parse(addr)
	if err != nil {
		return nil, errs.ErrURLParse.Wrap(err).GenWithStackByCause()
	}
	// Here we use a shorter MaxDelay to make the connection recover faster.
	// The default MaxDelay is 120s, which is too long for us.
	backoffOpts := grpc.WithConnectParams(grpc.ConnectParams{
		Backoff: backoff.Config{
			BaseDelay:  time.Second,
			Multiplier: 1.6,
			Jitter:     0.2,
			MaxDelay:   3 * time.Second,
		},
	})
	do = append(do, opt, backoffOpts)
	// TODO: use grpc.NewClient instead of grpc.DialContext.
	//nolint:staticcheck
	cc, err := grpc.DialContext(ctx, u.Host, do...)
	if err != nil {
		return nil, errs.ErrGRPCDial.Wrap(err).GenWithStackByCause()
	}
	return cc, nil
}
```

[pkg/utils/grpcutil/grpcutil.go:236](../../../pkg/utils/grpcutil/grpcutil.go#L236)

```go
func CreateClientConn(ctx context.Context, addr string, tlsConfig *TLSConfig, do ...grpc.DialOption) *grpc.ClientConn {
	var (
		conn *grpc.ClientConn
		err  error
	)
	for {
		select {
		case <-ctx.Done():
			return nil
		default:
		}
		conn, err = establish(ctx, addr, tlsConfig, do...)
		if err != nil {
			log.Warn("cannot establish connection", zap.String("addr", addr), errs.ZapError(err))
			continue
		}
		break
	}
	return conn
}
```

### pkg/utils/metricutil/metricutil.go — owner 15/86/101

[pkg/utils/metricutil/metricutil.go:72](../../../pkg/utils/metricutil/metricutil.go#L72)

```go
func prometheusPushClient(ctx context.Context, job, addr string, interval time.Duration) {
	defer logutil.LogPanic()

	pusher := push.New(addr, job).
		Gatherer(prometheus.DefaultGatherer).
		Grouping("instance", instanceName())

	for {
		err := pusher.Push()
		if err != nil {
			log.Error("could not push metrics to Prometheus Pushgateway", errs.ZapError(errs.ErrPrometheusPushMetrics, err))
		}

		select {
		case <-ctx.Done():
			log.Info("stop Prometheus push client")
			return
		case <-time.After(interval):
			// continue to push metrics
		}
	}
}
```

### server/config/config.go — owner 47/90

[server/config/config.go:176](../../../server/config/config.go#L176)

```go
const (
	defaultLeaderLease             = int64(5)
	defaultCompactionMode          = "periodic"
	defaultAutoCompactionRetention = "1h"
	defaultQuotaBackendBytes       = typeutil.ByteSize(8 * units.GiB) // 8GB

	// The default max bytes for grpc message
	// Unsafe recovery report is included in store heartbeat, and assume that each peer report occupies about 500B at most,
	// then 150MB can fit for store reports that have about 300k regions which is something of a huge amount of region on one TiKV.
	defaultMaxRequestBytes = uint(150 * units.MiB) // 150MB

	defaultName                = "pd"
	defaultClientUrls          = "http://127.0.0.1:2379"
	defaultPeerUrls            = "http://127.0.0.1:2380"
	defaultInitialClusterState = embed.ClusterStateFlagNew
	defaultInitialClusterToken = "pd-cluster"

	// etcd use 100ms for heartbeat and 1s for election timeout.
	// We can enlarge both a little to reduce the network aggression.
	// now embed etcd use TickMs for heartbeat, we will update
	// after embed etcd decouples tick and heartbeat.
	defaultTickInterval = 500 * time.Millisecond
	// embed etcd has a check that `5 * tick > election`
	defaultElectionInterval = 3000 * time.Millisecond

	defaultMetricsPushInterval = 15 * time.Second

	defaultHeartbeatStreamRebindInterval = time.Minute

	defaultLeaderPriorityCheckInterval = time.Minute

	defaultUseRegionStorage  = true
	defaultFlowRoundByDigit  = 3 // KB
	maxTraceFlowRoundByDigit = 5 // 0.1 MB
	defaultMaxResetTSGap     = 24 * time.Hour
	defaultKeyType           = "table"

	// DefaultMinResolvedTSPersistenceInterval is the default value of min resolved ts persistent interval.
	DefaultMinResolvedTSPersistenceInterval = time.Second
	// DefaultGCKeyspaceIndexLoadConcurrency is the default metadata scan concurrency.
	DefaultGCKeyspaceIndexLoadConcurrency = 4
	maxGCKeyspaceIndexLoadConcurrency     = 16
	// DefaultGCStateCacheWarmupConcurrency is the default GC state warmup concurrency.
	DefaultGCStateCacheWarmupConcurrency = 4
	maxGCStateCacheWarmupConcurrency     = 16

	defaultEnableGRPCGateway   = true
	defaultDisableErrorVerbose = true

	defaultDashboardAddress = "auto"

	defaultDRWaitStoreTimeout = time.Minute

	defaultMaxConcurrentTSOProxyStreamings = 5000
	defaultTSOProxyRecvFromClientTimeout   = 1 * time.Hour

	// DefaultTSOSaveInterval is the default value of the config `TSOSaveInterval`.
	DefaultTSOSaveInterval = time.Duration(defaultLeaderLease) * time.Second
	// defaultTSOUpdatePhysicalInterval is the default value of the config `TSOUpdatePhysicalInterval`.
	defaultTSOUpdatePhysicalInterval = 50 * time.Millisecond
	// MaxTSOUpdatePhysicalInterval is the max value of the config `TSOUpdatePhysicalInterval`.
	MaxTSOUpdatePhysicalInterval = 10 * time.Second
	minTSOUpdatePhysicalInterval = 1 * time.Millisecond

	defaultLogFormat = "text"
	defaultLogLevel  = "info"

	defaultServerMemoryLimit          = 0
	minServerMemoryLimit              = 0
	maxServerMemoryLimit              = 0.99
	defaultServerMemoryLimitGCTrigger = 0.7
	minServerMemoryLimitGCTrigger     = 0.5
	maxServerMemoryLimitGCTrigger     = 0.99
	defaultEnableGOGCTuner            = true
	defaultGCTunerThreshold           = 0.6
	minGCTunerThreshold               = 0
	maxGCTunerThreshold               = 0.9

	defaultWaitRegionSplitTimeout   = 30 * time.Second
	defaultCheckRegionSplitInterval = 50 * time.Millisecond
	minCheckRegionSplitInterval     = 1 * time.Millisecond
	maxCheckRegionSplitInterval     = 100 * time.Millisecond

	defaultEnableSchedulingFallback      = true
	defaultEnableTSODynamicSwitching     = false
	defaultEnableResourceManagerFallback = true
)
```

[server/config/config.go:264](../../../server/config/config.go#L264)

```go
var (
	defaultEnableTelemetry = false
	defaultRuntimeServices = []string{}
)
```

[server/config/config.go:273](../../../server/config/config.go#L273)

```go
func initByLDFlags(edition string) {
	if edition != versioninfo.CommunityEdition {
		defaultEnableTelemetry = false
	}
}
```

[server/config/config.go:280](../../../server/config/config.go#L280)

```go
func (c *Config) Parse(flagSet *pflag.FlagSet) error {
	// Load config file if specified.
	var (
		meta *toml.MetaData
		err  error
	)
	if configFile, _ := flagSet.GetString("config"); configFile != "" {
		meta, err = configutil.ConfigFromFile(c, configFile)
		if err != nil {
			return err
		}

		// Backward compatibility for toml config
		if c.LogFileDeprecated != "" {
			msg := fmt.Sprintf("log-file in %s is deprecated, use [log.file] instead", configFile)
			c.WarningMsgs = append(c.WarningMsgs, msg)
			if c.Log.File.Filename == "" {
				c.Log.File.Filename = c.LogFileDeprecated
			}
		}
		if c.LogLevelDeprecated != "" {
			msg := fmt.Sprintf("log-level in %s is deprecated, use [log] instead", configFile)
			c.WarningMsgs = append(c.WarningMsgs, msg)
			if c.Log.Level == "" {
				c.Log.Level = c.LogLevelDeprecated
			}
		}
		if meta.IsDefined("schedule", "disable-raft-learner") {
			msg := fmt.Sprintf("disable-raft-learner in %s is deprecated", configFile)
			c.WarningMsgs = append(c.WarningMsgs, msg)
		}
	}

	// ignore the error check here
	configutil.AdjustCommandLineString(flagSet, &c.Log.Level, "log-level")
	configutil.AdjustCommandLineString(flagSet, &c.Log.File.Filename, "log-file")
	configutil.AdjustCommandLineString(flagSet, &c.Name, "name")
	configutil.AdjustCommandLineString(flagSet, &c.DataDir, "data-dir")
	configutil.AdjustCommandLineString(flagSet, &c.ClientUrls, "client-urls")
	configutil.AdjustCommandLineString(flagSet, &c.AdvertiseClientUrls, "advertise-client-urls")
	configutil.AdjustCommandLineString(flagSet, &c.PeerUrls, "peer-urls")
	configutil.AdjustCommandLineString(flagSet, &c.AdvertisePeerUrls, "advertise-peer-urls")
	configutil.AdjustCommandLineString(flagSet, &c.InitialCluster, "initial-cluster")
	configutil.AdjustCommandLineString(flagSet, &c.Join, "join")
	configutil.AdjustCommandLineString(flagSet, &c.Metric.PushAddress, "metrics-addr")
	configutil.AdjustCommandLineString(flagSet, &c.Security.CAPath, "cacert")
	configutil.AdjustCommandLineString(flagSet, &c.Security.CertPath, "cert")
	configutil.AdjustCommandLineString(flagSet, &c.Security.KeyPath, "key")
	configutil.AdjustCommandLineBool(flagSet, &c.ForceNewCluster, "force-new-cluster")

	return c.Adjust(meta, false)
}
```

[server/config/config.go:334](../../../server/config/config.go#L334)

```go
func (c *Config) Validate() error {
	if c.Join != "" && c.InitialCluster != "" {
		return errors.New("-initial-cluster and -join can not be provided at the same time")
	}
	dataDir, err := filepath.Abs(c.DataDir)
	if err != nil {
		return errors.WithStack(err)
	}
	logFile, err := filepath.Abs(c.Log.File.Filename)
	if err != nil {
		return errors.WithStack(err)
	}
	rel, err := filepath.Rel(dataDir, filepath.Dir(logFile))
	if err != nil {
		return errors.WithStack(err)
	}
	if !strings.HasPrefix(rel, "..") {
		return errors.New("log directory shouldn't be the subdirectory of data directory")
	}

	return nil
}
```

[server/config/config.go:358](../../../server/config/config.go#L358)

```go
func (c *Config) Adjust(meta *toml.MetaData, reloading bool) error {
	configMetaData := configutil.NewConfigMetadata(meta)
	if err := configMetaData.CheckUndecoded(); err != nil {
		c.WarningMsgs = append(c.WarningMsgs, err.Error())
	}

	if c.Name == "" {
		hostname, err := os.Hostname()
		if err != nil {
			return err
		}
		configutil.AdjustString(&c.Name, fmt.Sprintf("%s-%s", defaultName, hostname))
	}
	configutil.AdjustString(&c.DataDir, fmt.Sprintf("default.%s", c.Name))
	configutil.AdjustPath(&c.DataDir)

	if err := c.Validate(); err != nil {
		return err
	}

	configutil.AdjustString(&c.ClientUrls, defaultClientUrls)
	configutil.AdjustString(&c.AdvertiseClientUrls, c.ClientUrls)
	configutil.AdjustString(&c.PeerUrls, defaultPeerUrls)
	configutil.AdjustString(&c.AdvertisePeerUrls, c.PeerUrls)
	configutil.AdjustDuration(&c.Metric.PushInterval, defaultMetricsPushInterval)

	if len(c.InitialCluster) == 0 {
		// The advertise peer urls may be http://127.0.0.1:2380,http://127.0.0.1:2381
		// so the initial cluster is pd=http://127.0.0.1:2380,pd=http://127.0.0.1:2381
		items := strings.Split(c.AdvertisePeerUrls, ",")

		sep := ""
		for _, item := range items {
			c.InitialCluster += fmt.Sprintf("%s%s=%s", sep, c.Name, item)
			sep = ","
		}
	}

	configutil.AdjustString(&c.InitialClusterState, defaultInitialClusterState)
	configutil.AdjustString(&c.InitialClusterToken, defaultInitialClusterToken)

	// Join is a comma-separated list of endpoints (see the field comment and
	// server/join, which splits it on ","), so validate it per endpoint rather
	// than passing the whole list to a single url.Parse, which accepts it as
	// one malformed URL with a host of "pd-0:2379,http:".
	if len(c.Join) > 0 {
		if _, err := parseUrls(c.Join); err != nil {
			return errors.Errorf("failed to parse join addr:%s, err:%v", c.Join, err)
		}
	}

	configutil.AdjustInt(&c.MaxConcurrentTSOProxyStreamings, defaultMaxConcurrentTSOProxyStreamings)
	configutil.AdjustDuration(&c.TSOProxyRecvFromClientTimeout, defaultTSOProxyRecvFromClientTimeout)

	configutil.AdjustInt64(&c.LeaderLease, defaultLeaderLease)
	configutil.AdjustDuration(&c.TSOSaveInterval, DefaultTSOSaveInterval)
	configutil.AdjustDuration(&c.TSOUpdatePhysicalInterval, defaultTSOUpdatePhysicalInterval)

	if c.TSOUpdatePhysicalInterval.Duration > MaxTSOUpdatePhysicalInterval {
		c.TSOUpdatePhysicalInterval.Duration = MaxTSOUpdatePhysicalInterval
	} else if c.TSOUpdatePhysicalInterval.Duration < minTSOUpdatePhysicalInterval {
		c.TSOUpdatePhysicalInterval.Duration = minTSOUpdatePhysicalInterval
	}
	if c.TSOUpdatePhysicalInterval.Duration != defaultTSOUpdatePhysicalInterval {
		log.Warn("tso update physical interval is non-default",
			zap.Duration("update-physical-interval", c.TSOUpdatePhysicalInterval.Duration))
	}

	if c.Labels == nil {
		c.Labels = make(map[string]string)
	}

	configutil.AdjustString(&c.AutoCompactionMode, defaultCompactionMode)
	configutil.AdjustString(&c.AutoCompactionRetention, defaultAutoCompactionRetention)
	if !configMetaData.IsDefined("quota-backend-bytes") {
		c.QuotaBackendBytes = defaultQuotaBackendBytes
	}
	if !configMetaData.IsDefined("max-request-bytes") {
		c.MaxRequestBytes = defaultMaxRequestBytes
	}
	configutil.AdjustDuration(&c.TickInterval, defaultTickInterval)
	configutil.AdjustDuration(&c.ElectionInterval, defaultElectionInterval)

	configutil.AdjustString(&c.Metric.PushJob, c.Name)

	if err := c.Schedule.Adjust(configMetaData.Child("schedule"), reloading); err != nil {
		return err
	}
	if err := c.Replication.Adjust(configMetaData.Child("replication")); err != nil {
		return err
	}

	if err := c.PDServerCfg.adjust(configMetaData.Child("pd-server")); err != nil {
		return err
	}

	c.adjustLog(configMetaData.Child("log"))
	configutil.AdjustDuration(&c.HeartbeatStreamBindInterval, defaultHeartbeatStreamRebindInterval)

	configutil.AdjustDuration(&c.LeaderPriorityCheckInterval, defaultLeaderPriorityCheckInterval)

	if !configMetaData.IsDefined("enable-prevote") {
		c.PreVote = true
	}
	if !configMetaData.IsDefined("enable-grpc-gateway") {
		c.EnableGRPCGateway = defaultEnableGRPCGateway
	}

	c.Dashboard.adjust(configMetaData.Child("dashboard"))

	c.ReplicationMode.adjust(configMetaData.Child("replication-mode"))

	if err := c.Keyspace.adjust(configMetaData.Child("keyspace")); err != nil {
		return err
	}

	c.Microservice.adjust(configMetaData.Child("micro-service"))

	if err := c.Security.Encryption.Adjust(); err != nil {
		return err
	}

	c.Controller.Adjust(configMetaData.Child("controller"))

	return nil
}
```

[server/config/config.go:485](../../../server/config/config.go#L485)

```go
func (c *Config) adjustLog(meta *configutil.ConfigMetaData) {
	if !meta.IsDefined("disable-error-verbose") {
		c.Log.DisableErrorVerbose = defaultDisableErrorVerbose
	}
	configutil.AdjustString(&c.Log.Format, defaultLogFormat)
	configutil.AdjustString(&c.Log.Level, defaultLogLevel)
}
```

[server/config/config.go:544](../../../server/config/config.go#L544)

```go
func (c *PDServerConfig) adjust(meta *configutil.ConfigMetaData) error {
	if !meta.IsDefined("gc-keyspace-index-load-concurrency") {
		configutil.AdjustInt(&c.GCKeyspaceIndexLoadConcurrency, DefaultGCKeyspaceIndexLoadConcurrency)
	}
	if !meta.IsDefined("gc-state-cache-warmup-concurrency") {
		configutil.AdjustInt(&c.GCStateCacheWarmupConcurrency, DefaultGCStateCacheWarmupConcurrency)
	}
	configutil.AdjustDuration(&c.MaxResetTSGap, defaultMaxResetTSGap)
	if !meta.IsDefined("use-region-storage") {
		c.UseRegionStorage = defaultUseRegionStorage
	}
	if !meta.IsDefined("key-type") {
		c.KeyType = defaultKeyType
	}
	if !meta.IsDefined("runtime-services") {
		c.RuntimeServices = defaultRuntimeServices
	}
	if !meta.IsDefined("dashboard-address") {
		c.DashboardAddress = defaultDashboardAddress
	}
	if !meta.IsDefined("flow-round-by-digit") {
		configutil.AdjustInt(&c.FlowRoundByDigit, defaultFlowRoundByDigit)
	}
	if !meta.IsDefined("min-resolved-ts-persistence-interval") {
		configutil.AdjustDuration(&c.MinResolvedTSPersistenceInterval, DefaultMinResolvedTSPersistenceInterval)
	}
	if !meta.IsDefined("server-memory-limit") {
		configutil.AdjustFloat64(&c.ServerMemoryLimit, defaultServerMemoryLimit)
	}
	if c.ServerMemoryLimit < minServerMemoryLimit {
		c.ServerMemoryLimit = minServerMemoryLimit
	} else if c.ServerMemoryLimit > maxServerMemoryLimit {
		c.ServerMemoryLimit = maxServerMemoryLimit
	}
	if !meta.IsDefined("server-memory-limit-gc-trigger") {
		configutil.AdjustFloat64(&c.ServerMemoryLimitGCTrigger, defaultServerMemoryLimitGCTrigger)
	}
	if c.ServerMemoryLimitGCTrigger < minServerMemoryLimitGCTrigger {
		c.ServerMemoryLimitGCTrigger = minServerMemoryLimitGCTrigger
	} else if c.ServerMemoryLimitGCTrigger > maxServerMemoryLimitGCTrigger {
		c.ServerMemoryLimitGCTrigger = maxServerMemoryLimitGCTrigger
	}
	if !meta.IsDefined("enable-gogc-tuner") {
		c.EnableGOGCTuner = defaultEnableGOGCTuner
	}
	if !meta.IsDefined("gc-tuner-threshold") {
		configutil.AdjustFloat64(&c.GCTunerThreshold, defaultGCTunerThreshold)
	}
	if c.GCTunerThreshold < minGCTunerThreshold {
		c.GCTunerThreshold = minGCTunerThreshold
	} else if c.GCTunerThreshold > maxGCTunerThreshold {
		c.GCTunerThreshold = maxGCTunerThreshold
	}
	if err := migrateConfigurationFromFile(meta); err != nil {
		return err
	}
	return c.Validate()
}
```

[server/config/config.go:603](../../../server/config/config.go#L603)

```go
func migrateConfigurationFromFile(meta *configutil.ConfigMetaData) error {
	oldName, newName := "trace-region-flow", "flow-round-by-digit"
	defineOld := meta.IsDefined(oldName)
	switch {
	case defineOld:
		return errors.Errorf("config item %s and %s(deprecated) are conflict", newName, oldName)
	default:
	}
	return nil
}
```

[server/config/config.go:623](../../../server/config/config.go#L623)

```go
func (c *PDServerConfig) Validate() error {
	if err := c.validateGCConcurrency(); err != nil {
		return err
	}
	switch c.DashboardAddress {
	case "auto":
	case "none":
	default:
		if err := ValidateURLWithScheme(c.DashboardAddress); err != nil {
			return err
		}
	}
	if c.KeyType != "table" && c.KeyType != "raw" && c.KeyType != "txn" {
		return errors.Errorf("key-type %v is invalid", c.KeyType)
	}
	if c.FlowRoundByDigit < 0 {
		return errs.ErrConfigItem.GenWithStack("flow round by digit cannot be negative number")
	}
	if c.ServerMemoryLimit < minServerMemoryLimit || c.ServerMemoryLimit > maxServerMemoryLimit {
		return errors.New(fmt.Sprintf("server-memory-limit should between %v and %v", minServerMemoryLimit, maxServerMemoryLimit))
	}
	if c.ServerMemoryLimitGCTrigger < minServerMemoryLimitGCTrigger || c.ServerMemoryLimitGCTrigger > maxServerMemoryLimitGCTrigger {
		return errors.New(fmt.Sprintf("server-memory-limit-gc-trigger should between %v and %v",
			minServerMemoryLimitGCTrigger, maxServerMemoryLimitGCTrigger))
	}
	if c.GCTunerThreshold < minGCTunerThreshold || c.GCTunerThreshold > maxGCTunerThreshold {
		return errors.New(fmt.Sprintf("gc-tuner-threshold should between %v and %v", minGCTunerThreshold, maxGCTunerThreshold))
	}

	return nil
}
```

[server/config/config.go:655](../../../server/config/config.go#L655)

```go
func (c *PDServerConfig) validateGCConcurrency() error {
	if c.GCKeyspaceIndexLoadConcurrency < 1 || c.GCKeyspaceIndexLoadConcurrency > maxGCKeyspaceIndexLoadConcurrency {
		return errors.Errorf("gc-keyspace-index-load-concurrency must be between 1 and %d", maxGCKeyspaceIndexLoadConcurrency)
	}
	if c.GCStateCacheWarmupConcurrency < 1 || c.GCStateCacheWarmupConcurrency > maxGCStateCacheWarmupConcurrency {
		return errors.Errorf("gc-state-cache-warmup-concurrency must be between 1 and %d", maxGCStateCacheWarmupConcurrency)
	}
	return nil
}
```

[server/config/config.go:809](../../../server/config/config.go#L809)

```go
func (c *DashboardConfig) adjust(meta *configutil.ConfigMetaData) {
	if !meta.IsDefined("enable-telemetry") {
		c.EnableTelemetry = defaultEnableTelemetry
	}
}
```

[server/config/config.go:828](../../../server/config/config.go#L828)

```go
func (c *ReplicationModeConfig) adjust(meta *configutil.ConfigMetaData) {
	if !meta.IsDefined("replication-mode") || NormalizeReplicationMode(c.ReplicationMode) == "" {
		c.ReplicationMode = "majority"
	}
	c.DRAutoSync.adjust(meta.Child("dr-auto-sync"))
}
```

[server/config/config.go:857](../../../server/config/config.go#L857)

```go
func (c *DRAutoSyncReplicationConfig) adjust(meta *configutil.ConfigMetaData) {
	if !meta.IsDefined("wait-store-timeout") {
		c.WaitStoreTimeout = typeutil.NewDuration(defaultDRWaitStoreTimeout)
	}
}
```

[server/config/config.go:873](../../../server/config/config.go#L873)

```go
func (c *MicroserviceConfig) adjust(meta *configutil.ConfigMetaData) {
	if !meta.IsDefined("enable-scheduling-fallback") {
		c.EnableSchedulingFallback = defaultEnableSchedulingFallback
	}
	if !meta.IsDefined("enable-tso-dynamic-switching") {
		c.EnableTSODynamicSwitching = defaultEnableTSODynamicSwitching
	}
	if !meta.IsDefined("enable-resource-manager-fallback") {
		c.EnableResourceManagerFallback = defaultEnableResourceManagerFallback
	}
}
```

[server/config/config.go:922](../../../server/config/config.go#L922)

```go
func (c *KeyspaceConfig) Validate() error {
	if c.CheckRegionSplitInterval.Duration > maxCheckRegionSplitInterval || c.CheckRegionSplitInterval.Duration < minCheckRegionSplitInterval {
		return errors.New(fmt.Sprintf("[keyspace] check-region-split-interval should between %v and %v",
			minCheckRegionSplitInterval, maxCheckRegionSplitInterval))
	}
	if c.CheckRegionSplitInterval.Duration >= c.WaitRegionSplitTimeout.Duration {
		return errors.New("[keyspace] check-region-split-interval should be less than wait-region-split-timeout")
	}
	return nil
}
```

[server/config/config.go:933](../../../server/config/config.go#L933)

```go
func (c *KeyspaceConfig) adjust(meta *configutil.ConfigMetaData) error {
	if !meta.IsDefined("wait-region-split") {
		c.WaitRegionSplit = true
	}
	if !meta.IsDefined("wait-region-split-timeout") {
		c.WaitRegionSplitTimeout = typeutil.NewDuration(defaultWaitRegionSplitTimeout)
	}
	if !meta.IsDefined("check-region-split-interval") {
		c.CheckRegionSplitInterval = typeutil.NewDuration(defaultCheckRegionSplitInterval)
	}

	return AdjustMetaServiceGroups(c.MetaServiceGroups)
}
```

[server/config/config.go:956](../../../server/config/config.go#L956)

```go
func AdjustMetaServiceGroups(metaGroups map[string]string) error {
	dict := make(map[string]string, len(metaGroups))
	for groupID, endpoint := range metaGroups {
		id := strings.TrimSpace(groupID)
		if id == "" {
			return errors.New("[keyspace] meta-service group ID cannot be empty")
		}
		if !IsValidMetaServiceGroupID(id) {
			return errors.New(fmt.Sprintf("[keyspace] meta-service group ID cannot contain '/': %s", id))
		}
		address := strings.TrimSpace(endpoint)
		if address == "" {
			return errors.New("[keyspace] meta-service group addresses cannot be empty")
		}
		if _, ok := dict[id]; ok {
			return errors.New(fmt.Sprintf("[keyspace] meta-service group ID cannot be duplicated: %s", id))
		}
		dict[id] = address
	}
	// Clear the original map and copy the validated values back to it.
	for groupID := range metaGroups {
		delete(metaGroups, groupID)
	}
	for groupID, endpoint := range dict {
		metaGroups[groupID] = endpoint
	}

	return nil
}
```

### server/config/persist_options.go — owner 47/90

[server/config/persist_options.go:439](../../../server/config/persist_options.go#L439)

```go
func (o *PersistOptions) SetStoreLimit(storeID uint64, typ storelimit.Type, ratePerMin float64) {
	o.mutateScheduleConfig(func(next *sc.ScheduleConfig) {
		limit, ok := next.StoreLimit[storeID]
		if !ok {
			limit = next.GetDefaultStoreLimit()
		}
		next.StoreLimit[storeID] = limit.SetLimit(typ, ratePerMin)
	})
}
```

[server/config/persist_options.go:450](../../../server/config/persist_options.go#L450)

```go
func (o *PersistOptions) SetAllStoresLimit(typ storelimit.Type, ratePerMin float64) {
	o.mutateScheduleConfig(func(next *sc.ScheduleConfig) {
		next.DefaultStoreLimit = next.DefaultStoreLimit.SetLimit(typ, ratePerMin)
		sc.DefaultStoreLimit.SetDefaultStoreLimit(typ, ratePerMin)
		for storeID, limit := range next.StoreLimit {
			next.StoreLimit[storeID] = limit.SetLimit(typ, ratePerMin)
		}
	})
}
```

[server/config/persist_options.go:516](../../../server/config/persist_options.go#L516)

```go
func (o *PersistOptions) GetStoreLimit(storeID uint64) (returnSC sc.StoreLimitConfig) {
	if limit, ok := o.GetScheduleConfig().StoreLimit[storeID]; ok {
		return limit
	}
	o.mutateScheduleConfig(func(next *sc.ScheduleConfig) {
		if limit, ok := next.StoreLimit[storeID]; ok {
			returnSC = limit
			return
		}
		returnSC = next.GetDefaultStoreLimit()
		next.StoreLimit[storeID] = returnSC
	})
	return returnSC
}
```

[server/config/persist_options.go:546](../../../server/config/persist_options.go#L546)

```go
func (o *PersistOptions) PeekStoreLimitByType(storeID uint64, typ storelimit.Type) float64 {
	cfg := o.GetScheduleConfig()
	limit, ok := cfg.StoreLimit[storeID]
	if !ok {
		limit = cfg.GetDefaultStoreLimit()
	}
	return storeLimitByType(limit, typ)
}
```

[server/config/persist_options.go:555](../../../server/config/persist_options.go#L555)

```go
func storeLimitByType(limit sc.StoreLimitConfig, typ storelimit.Type) float64 {
	switch typ {
	case storelimit.AddPeer:
		return limit.AddPeer
	case storelimit.RemovePeer:
		return limit.RemovePeer
	case storelimit.TransferLeaderIn:
		return limit.TransferLeaderIn
	// todo: impl it in store limit v2.
	case storelimit.SendSnapshot:
		return 0.0
	default:
		panic("no such limit type")
	}
}
```

[server/config/persist_options.go:779](../../../server/config/persist_options.go#L779)

```go
func (o *PersistOptions) RemoveSchedulerCfg(tp types.CheckerSchedulerType) {
	oldType := types.SchedulerTypeCompatibleMap[tp]
	o.mutateScheduleConfig(func(next *sc.ScheduleConfig) {
		for i, schedulerCfg := range next.Schedulers {
			if oldType == schedulerCfg.Type {
				if sc.IsDefaultScheduler(oldType) {
					schedulerCfg.Disable = true
					next.Schedulers[i] = schedulerCfg
				} else {
					next.Schedulers = append(next.Schedulers[:i], next.Schedulers[i+1:]...)
				}
				return
			}
		}
	})
}
```

[server/config/persist_options.go:834](../../../server/config/persist_options.go#L834)

```go
func (c *persistedConfig) UnmarshalJSON(data []byte) error {
	type plainPersistedConfig persistedConfig
	if err := json.Unmarshal(data, (*plainPersistedConfig)(c)); err != nil {
		return err
	}
	var fields struct {
		Schedule json.RawMessage `json:"schedule"`
	}
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	if len(fields.Schedule) == 0 {
		return nil
	}
	return c.Schedule.MigrateDeprecatedFlagsFromJSON(fields.Schedule)
}
```

[server/config/persist_options.go:887](../../../server/config/persist_options.go#L887)

```go
func (o *PersistOptions) Reload(storage endpoint.ConfigStorage) error {
	cfg := &persistedConfig{Config: &Config{}}
	// Pass nil to initialize cfg to default values (all items undefined)
	if err := cfg.Adjust(nil, true); err != nil {
		return err
	}

	isExist, err := storage.LoadConfig(cfg)
	if err != nil {
		return err
	}
	if err := cfg.PDServerCfg.validateGCConcurrency(); err != nil {
		return err
	}
	adjustScheduleCfg(&cfg.Schedule)
	// Some fields may not be stored in the storage, we need to calculate them manually.
	cfg.StoreConfig.Adjust()
	if isExist {
		o.schedule.Store(&cfg.Schedule)
		o.replication.Store(&cfg.Replication)
		o.pdServerConfig.Store(&cfg.PDServerCfg)
		o.replicationMode.Store(&cfg.ReplicationMode)
		o.labelProperty.Store(cfg.LabelProperty)
		o.keyspace.Store(&cfg.Keyspace)
		o.microservice.Store(&cfg.Microservice)
		o.storeConfig.Store(&cfg.StoreConfig)
		o.SetClusterVersion(&cfg.ClusterVersion)
	}
	return nil
}
```

[server/config/persist_options.go:918](../../../server/config/persist_options.go#L918)

```go
func adjustScheduleCfg(scheduleCfg *sc.ScheduleConfig) {
	// In case we add new default schedulers.
	for _, ps := range sc.DefaultSchedulers {
		if slice.NoneOf(scheduleCfg.Schedulers, func(i int) bool {
			return scheduleCfg.Schedulers[i].Type == ps.Type
		}) {
			scheduleCfg.Schedulers = append(scheduleCfg.Schedulers, ps)
		}
	}
	scheduleCfg.MigrateDeprecatedFlags()
}
```

[server/config/persist_options.go:949](../../../server/config/persist_options.go#L949)

```go
func (o *PersistOptions) SetTTLData(parCtx context.Context, client *clientv3.Client, key string, value string, ttl time.Duration) error {
	if o.ttl == nil {
		o.ttl = cache.NewStringTTL(parCtx, sc.DefaultGCInterval, sc.DefaultTTL)
	}
	if ttl != 0 {
		// the minimum ttl is 5 seconds, if the given ttl is less than 5 seconds, we will use 5 seconds instead.
		_, err := etcdutil.EtcdKVPutWithTTL(parCtx, client, sc.TTLConfigPrefix+"/"+key, value, int64(ttl.Seconds()))
		if err != nil {
			return err
		}
	} else {
		_, err := client.Delete(parCtx, sc.TTLConfigPrefix+"/"+key)
		if err != nil {
			return err
		}
	}
	o.ttl.PutWithTTL(key, value, ttl)
	return nil
}
```

[server/config/persist_options.go:990](../../../server/config/persist_options.go#L990)

```go
func (o *PersistOptions) getTTLNumberOr(key string, defaultValue uint64) uint64 {
	if v, ok, err := o.getTTLNumber(key); ok {
		if err == nil {
			return v
		}
		log.Warn("failed to parse "+key+" from PersistOptions's ttl storage", zap.Error(err))
	}
	return defaultValue
}
```

[server/config/persist_options.go:1010](../../../server/config/persist_options.go#L1010)

```go
func (o *PersistOptions) getTTLBoolOr(key string, defaultValue bool) bool {
	if v, ok, err := o.getTTLBool(key); ok {
		if err == nil {
			return v
		}
		log.Warn("failed to parse "+key+" from PersistOptions's ttl storage", zap.Error(err))
	}
	return defaultValue
}
```

[server/config/persist_options.go:1032](../../../server/config/persist_options.go#L1032)

```go
func (o *PersistOptions) LoadTTLFromEtcd(ctx context.Context, client *clientv3.Client) error {
	resps, err := etcdutil.EtcdKVGet(client, sc.TTLConfigPrefix, clientv3.WithPrefix())
	if err != nil {
		return err
	}
	if o.ttl == nil {
		o.ttl = cache.NewStringTTL(ctx, sc.DefaultGCInterval, sc.DefaultTTL)
	}
	for _, resp := range resps.Kvs {
		key := string(resp.Key)[len(sc.TTLConfigPrefix)+1:]
		value := string(resp.Value)
		leaseID := resp.Lease
		resp, err := client.TimeToLive(ctx, clientv3.LeaseID(leaseID))
		if err != nil {
			return err
		}
		o.ttl.PutWithTTL(key, value, time.Duration(resp.TTL)*time.Second)
	}
	return nil
}
```

### server/config/service_middleware_config.go — owner 50/79/80/81

[server/config/service_middleware_config.go:19](../../../server/config/service_middleware_config.go#L19)

```go
const (
	defaultEnableAuditMiddleware         = true
	defaultEnableRateLimitMiddleware     = true
	defaultEnableGRPCRateLimitMiddleware = true
)
```

[server/config/service_middleware_config.go:33](../../../server/config/service_middleware_config.go#L33)

```go
func NewServiceMiddlewareConfig() *ServiceMiddlewareConfig {
	audit := AuditConfig{
		EnableAudit: defaultEnableAuditMiddleware,
	}
	rateLimit := RateLimitConfig{
		EnableRateLimit: defaultEnableRateLimitMiddleware,
		LimiterConfig:   make(map[string]ratelimit.DimensionConfig),
	}
	grpcRateLimit := GRPCRateLimitConfig{
		EnableRateLimit: defaultEnableRateLimitMiddleware,
		LimiterConfig:   make(map[string]ratelimit.DimensionConfig),
	}
	cfg := &ServiceMiddlewareConfig{
		AuditConfig:         audit,
		RateLimitConfig:     rateLimit,
		GRPCRateLimitConfig: grpcRateLimit,
	}
	return cfg
}
```

### server/config/service_middleware_persist_options.go — owner 50/79/80/81

[server/config/service_middleware_persist_options.go:103](../../../server/config/service_middleware_persist_options.go#L103)

```go
func (o *ServiceMiddlewarePersistOptions) Reload(storage endpoint.ServiceMiddlewareStorage) error {
	cfg := NewServiceMiddlewareConfig()

	isExist, err := storage.LoadServiceMiddlewareConfig(cfg)
	if err != nil {
		return err
	}
	if isExist {
		o.audit.Store(&cfg.AuditConfig)
		o.rateLimit.Store(&cfg.RateLimitConfig)
		o.grpcRateLimit.Store(&cfg.GRPCRateLimitConfig)
	}
	return nil
}
```

### server/config/util.go — owner 47/90

[server/config/util.go:27](../../../server/config/util.go#L27)

```go
func ValidateURLWithScheme(rawURL string) error {
	u, err := url.ParseRequestURI(rawURL)
	if err != nil {
		return err
	}
	if u.Scheme == "" || u.Host == "" {
		return errors.Errorf("%s has no scheme", rawURL)
	}
	return nil
}
```

## 已冻结 Go 输出逐字段观测（独立于源码默认）

以下仅 missing 案例中实际保存的 config；其 TOML/启动元数据见 fixture，不声称本次重跑，也不把显式name/端口/log设置解释为默认。外部 log.Config、metering_sdk、semver 的内部字段由冻结输出界定观测上界；未覆盖分支必须补任务4/90/92或主题9证据。

| JSON路径 | missing案例值 | 原始输出 | owner |
| --- | --- | --- | --- |
| client-urls | "http://127.0.0.1:0" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| peer-urls | "http://127.0.0.1:0" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| advertise-client-urls | "http://127.0.0.1:0" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| advertise-peer-urls | "http://127.0.0.1:0" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| name | "nested-oracle" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| data-dir | "/var/folders/gr/w4kj8kxx1v53r3p8ycyv4s3c0000gn/T/pd97-oracle-o1r5iizq/data" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| force-new-cluster | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| enable-grpc-gateway | true | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| initial-cluster | "nested-oracle=http://127.0.0.1:0" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| initial-cluster-state | "new" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| initial-cluster-token | "pd-cluster" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| join | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| lease | 5 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| log.level | "info" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.format | "json" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.disable-timestamp | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.file.filename | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.file.max-size | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.file.max-days | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.file.max-backups | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.development | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.disable-caller | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.disable-stacktrace | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.disable-error-verbose | true | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.sampling | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| log.error-output-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| max-concurrent-tso-proxy-streamings | 5000 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| tso-proxy-recv-from-client-timeout | "1h0m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| tso-save-interval | "5s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| tso-update-physical-interval | "50ms" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| enable-local-tso | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| metric.job | "nested-oracle" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| metric.address | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| metric.interval | "15s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| schedule.max-snapshot-count | 64 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-pending-peer-count | 64 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-merge-region-size | 54 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-merge-region-keys | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.split-merge-interval | "1h0m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.switch-witness-interval | "1h0m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-one-way-merge | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-cross-table-merge | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.patrol-region-interval | "10ms" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-store-down-time | "30m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-store-preparing-time | "48h0m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.leader-schedule-limit | 4 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.leader-schedule-policy | "count" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.region-schedule-limit | 2048 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.witness-schedule-limit | 4 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.replica-schedule-limit | 64 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.merge-schedule-limit | 8 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.hot-region-schedule-limit | 4 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.affinity-schedule-limit | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.split-scatter-schedule-limit | 4 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.hot-region-cache-hits-threshold | 3 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.default-store-limit.add-peer | 15 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.default-store-limit.remove-peer | 15 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.default-store-limit.transfer-leader-in | 100000000 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.store-limit | {} | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.tolerant-size-ratio | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.low-space-ratio | 0.8 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.high-space-ratio | 0.7 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.region-score-formula-version | "v2" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.scheduler-max-waiting-operator | 5 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-remove-down-replica | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-replace-offline-replica | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-make-up-replica | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-remove-extra-replica | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-location-replacement | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-debug-metrics | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-joint-consensus | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-tikv-split-region | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-heartbeat-breakdown-metrics | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-heartbeat-concurrent-runner | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.schedulers-v2 | [{"type": "balance-region", "args": null, "disable": false, "args-payload": ""}, {"type": "balance-leader", "args": null, "disable": false, "args-payload": ""}, {"type": "hot-region", "args": null, "disable": false, "args-payload": ""}, {"type": "evict-slow-store", "args": null, "disable": false, "args-payload": ""}, {"type": "evict-stopping-store", "args": null, "disable": false, "args-payload": ""}] | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.hot-regions-write-interval | "10m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.hot-regions-reserved-days | 7 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-movable-hot-peer-size | 512 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-diagnostic | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.enable-witness | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.slow-store-evicting-affected-store-ratio-threshold | 0.3 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.store-limit-version | "v1" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.patrol-region-worker-count | 1 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| schedule.max-affinity-merge-region-size | 256 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 48 |
| replication.max-replicas | 3 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 49 |
| replication.location-labels | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 49 |
| replication.strictly-match-label | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 49 |
| replication.enable-placement-rules | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 49 |
| replication.enable-placement-rules-cache | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 49 |
| replication.isolation-level | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 49 |
| pd-server.use-region-storage | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.max-gap-reset-ts | "24h0m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.key-type | "table" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.runtime-services | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.metric-storage | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.dashboard-address | "auto" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.flow-round-by-digit | 3 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.min-resolved-ts-persistence-interval | "1s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.server-memory-limit | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.server-memory-limit-gc-trigger | 0.7 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.enable-gogc-tuner | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.gc-tuner-threshold | 0.6 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.block-safe-point-v1 | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.gc-keyspace-index-load-concurrency | 4 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| pd-server.gc-state-cache-warmup-concurrency | 4 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| cluster-version | "0.0.0" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| labels | {} | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| quota-backend-bytes | "8GiB" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| auto-compaction-mode | "periodic" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| auto-compaction-retention-v2 | "1h" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| tick-interval | "500ms" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| election-interval | "3s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| enable-prevote | true | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| max-request-bytes | 157286400 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| security.cacert-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.cert-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.key-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.cert-allowed-cn | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.SSLCABytes | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.SSLCertBytes | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.SSLKEYBytes | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.redact-info-log | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 82–85 |
| security.encryption.data-encryption-method | "plaintext" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| security.encryption.data-key-rotation-period | "168h0m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| security.encryption.master-key.type | "plaintext" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| security.encryption.master-key.key-id | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| security.encryption.master-key.region | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| security.encryption.master-key.endpoint | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| security.encryption.master-key.path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| label-property | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| dashboard.tidb-cacert-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.tidb-cert-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.tidb-key-path | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.public-path-prefix | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.temp-dir | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.internal-proxy | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.enable-telemetry | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.enable-experimental | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| dashboard.disable-custom-prom-addr | false | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |
| replication-mode.replication-mode | "majority" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.label-key | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.primary | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.dr | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.primary-replicas | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.dr-replicas | 0 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.wait-store-timeout | "1m0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.wait-recover-timeout | "0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| replication-mode.dr-auto-sync.pause-region-split | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 47/90 |
| keyspace.pre-alloc | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| keyspace.wait-region-split | true | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| keyspace.wait-region-split-timeout | "30s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| keyspace.check-region-split-interval | "50ms" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| keyspace.meta-service-groups | null | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| micro-service.enable-scheduling-fallback | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| micro-service.enable-tso-dynamic-switching | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| micro-service.enable-resource-manager-fallback | "true" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.degraded-mode-wait-duration | "0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.ltb-max-wait-duration | "30s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.ltb-token-rpc-max-delay | "1s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.read-base-cost | 0.125 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.read-per-batch-base-cost | 0.5 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.read-cost-per-byte | 1.52587890625e-05 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.write-base-cost | 1 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.write-per-batch-base-cost | 1 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.write-cost-per-byte | 0.0009765625 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.request-unit.read-cpu-ms-cost | 0.3333333333333333 | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.enable-controller-trace-log | "false" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.push-metrics-address | "" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| controller.push-metrics-interval | "0s" | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题8；78/102边界 |
| metering | {} | [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | 主题9；90/92接线 |

## 废弃字段专门验收索引

所有条目均缺当前运行差分；准入任务4，owner47/48/90，日志/Go资源调优相关另依赖主题9。默认与冲突分支见上面的原文块。

| 字段 | Go契约/来源 | Rust候选 | owner |
| --- | --- | --- | --- |
| log-file / log-level | [Config.Parse](../../../server/config/config.go#L279)：警告；新log字段为空时才迁入 | [deprecated_config.rs](../../../server/config/deprecated_config.rs) | 90/92 |
| enable-local-tso | [Config声明](../../../server/config/config.go#L100)：标为Deprecated，不能据此假设被拒绝 | [config_decode.rs](../../../server/config/config_decode.rs) | 90/36 |
| schedule.disable-raft-learner | [ScheduleConfig](../../../pkg/schedule/config/config.go#L260)：废弃，Parse警告 | [deprecated_config.rs](../../../server/config/deprecated_config.rs) | 48/90 |
| schedule.store-balance-rate | [ScheduleConfig](../../../pkg/schedule/config/config.go#L236)：向default-store-limit迁移，显式维度保留 | [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | 48 |
| disable-remove-down-replica | [ScheduleConfig](../../../pkg/schedule/config/config.go#L265)：迁移至enable-remove-down-replica | [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | 48 |
| disable-replace-offline-replica | [ScheduleConfig](../../../pkg/schedule/config/config.go#L269)：迁移至enable-replace-offline-replica | [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | 48 |
| disable-make-up-replica | [ScheduleConfig](../../../pkg/schedule/config/config.go#L273)：迁移至enable-make-up-replica | [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | 48 |
| disable-remove-extra-replica | [ScheduleConfig](../../../pkg/schedule/config/config.go#L277)：迁移至enable-remove-extra-replica | [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | 48 |
| disable-location-replacement | [ScheduleConfig](../../../pkg/schedule/config/config.go#L281)：迁移至enable-location-replacement；TOML、JSON持久化入口须分别比较 | [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | 48 |
| pd-server.trace-region-flow | [migrateConfigurationFromFile](../../../server/config/config.go#L603)：只要旧字段定义就返回冲突错误，不推断静默迁移 | [deprecated_config.rs](../../../server/config/deprecated_config.rs) | 47/90 |

命令静态补充：[createServerWrapper/start](../../../cmd/pd-server/main.go#L210)仅将非空且忽略大小写等于api的PD_SERVICE_MODE转为api服务；其它值保持classic。Go启动先Config.Parse，之后才处理version/config-check；两者显式exit(0)，顶层Cobra.Execute错误打印后exit(1)，start内部部分错误只打印并return。Rust当前parse错误为2、启动错误为1、成功为0。这些差异需91取得真实进程stdout/stderr/退出码，不能擅自统一错误码。审计实现依赖主题9，81只负责接线；独立MCS及keyspace/GC/encryption/controller依赖主题8，78/102只负责边界；主题10保留embedded-etcd及原生后端的拓扑与耐久性责任。未发现需要新增来源史诗的未映射需求，未改写任何来源计划。

## CLI flag（命令作用域分别保留）

| ID/来源/命令函数 | 声明含类型、默认、短flag、help | Rust | owner |
| --- | --- | --- | --- |
| F-001 [cmd/pd-server/main.go:97](../../../cmd/pd-server/main.go#L97) NewTSOServiceCommand | cmd.Flags().StringP("name", "", "", "human-readable name for this tso member") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-002 [cmd/pd-server/main.go:98](../../../cmd/pd-server/main.go#L98) NewTSOServiceCommand | cmd.Flags().BoolP("version", "V", false, "print version information and exit") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-003 [cmd/pd-server/main.go:99](../../../cmd/pd-server/main.go#L99) NewTSOServiceCommand | cmd.Flags().StringP("config", "", "", "config file") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-004 [cmd/pd-server/main.go:100](../../../cmd/pd-server/main.go#L100) NewTSOServiceCommand | cmd.Flags().StringP("backend-endpoints", "", "", "url for etcd client") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-005 [cmd/pd-server/main.go:101](../../../cmd/pd-server/main.go#L101) NewTSOServiceCommand | cmd.Flags().StringP("listen-addr", "", "", "listen address for tso service") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-006 [cmd/pd-server/main.go:102](../../../cmd/pd-server/main.go#L102) NewTSOServiceCommand | cmd.Flags().StringP("advertise-listen-addr", "", "", "advertise urls for listen address (default '${listen-addr}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-007 [cmd/pd-server/main.go:103](../../../cmd/pd-server/main.go#L103) NewTSOServiceCommand | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-008 [cmd/pd-server/main.go:104](../../../cmd/pd-server/main.go#L104) NewTSOServiceCommand | cmd.Flags().StringP("cert", "", "", "path of file that contains X509 certificate in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-009 [cmd/pd-server/main.go:105](../../../cmd/pd-server/main.go#L105) NewTSOServiceCommand | cmd.Flags().StringP("key", "", "", "path of file that contains X509 key in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-010 [cmd/pd-server/main.go:106](../../../cmd/pd-server/main.go#L106) NewTSOServiceCommand | cmd.Flags().StringP("log-level", "L", "", "log level: debug, info, warn, error, fatal (default 'info')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-011 [cmd/pd-server/main.go:107](../../../cmd/pd-server/main.go#L107) NewTSOServiceCommand | cmd.Flags().StringP("log-file", "", "", "log file path") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-012 [cmd/pd-server/main.go:118](../../../cmd/pd-server/main.go#L118) NewSchedulingServiceCommand | cmd.Flags().StringP("name", "", "", "human-readable name for this scheduling member") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-013 [cmd/pd-server/main.go:119](../../../cmd/pd-server/main.go#L119) NewSchedulingServiceCommand | cmd.Flags().BoolP("version", "V", false, "print version information and exit") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-014 [cmd/pd-server/main.go:120](../../../cmd/pd-server/main.go#L120) NewSchedulingServiceCommand | cmd.Flags().StringP("config", "", "", "config file") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-015 [cmd/pd-server/main.go:121](../../../cmd/pd-server/main.go#L121) NewSchedulingServiceCommand | cmd.Flags().StringP("backend-endpoints", "", "", "url for etcd client") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-016 [cmd/pd-server/main.go:122](../../../cmd/pd-server/main.go#L122) NewSchedulingServiceCommand | cmd.Flags().StringP("listen-addr", "", "", "listen address for tso service") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-017 [cmd/pd-server/main.go:123](../../../cmd/pd-server/main.go#L123) NewSchedulingServiceCommand | cmd.Flags().StringP("advertise-listen-addr", "", "", "advertise urls for listen address (default '${listen-addr}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-018 [cmd/pd-server/main.go:124](../../../cmd/pd-server/main.go#L124) NewSchedulingServiceCommand | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-019 [cmd/pd-server/main.go:125](../../../cmd/pd-server/main.go#L125) NewSchedulingServiceCommand | cmd.Flags().StringP("cert", "", "", "path of file that contains X509 certificate in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-020 [cmd/pd-server/main.go:126](../../../cmd/pd-server/main.go#L126) NewSchedulingServiceCommand | cmd.Flags().StringP("key", "", "", "path of file that contains X509 key in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-021 [cmd/pd-server/main.go:127](../../../cmd/pd-server/main.go#L127) NewSchedulingServiceCommand | cmd.Flags().StringP("log-level", "L", "", "log level: debug, info, warn, error, fatal (default 'info')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-022 [cmd/pd-server/main.go:128](../../../cmd/pd-server/main.go#L128) NewSchedulingServiceCommand | cmd.Flags().StringP("log-file", "", "", "log file path") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-023 [cmd/pd-server/main.go:139](../../../cmd/pd-server/main.go#L139) NewResourceManagerServiceCommand | cmd.Flags().StringP("name", "", "", "human-readable name for this resource manager member") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-024 [cmd/pd-server/main.go:140](../../../cmd/pd-server/main.go#L140) NewResourceManagerServiceCommand | cmd.Flags().BoolP("version", "V", false, "print version information and exit") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-025 [cmd/pd-server/main.go:141](../../../cmd/pd-server/main.go#L141) NewResourceManagerServiceCommand | cmd.Flags().StringP("config", "", "", "config file") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-026 [cmd/pd-server/main.go:142](../../../cmd/pd-server/main.go#L142) NewResourceManagerServiceCommand | cmd.Flags().StringP("backend-endpoints", "", "", "url for etcd client") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-027 [cmd/pd-server/main.go:143](../../../cmd/pd-server/main.go#L143) NewResourceManagerServiceCommand | cmd.Flags().StringP("listen-addr", "", "", "listen address for resource management service") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-028 [cmd/pd-server/main.go:144](../../../cmd/pd-server/main.go#L144) NewResourceManagerServiceCommand | cmd.Flags().StringP("advertise-listen-addr", "", "", "advertise urls for listen address (default '${listen-addr}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-029 [cmd/pd-server/main.go:145](../../../cmd/pd-server/main.go#L145) NewResourceManagerServiceCommand | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-030 [cmd/pd-server/main.go:146](../../../cmd/pd-server/main.go#L146) NewResourceManagerServiceCommand | cmd.Flags().StringP("cert", "", "", "path of file that contains X509 certificate in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-031 [cmd/pd-server/main.go:147](../../../cmd/pd-server/main.go#L147) NewResourceManagerServiceCommand | cmd.Flags().StringP("key", "", "", "path of file that contains X509 key in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-032 [cmd/pd-server/main.go:148](../../../cmd/pd-server/main.go#L148) NewResourceManagerServiceCommand | cmd.Flags().StringP("log-level", "L", "", "log level: debug, info, warn, error, fatal (default 'info')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-033 [cmd/pd-server/main.go:149](../../../cmd/pd-server/main.go#L149) NewResourceManagerServiceCommand | cmd.Flags().StringP("log-file", "", "", "log file path") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-034 [cmd/pd-server/main.go:160](../../../cmd/pd-server/main.go#L160) NewRouterServiceCommand | cmd.Flags().StringP("name", "", "", "human-readable name for this router member") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-035 [cmd/pd-server/main.go:161](../../../cmd/pd-server/main.go#L161) NewRouterServiceCommand | cmd.Flags().BoolP("version", "V", false, "print version information and exit") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-036 [cmd/pd-server/main.go:162](../../../cmd/pd-server/main.go#L162) NewRouterServiceCommand | cmd.Flags().StringP("config", "", "", "config file") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-037 [cmd/pd-server/main.go:163](../../../cmd/pd-server/main.go#L163) NewRouterServiceCommand | cmd.Flags().StringP("backend-endpoints", "", "", "url for etcd client") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-038 [cmd/pd-server/main.go:164](../../../cmd/pd-server/main.go#L164) NewRouterServiceCommand | cmd.Flags().StringP("listen-addr", "", "", "listen address for tso service") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-039 [cmd/pd-server/main.go:165](../../../cmd/pd-server/main.go#L165) NewRouterServiceCommand | cmd.Flags().StringP("advertise-listen-addr", "", "", "advertise urls for listen address (default '${listen-addr}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-040 [cmd/pd-server/main.go:166](../../../cmd/pd-server/main.go#L166) NewRouterServiceCommand | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-041 [cmd/pd-server/main.go:167](../../../cmd/pd-server/main.go#L167) NewRouterServiceCommand | cmd.Flags().StringP("cert", "", "", "path of file that contains X509 certificate in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-042 [cmd/pd-server/main.go:168](../../../cmd/pd-server/main.go#L168) NewRouterServiceCommand | cmd.Flags().StringP("key", "", "", "path of file that contains X509 key in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-043 [cmd/pd-server/main.go:169](../../../cmd/pd-server/main.go#L169) NewRouterServiceCommand | cmd.Flags().StringP("log-level", "L", "", "log level: debug, info, warn, error, fatal (default 'info')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-044 [cmd/pd-server/main.go:170](../../../cmd/pd-server/main.go#L170) NewRouterServiceCommand | cmd.Flags().StringP("log-file", "", "", "log file path") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 主题8；78/102边界 |
| F-045 [cmd/pd-server/main.go:186](../../../cmd/pd-server/main.go#L186) addFlags | cmd.Flags().BoolP("version", "V", false, "print version information and exit") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-046 [cmd/pd-server/main.go:187](../../../cmd/pd-server/main.go#L187) addFlags | cmd.Flags().StringP("config", "", "", "config file") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-047 [cmd/pd-server/main.go:188](../../../cmd/pd-server/main.go#L188) addFlags | cmd.Flags().BoolP("config-check", "", false, "check config file validity and exit") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-048 [cmd/pd-server/main.go:189](../../../cmd/pd-server/main.go#L189) addFlags | cmd.Flags().StringP("name", "", "", "human-readable name for this pd member") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-049 [cmd/pd-server/main.go:190](../../../cmd/pd-server/main.go#L190) addFlags | cmd.Flags().StringP("data-dir", "", "", "path to the data directory (default 'default.${name}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-050 [cmd/pd-server/main.go:191](../../../cmd/pd-server/main.go#L191) addFlags | cmd.Flags().StringP("client-urls", "", "", "urls for client traffic") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-051 [cmd/pd-server/main.go:192](../../../cmd/pd-server/main.go#L192) addFlags | cmd.Flags().StringP("advertise-client-urls", "", "", "advertise urls for client traffic (default '${client-urls}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-052 [cmd/pd-server/main.go:193](../../../cmd/pd-server/main.go#L193) addFlags | cmd.Flags().StringP("peer-urls", "", "", "urls for peer traffic") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-053 [cmd/pd-server/main.go:194](../../../cmd/pd-server/main.go#L194) addFlags | cmd.Flags().StringP("advertise-peer-urls", "", "", "advertise urls for peer traffic (default '${peer-urls}')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-054 [cmd/pd-server/main.go:195](../../../cmd/pd-server/main.go#L195) addFlags | cmd.Flags().StringP("initial-cluster", "", "", "initial cluster configuration for bootstrapping, e,g. pd=http://127.0.0.1:2380") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-055 [cmd/pd-server/main.go:196](../../../cmd/pd-server/main.go#L196) addFlags | cmd.Flags().StringP("join", "", "", "join to an existing cluster (usage: cluster's '${advertise-client-urls}'") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-056 [cmd/pd-server/main.go:197](../../../cmd/pd-server/main.go#L197) addFlags | cmd.Flags().StringP("metrics-addr", "", "", "prometheus pushgateway address, leaves it empty will disable prometheus push") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-057 [cmd/pd-server/main.go:198](../../../cmd/pd-server/main.go#L198) addFlags | cmd.Flags().StringP("log-level", "L", "", "log level: debug, info, warn, error, fatal (default 'info')") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-058 [cmd/pd-server/main.go:199](../../../cmd/pd-server/main.go#L199) addFlags | cmd.Flags().StringP("log-file", "", "", "log file path") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-059 [cmd/pd-server/main.go:200](../../../cmd/pd-server/main.go#L200) addFlags | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-060 [cmd/pd-server/main.go:201](../../../cmd/pd-server/main.go#L201) addFlags | cmd.Flags().StringP("cert", "", "", "path of file that contains X509 certificate in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-061 [cmd/pd-server/main.go:202](../../../cmd/pd-server/main.go#L202) addFlags | cmd.Flags().StringP("key", "", "", "path of file that contains X509 key in PEM format") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |
| F-062 [cmd/pd-server/main.go:203](../../../cmd/pd-server/main.go#L203) addFlags | cmd.Flags().BoolP("force-new-cluster", "", false, "force to create a new one-member cluster") | [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs)；未证明逐flag等价 | 90/91 |

## 环境覆盖与命令优先级

addFlags 共用于根命令及 services api；表内62个显式注册点（含4个独立MCS作用域各11项，以及classic共享18项）不重复展开共享函数调用。Cobra自动help/completion与 -h、命令错误/usage、未知参数和退出码须由91运行冻结，未从注册点推断。

配置加载顺序为 `Config.Parse`：读 --config TOML → deprecated log字段兼容（新字段非空优先）→ Changed CLI覆盖 → Adjust(meta,false)。不推断通用 PD_* 自动覆盖。`PD_SERVICE_MODE` 是进程模式分流，独立services子命令属于主题8；Pyroscope环境变量只供profiling组件。环境值从未读取/保存，以下仅变量名源码。

| 来源 | 环境/解析或命令行为原文 | owner |
| --- | --- | --- |
| [cmd/pd-server/main.go:55](../../../cmd/pd-server/main.go#L55) | serviceModeEnv = "PD_SERVICE_MODE" | 90/91/92 |
| [cmd/pd-server/main.go:211](../../../cmd/pd-server/main.go#L211) | mode := os.Getenv(serviceModeEnv) | 90/91/92 |
| [pkg/encryption/kms.go:62](../../../pkg/encryption/kms.go#L62) | roleArn := os.Getenv(envAwsRoleArn) | 主题8；78/102边界 |
| [pkg/encryption/kms.go:63](../../../pkg/encryption/kms.go#L63) | tokenFile := os.Getenv(envAwsWebIdentityTokenFile) | 主题8；78/102边界 |
| [pkg/encryption/kms.go:64](../../../pkg/encryption/kms.go#L64) | sessionName := os.Getenv(envAwsRoleSessionName) | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:288](../../../pkg/mcs/resourcemanager/server/config.go#L288) | configutil.AdjustCommandLineString(flagSet, &c.Name, "name") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:289](../../../pkg/mcs/resourcemanager/server/config.go#L289) | configutil.AdjustCommandLineString(flagSet, &c.Log.Level, "log-level") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:290](../../../pkg/mcs/resourcemanager/server/config.go#L290) | configutil.AdjustCommandLineString(flagSet, &c.Log.File.Filename, "log-file") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:291](../../../pkg/mcs/resourcemanager/server/config.go#L291) | configutil.AdjustCommandLineString(flagSet, &c.Metric.PushAddress, "metrics-addr") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:292](../../../pkg/mcs/resourcemanager/server/config.go#L292) | configutil.AdjustCommandLineString(flagSet, &c.Security.CAPath, "cacert") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:293](../../../pkg/mcs/resourcemanager/server/config.go#L293) | configutil.AdjustCommandLineString(flagSet, &c.Security.CertPath, "cert") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:294](../../../pkg/mcs/resourcemanager/server/config.go#L294) | configutil.AdjustCommandLineString(flagSet, &c.Security.KeyPath, "key") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:295](../../../pkg/mcs/resourcemanager/server/config.go#L295) | configutil.AdjustCommandLineString(flagSet, &c.BackendEndpoints, "backend-endpoints") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:296](../../../pkg/mcs/resourcemanager/server/config.go#L296) | configutil.AdjustCommandLineString(flagSet, &c.ListenAddr, "listen-addr") | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:297](../../../pkg/mcs/resourcemanager/server/config.go#L297) | configutil.AdjustCommandLineString(flagSet, &c.AdvertiseListenAddr, "advertise-listen-addr") | 主题8；78/102边界 |
| [pkg/utils/configutil/configutil.go:111](../../../pkg/utils/configutil/configutil.go#L111) | // AdjustCommandLineString adjusts the value of a string variable from command line flags. | 90/91/92 |
| [pkg/utils/configutil/configutil.go:112](../../../pkg/utils/configutil/configutil.go#L112) | func AdjustCommandLineString(flagSet *pflag.FlagSet, v *string, name string) { | 90/91/92 |
| [pkg/utils/configutil/configutil.go:118](../../../pkg/utils/configutil/configutil.go#L118) | // AdjustCommandLineBool adjusts the value of a bool variable from command line flags. | 90/91/92 |
| [pkg/utils/configutil/configutil.go:119](../../../pkg/utils/configutil/configutil.go#L119) | func AdjustCommandLineBool(flagSet *pflag.FlagSet, v *bool, name string) { | 90/91/92 |
| [pkg/utils/metricutil/metricutil.go:118](../../../pkg/utils/metricutil/metricutil.go#L118) | if os.Getenv("PYROSCOPE_SERVER_ADDRESS") != "" { | 主题9；92接线 |
| [pkg/utils/metricutil/metricutil.go:123](../../../pkg/utils/metricutil/metricutil.go#L123) | ServerAddress:     os.Getenv("PYROSCOPE_SERVER_ADDRESS"), | 主题9；92接线 |
| [pkg/utils/metricutil/metricutil.go:125](../../../pkg/utils/metricutil/metricutil.go#L125) | AuthToken:         os.Getenv("PYROSCOPE_AUTH_TOKEN"), //nolint:staticcheck // Preserve token authentication for existing deployments. | 主题9；92接线 |
| [pkg/utils/metricutil/metricutil.go:126](../../../pkg/utils/metricutil/metricutil.go#L126) | TenantID:          os.Getenv("PYROSCOPE_TENANT_ID"), | 主题9；92接线 |
| [pkg/utils/metricutil/metricutil.go:127](../../../pkg/utils/metricutil/metricutil.go#L127) | BasicAuthUser:     os.Getenv("PYROSCOPE_BASIC_AUTH_USER"), | 主题9；92接线 |
| [pkg/utils/metricutil/metricutil.go:128](../../../pkg/utils/metricutil/metricutil.go#L128) | BasicAuthPassword: os.Getenv("PYROSCOPE_BASIC_AUTH_PASSWORD"), | 主题9；92接线 |
| [server/config/config.go:314](../../../server/config/config.go#L314) | configutil.AdjustCommandLineString(flagSet, &c.Log.Level, "log-level") | 47/90 |
| [server/config/config.go:315](../../../server/config/config.go#L315) | configutil.AdjustCommandLineString(flagSet, &c.Log.File.Filename, "log-file") | 47/90 |
| [server/config/config.go:316](../../../server/config/config.go#L316) | configutil.AdjustCommandLineString(flagSet, &c.Name, "name") | 47/90 |
| [server/config/config.go:317](../../../server/config/config.go#L317) | configutil.AdjustCommandLineString(flagSet, &c.DataDir, "data-dir") | 47/90 |
| [server/config/config.go:318](../../../server/config/config.go#L318) | configutil.AdjustCommandLineString(flagSet, &c.ClientUrls, "client-urls") | 47/90 |
| [server/config/config.go:319](../../../server/config/config.go#L319) | configutil.AdjustCommandLineString(flagSet, &c.AdvertiseClientUrls, "advertise-client-urls") | 47/90 |
| [server/config/config.go:320](../../../server/config/config.go#L320) | configutil.AdjustCommandLineString(flagSet, &c.PeerUrls, "peer-urls") | 47/90 |
| [server/config/config.go:321](../../../server/config/config.go#L321) | configutil.AdjustCommandLineString(flagSet, &c.AdvertisePeerUrls, "advertise-peer-urls") | 47/90 |
| [server/config/config.go:322](../../../server/config/config.go#L322) | configutil.AdjustCommandLineString(flagSet, &c.InitialCluster, "initial-cluster") | 47/90 |
| [server/config/config.go:323](../../../server/config/config.go#L323) | configutil.AdjustCommandLineString(flagSet, &c.Join, "join") | 47/90 |
| [server/config/config.go:324](../../../server/config/config.go#L324) | configutil.AdjustCommandLineString(flagSet, &c.Metric.PushAddress, "metrics-addr") | 47/90 |
| [server/config/config.go:325](../../../server/config/config.go#L325) | configutil.AdjustCommandLineString(flagSet, &c.Security.CAPath, "cacert") | 82–85 |
| [server/config/config.go:326](../../../server/config/config.go#L326) | configutil.AdjustCommandLineString(flagSet, &c.Security.CertPath, "cert") | 82–85 |
| [server/config/config.go:327](../../../server/config/config.go#L327) | configutil.AdjustCommandLineString(flagSet, &c.Security.KeyPath, "key") | 82–85 |
| [server/config/config.go:328](../../../server/config/config.go#L328) | configutil.AdjustCommandLineBool(flagSet, &c.ForceNewCluster, "force-new-cluster") | 47/90 |

## Prometheus 构造契约

覆盖 server/ 与 pkg/ 的生产源码所有 `prometheus.New*` 构造调用（包含独立领域，owner明确移交）。这是声明上界，不是单进程实际注册/暴露集合；注册与采集调用另列。Name由 Namespace/Subsystem/Name以下三项拼接，空项省略；动态表达式保持原文，不虚构输出。Counter导出还须验证_total归一化；Histogram/Summary须验证派生bucket/sum/count。标签保留顺序，ConstLabels、桶/分位数见完整构造原文。

| ID/来源/变量 | type | Namespace / Subsystem / Name | HELP | labels/选项原文 | Rust/owner |
| --- | --- | --- | --- | --- | --- |
| M-001 [client/resource_group/controller/metrics/metrics.go:83](../../../client/resource_group/controller/metrics/metrics.go#L83) ResourceGroupStatusGauge = | GaugeVec | namespace / "resource_group" / "status" | "Status of the resource group." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace:   namespace,<br>			Subsystem:   "resource_group",<br>			Name:        "status",<br>			Help:        "Status of the resource group.",<br>			ConstLabels: constLabels,<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-002 [client/resource_group/controller/metrics/metrics.go:92](../../../client/resource_group/controller/metrics/metrics.go#L92) SuccessfulRequestDuration = | HistogramVec | namespace / requestSubsystem / "success" | "Bucketed histogram of wait duration of successful request." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "success",<br>			Buckets:     []float64{0.0005, .005, .01, .05, .1, .5, 1, 5, 10, 20, 25, 30, 60, 600, 1800, 3600}, // 0.0005 ~ 1h<br>			Help:        "Bucketed histogram of wait duration of successful request.",<br>			ConstLabels: constLabels,<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-003 [client/resource_group/controller/metrics/metrics.go:102](../../../client/resource_group/controller/metrics/metrics.go#L102) FailedLimitReserveDuration = | HistogramVec | namespace / requestSubsystem / "limit_reserve_time_failed" | "Bucketed histogram of wait duration of failed request." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "limit_reserve_time_failed",<br>			Buckets:     []float64{0.0005, .01, .05, .1, .5, 1, 5, 10, 20, 25, 30, 60, 600, 1800, 3600, 86400}, // 0.0005 ~ 24h<br>			Help:        "Bucketed histogram of wait duration of failed request.",<br>			ConstLabels: constLabels,<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-004 [client/resource_group/controller/metrics/metrics.go:112](../../../client/resource_group/controller/metrics/metrics.go#L112) FailedRequestCounter = | CounterVec | namespace / requestSubsystem / "fail" | "Counter of failed request." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "fail",<br>			Help:        "Counter of failed request.",<br>			ConstLabels: constLabels,<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-005 [client/resource_group/controller/metrics/metrics.go:121](../../../client/resource_group/controller/metrics/metrics.go#L121) GroupRunningKVRequestCounter = | GaugeVec | namespace / requestSubsystem / "running_kv_request" | "Counter of running kv request." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "running_kv_request",<br>			Help:        "Counter of running kv request.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-006 [client/resource_group/controller/metrics/metrics.go:130](../../../client/resource_group/controller/metrics/metrics.go#L130) RequestRetryCounter = | CounterVec | namespace / requestSubsystem / "retry" | "Counter of retry time for request." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "retry",<br>			Help:        "Counter of retry time for request.",<br>			ConstLabels: constLabels,<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-007 [client/resource_group/controller/metrics/metrics.go:139](../../../client/resource_group/controller/metrics/metrics.go#L139) TokenRequestDuration = | HistogramVec | namespace / tokenRequestSubsystem / "duration" | "Bucketed histogram of latency(s) of token request." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace:   namespace,<br>			Subsystem:   tokenRequestSubsystem,<br>			Buckets:     prometheus.ExponentialBuckets(0.001, 2, 13), // 1ms ~ 8s<br>			Name:        "duration",<br>			Help:        "Bucketed histogram of latency(s) of token request.",<br>			ConstLabels: constLabels,<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-008 [client/resource_group/controller/metrics/metrics.go:149](../../../client/resource_group/controller/metrics/metrics.go#L149) ResourceGroupTokenRequestCounter = | CounterVec | namespace / tokenRequestSubsystem / "resource_group" | "Counter of token request by every resource group." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   tokenRequestSubsystem,<br>			Name:        "resource_group",<br>			Help:        "Counter of token request by every resource group.",<br>			ConstLabels: constLabels,<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-009 [client/resource_group/controller/metrics/metrics.go:158](../../../client/resource_group/controller/metrics/metrics.go#L158) LowTokenRequestNotifyCounter = | CounterVec | namespace / tokenRequestSubsystem / "low_token_notified" | "Counter of low token request." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   tokenRequestSubsystem,<br>			Name:        "low_token_notified",<br>			Help:        "Counter of low token request.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-010 [client/resource_group/controller/metrics/metrics.go:166](../../../client/resource_group/controller/metrics/metrics.go#L166) TokenConsumedHistogram = | HistogramVec | namespace / tokenRequestSubsystem / "consume" | "Bucketed histogram of token consume." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace:   namespace,<br>			Subsystem:   tokenRequestSubsystem,<br>			Name:        "consume",<br>			Buckets:     []float64{.5, 1, 2, 5, 10, 15, 20, 40, 64, 128, 256, 512, 1024, 2048}, // 0 ~ 2048<br>			Help:        "Bucketed histogram of token consume.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-011 [client/resource_group/controller/metrics/metrics.go:176](../../../client/resource_group/controller/metrics/metrics.go#L176) RequestSourceRUCounter = | CounterVec | namespace / requestSubsystem / "ru_total" | "Counter of request RU accounting deltas grouped by resource group and request source. Refund deltas are stored as positive values under direction=refund." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "ru_total",<br>			Help:        "Counter of request RU accounting deltas grouped by resource group and request source. Refund deltas are stored as positive values under direction=refund.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel, requestSourceLabel, typeLabel, directionLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-012 [client/resource_group/controller/metrics/metrics.go:189](../../../client/resource_group/controller/metrics/metrics.go#L189) CopReadPrechargeCounter = | CounterVec | namespace / requestSubsystem / "cop_read_precharge_total" | "Counter of read coprocessor RPCs that carried a positive PredictedReadBytes hint and triggered request-side read-byte pre-charge." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "cop_read_precharge_total",<br>			Help:        "Counter of read coprocessor RPCs that carried a positive PredictedReadBytes hint and triggered request-side read-byte pre-charge.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-013 [client/resource_group/controller/metrics/metrics.go:198](../../../client/resource_group/controller/metrics/metrics.go#L198) CopReadNoPrechargeCounter = | CounterVec | namespace / requestSubsystem / "cop_read_no_precharge_total" | "Counter of read coprocessor RPCs that reached the RC interceptor without a positive PredictedReadBytes hint, so request-side read-byte pre-charge was skipped." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "cop_read_no_precharge_total",<br>			Help:        "Counter of read coprocessor RPCs that reached the RC interceptor without a positive PredictedReadBytes hint, so request-side read-byte pre-charge was skipped.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-014 [client/resource_group/controller/metrics/metrics.go:207](../../../client/resource_group/controller/metrics/metrics.go#L207) PagingPrechargeBytesCounter = | CounterVec | namespace / requestSubsystem / "paging_precharge_bytes_total" | "Sum of bytes used as the RC paging pre-charge basis (predicted hint from coprocessor RPCs)." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "paging_precharge_bytes_total",<br>			Help:        "Sum of bytes used as the RC paging pre-charge basis (predicted hint from coprocessor RPCs).",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-015 [client/resource_group/controller/metrics/metrics.go:216](../../../client/resource_group/controller/metrics/metrics.go#L216) PagingActualBytesCounter = | CounterVec | namespace / requestSubsystem / "paging_actual_bytes_total" | "Sum of actual bytes read by pre-charged coprocessor RPCs." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace:   namespace,<br>			Subsystem:   requestSubsystem,<br>			Name:        "paging_actual_bytes_total",<br>			Help:        "Sum of actual bytes read by pre-charged coprocessor RPCs.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-016 [client/resource_group/controller/metrics/metrics.go:225](../../../client/resource_group/controller/metrics/metrics.go#L225) PagingPredictionResidualBytes = | HistogramVec | namespace / requestSubsystem / "paging_prediction_residual_bytes" | "Histogram of signed (actual_read_bytes - predicted_read_bytes) for pre-charged coprocessor RPCs. Use bucket series for distribution/quantiles; _sum is non-monotonic because observations can be negative." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: requestSubsystem,<br>			Name:      "paging_prediction_residual_bytes",<br>			// Signed residual = actual - predicted for a single coprocessor<br>			// RPC response. Buckets span ±64MB to cover per-RPC prediction<br>			// residuals in both directions while factor-4 spacing keeps<br>			// resolution near zero.<br>			Buckets:     []float64{-67108864, -16777216, -4194304, -1048576, -262144, -65536, -16384, -4096, 0, 4096, 16384, 65536, 262144, 1048576, 4194304, 16777216, 67108864},<br>			Help:        "Histogram of signed (actual_read_bytes - predicted_read_bytes) for pre-charged coprocessor RPCs. Use bucket series for distribution/quantiles; _sum is non-monotonic because observations can be negative.",<br>			ConstLabels: constLabels,<br>		}, []string{newResourceGroupNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-017 [pkg/basicserver/metrics.go:24](../../../pkg/basicserver/metrics.go#L24) ServerMaxProcsGauge = | Gauge | "pd" / "service" / "maxprocs" | "The value of GOMAXPROCS." | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "service",<br>			Name:      "maxprocs",<br>			Help:      "The value of GOMAXPROCS.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-018 [pkg/basicserver/metrics.go:33](../../../pkg/basicserver/metrics.go#L33) ServerMemoryLimit = | Gauge | "pd" / "service" / "memory_quota_bytes" | "The value of memory quota bytes." | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "service",<br>			Name:      "memory_quota_bytes",<br>			Help:      "The value of memory quota bytes.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-019 [pkg/basicserver/metrics.go:42](../../../pkg/basicserver/metrics.go#L42) ServerInfoGauge = | GaugeVec | "pd" / "server" / "info" | "Indicate the pd server info, and the value is the start timestamp (s)." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "info",<br>			Help:      "Indicate the pd server info, and the value is the start timestamp (s).",<br>		}, []string{"version", "hash"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-020 [pkg/core/metrics.go:26](../../../pkg/core/metrics.go#L26) HeartbeatBreakdownHandleDurationSum = | CounterVec | "pd" / "core" / "region_heartbeat_breakdown_handle_duration_seconds_sum" | "Bucketed histogram of processing time (s) of handle the heartbeat stage." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "core",<br>			Name:      "region_heartbeat_breakdown_handle_duration_seconds_sum",<br>			Help:      "Bucketed histogram of processing time (s) of handle the heartbeat stage.",<br>		}, []string{"name"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-021 [pkg/core/metrics.go:35](../../../pkg/core/metrics.go#L35) HeartbeatBreakdownHandleCount = | CounterVec | "pd" / "core" / "region_heartbeat_breakdown_handle_duration_seconds_count" | "Bucketed histogram of processing count of handle the heartbeat stage." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "core",<br>			Name:      "region_heartbeat_breakdown_handle_duration_seconds_count",<br>			Help:      "Bucketed histogram of processing count of handle the heartbeat stage.",<br>		}, []string{"name"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-022 [pkg/core/metrics.go:43](../../../pkg/core/metrics.go#L43) AcquireRegionsLockWaitDurationSum = | CounterVec | "pd" / "core" / "acquire_regions_lock_wait_duration_seconds_sum" | "Bucketed histogram of processing time (s) of waiting for acquiring regions lock." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "core",<br>			Name:      "acquire_regions_lock_wait_duration_seconds_sum",<br>			Help:      "Bucketed histogram of processing time (s) of waiting for acquiring regions lock.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-023 [pkg/core/metrics.go:51](../../../pkg/core/metrics.go#L51) AcquireRegionsLockWaitCount = | CounterVec | "pd" / "core" / "acquire_regions_lock_wait_duration_seconds_count" | "Bucketed histogram of processing count of waiting for acquiring regions lock." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "core",<br>			Name:      "acquire_regions_lock_wait_duration_seconds_count",<br>			Help:      "Bucketed histogram of processing count of waiting for acquiring regions lock.",<br>		}, []string{"name"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-024 [pkg/core/metrics.go:86](../../../pkg/core/metrics.go#L86) queryRegionDuration = | HistogramVec | "pd" / "core" / "query_region_duration_seconds" | "Bucketed histogram of processing time (s) of region query." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "core",<br>			Name:      "query_region_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of region query.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-025 [pkg/core/metrics.go:95](../../../pkg/core/metrics.go#L95) queryRegionCount = | CounterVec | "pd" / "core" / "query_region_count" | "The number of region query types." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "core",<br>			Name:      "query_region_count",<br>			Help:      "The number of region query types.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-026 [pkg/core/metrics.go:111](../../../pkg/core/metrics.go#L111) bucketEventCounter = | CounterVec | "pd" / "cluster" / "bucket_event" | "Counter of the bucket event" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "bucket_event",<br>			Help:      "Counter of the bucket event",<br>		}, []string{"event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-027 [pkg/election/metrics.go:59](../../../pkg/election/metrics.go#L59) keepAliveResponseInterval = | HistogramVec | metricsNamespace / metricsSubsystem / "keepalive_response_interval_seconds" | "Interval between consecutive valid lease keepalive responses received from etcd, by purpose." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: metricsNamespace,<br>			Subsystem: metricsSubsystem,<br>			Name:      "keepalive_response_interval_seconds",<br>			Help:      "Interval between consecutive valid lease keepalive responses received from etcd, by purpose.",<br>			Buckets:   prometheus.ExponentialBuckets(0.001, 2, 16),<br>		},<br>		[]string{metricsLabelPurpose},<br>	) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-028 [pkg/election/metrics.go:70](../../../pkg/election/metrics.go#L70) renewalTerminationTotal = | CounterVec | metricsNamespace / metricsSubsystem / "renewal_termination_total" | "Number of lease renewal stop or anomaly events, broken down by purpose and reason. Reason &#96;context_canceled&#96; is benign (caller-initiated shutdown); &#96;lease_expired&#96; indicates keepalive timeout; &#96;invalid_ttl&#96; indicates a successful keepalive response with non-positive TTL." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: metricsNamespace,<br>			Subsystem: metricsSubsystem,<br>			Name:      "renewal_termination_total",<br>			Help:      "Number of lease renewal stop or anomaly events, broken down by purpose and reason. Reason &#96;context_canceled&#96; is benign (caller-initiated shutdown); &#96;lease_expired&#96; indicates keepalive timeout; &#96;invalid_ttl&#96; indicates a successful keepalive response with non-positive TTL.",<br>		},<br>		[]string{metricsLabelPurpose, metricsLabelReason},<br>	) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-029 [pkg/election/metrics.go:80](../../../pkg/election/metrics.go#L80) keepAliveRequestDuration = | HistogramVec | metricsNamespace / metricsSubsystem / "keepalive_request_duration_seconds" | "Duration of etcd Lease.KeepAliveOnce requests observed by PD, by purpose and result." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: metricsNamespace,<br>			Subsystem: metricsSubsystem,<br>			Name:      "keepalive_request_duration_seconds",<br>			Help:      "Duration of etcd Lease.KeepAliveOnce requests observed by PD, by purpose and result.",<br>			Buckets:   keepAliveLatencyBuckets,<br>		},<br>		[]string{metricsLabelPurpose, metricsLabelResult},<br>	) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-030 [pkg/election/metrics.go:91](../../../pkg/election/metrics.go#L91) keepAliveTickInterval = | HistogramVec | metricsNamespace / metricsSubsystem / "keepalive_tick_interval_seconds" | "Interval between consecutive KeepAliveOnce call start times, observed inside each worker goroutine by purpose. Spikes indicate local scheduling delay (CPU starvation, GC pauses, goroutine pressure) and correlate with the &#96;the interval between keeping alive lease is too long&#96; warning." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: metricsNamespace,<br>			Subsystem: metricsSubsystem,<br>			Name:      "keepalive_tick_interval_seconds",<br>			Help:      "Interval between consecutive KeepAliveOnce call start times, observed inside each worker goroutine by purpose. Spikes indicate local scheduling delay (CPU starvation, GC pauses, goroutine pressure) and correlate with the &#96;the interval between keeping alive lease is too long&#96; warning.",<br>			Buckets:   keepAliveLatencyBuckets,<br>		},<br>		[]string{metricsLabelPurpose},<br>	) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-031 [pkg/election/metrics.go:149](../../../pkg/election/metrics.go#L149) desc: | Desc | ∅（非Opts或动态） / ∅（非Opts或动态） / ∅（非Opts或动态） | ∅（非Opts或动态） | prometheus.NewDesc(<br>			prometheus.BuildFQName(metricsNamespace, metricsSubsystem, localTTLRemainingName),<br>			localTTLRemainingHelp,<br>			[]string{metricsLabelPurpose},<br>			nil,<br>		) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-032 [pkg/gc/metrics.go:57](../../../pkg/gc/metrics.go#L57) barrierTimestampDesc = | Desc | ∅（非Opts或动态） / ∅（非Opts或动态） / ∅（非Opts或动态） | ∅（非Opts或动态） | prometheus.NewDesc(<br>		"pd_gc_barrier_timestamp_seconds",<br>		"Physical Unix timestamp of a valid GC barrier more than 24 hours old, observed by successful transaction safe point advancement.",<br>		[]string{"scope", "keyspace_id", "keyspace_name", "barrier_id"}, nil,<br>	) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-033 [pkg/gc/metrics.go:63](../../../pkg/gc/metrics.go#L63) gcSafePointGauge         = | GaugeVec | "pd" / "gc" / "gc_safepoint" | "The ts of gc safepoint" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "gc",<br>			Name:      "gc_safepoint",<br>			Help:      "The ts of gc safepoint",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-034 [pkg/gc/metrics.go:70](../../../pkg/gc/metrics.go#L70) gcStateCacheAccessCounter = | CounterVec | "pd" / "gc" / "gc_state_cache_access_total" | "Counter of GC state cache accesses by result." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "gc",<br>			Name:      "gc_state_cache_access_total",<br>			Help:      "Counter of GC state cache accesses by result.",<br>		}, []string{"result"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-035 [pkg/id/metrics.go:20](../../../pkg/id/metrics.go#L20) idGauge = | GaugeVec | "pd" / "cluster" / "id" | "Record of id allocator." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "id",<br>			Help:      "Record of id allocator.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-036 [pkg/keyspace/metrics.go:43](../../../pkg/keyspace/metrics.go#L43) createKeyspaceStepDuration = | HistogramVec | namespace / subsystem / "create_keyspace_step_duration_seconds" | "Bucketed histogram of processing time (s) of each step or total in create keyspace operation." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: subsystem,<br>			Name:      "create_keyspace_step_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of each step or total in create keyspace operation.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{"step"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-037 [pkg/maintenance/metrics.go:20](../../../pkg/maintenance/metrics.go#L20) maintenanceTaskInfo = | GaugeVec | "pd" / "maintenance" / "task_info" | "Current maintenance task info in PD" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "maintenance",<br>			Name:      "task_info",<br>			Help:      "Current maintenance task info in PD",<br>		}, []string{"task_type", "task_id"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-038 [pkg/mcs/resourcemanager/server/metrics.go:52](../../../pkg/mcs/resourcemanager/server/metrics.go#L52) readRequestUnitCost = | CounterVec | namespace / ruSubsystem / "read_request_unit_sum" | "Counter of the read request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "read_request_unit_sum",<br>			Help:      "Counter of the read request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-039 [pkg/mcs/resourcemanager/server/metrics.go:59](../../../pkg/mcs/resourcemanager/server/metrics.go#L59) writeRequestUnitCost = | CounterVec | namespace / ruSubsystem / "write_request_unit_sum" | "Counter of the write request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "write_request_unit_sum",<br>			Help:      "Counter of the write request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-040 [pkg/mcs/resourcemanager/server/metrics.go:66](../../../pkg/mcs/resourcemanager/server/metrics.go#L66) activeRequestUnitCost = | CounterVec | namespace / ruSubsystem / "active_request_unit_sum" | "Counter of the active request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "active_request_unit_sum",<br>			Help:      "Counter of the active request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-041 [pkg/mcs/resourcemanager/server/metrics.go:75](../../../pkg/mcs/resourcemanager/server/metrics.go#L75) requestUnitV2Cost = | CounterVec | namespace / ruSubsystem / "request_unit_v2_sum" | "Counter of the total experimental v2 request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "request_unit_v2_sum",<br>			Help:      "Counter of the total experimental v2 request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-042 [pkg/mcs/resourcemanager/server/metrics.go:82](../../../pkg/mcs/resourcemanager/server/metrics.go#L82) tikvRequestUnitV2Cost = | CounterVec | namespace / ruSubsystem / "tikv_request_unit_v2_sum" | "Counter of the TiKV-side experimental v2 request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "tikv_request_unit_v2_sum",<br>			Help:      "Counter of the TiKV-side experimental v2 request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-043 [pkg/mcs/resourcemanager/server/metrics.go:89](../../../pkg/mcs/resourcemanager/server/metrics.go#L89) tidbRequestUnitV2Cost = | CounterVec | namespace / ruSubsystem / "tidb_request_unit_v2_sum" | "Counter of the TiDB-side experimental v2 request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "tidb_request_unit_v2_sum",<br>			Help:      "Counter of the TiDB-side experimental v2 request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-044 [pkg/mcs/resourcemanager/server/metrics.go:96](../../../pkg/mcs/resourcemanager/server/metrics.go#L96) tiflashRequestUnitV2Cost = | CounterVec | namespace / ruSubsystem / "tiflash_request_unit_v2_sum" | "Counter of the TiFlash-side experimental v2 request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "tiflash_request_unit_v2_sum",<br>			Help:      "Counter of the TiFlash-side experimental v2 request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-045 [pkg/mcs/resourcemanager/server/metrics.go:104](../../../pkg/mcs/resourcemanager/server/metrics.go#L104) readRequestUnitMaxPerSecCost = | GaugeVec | namespace / ruSubsystem / "read_request_unit_max_per_sec" | "Gauge of the max read request unit per second for all resource groups." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "read_request_unit_max_per_sec",<br>			Help:      "Gauge of the max read request unit per second for all resource groups.",<br>		}, []string{newResourceGroupNameLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-046 [pkg/mcs/resourcemanager/server/metrics.go:111](../../../pkg/mcs/resourcemanager/server/metrics.go#L111) writeRequestUnitMaxPerSecCost = | GaugeVec | namespace / ruSubsystem / "write_request_unit_max_per_sec" | "Gauge of the max write request unit per second for all resource groups." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "write_request_unit_max_per_sec",<br>			Help:      "Gauge of the max write request unit per second for all resource groups.",<br>		}, []string{newResourceGroupNameLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-047 [pkg/mcs/resourcemanager/server/metrics.go:119](../../../pkg/mcs/resourcemanager/server/metrics.go#L119) sqlLayerRequestUnitCost = | CounterVec | namespace / ruSubsystem / "sql_layer_request_unit_sum" | "The number of the sql layer request unit cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "sql_layer_request_unit_sum",<br>			Help:      "The number of the sql layer request unit cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-048 [pkg/mcs/resourcemanager/server/metrics.go:128](../../../pkg/mcs/resourcemanager/server/metrics.go#L128) readByteCost = | CounterVec | namespace / resourceSubsystem / "read_byte_sum" | "Counter of the read byte cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: resourceSubsystem,<br>			Name:      "read_byte_sum",<br>			Help:      "Counter of the read byte cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-049 [pkg/mcs/resourcemanager/server/metrics.go:135](../../../pkg/mcs/resourcemanager/server/metrics.go#L135) writeByteCost = | CounterVec | namespace / resourceSubsystem / "write_byte_sum" | "Counter of the write byte cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: resourceSubsystem,<br>			Name:      "write_byte_sum",<br>			Help:      "Counter of the write byte cost for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-050 [pkg/mcs/resourcemanager/server/metrics.go:142](../../../pkg/mcs/resourcemanager/server/metrics.go#L142) crossAZTrafficCost = | CounterVec | namespace / resourceSubsystem / "cross_az_traffic_byte_sum" | "Counter of the cross AZ traffic byte cost for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: resourceSubsystem,<br>			Name:      "cross_az_traffic_byte_sum",<br>			Help:      "Counter of the cross AZ traffic byte cost for all resource groups.",<br>		}, []string{newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-051 [pkg/mcs/resourcemanager/server/metrics.go:149](../../../pkg/mcs/resourcemanager/server/metrics.go#L149) kvCPUCost = | CounterVec | namespace / resourceSubsystem / "kv_cpu_time_ms_sum" | "Counter of the KV CPU time cost in milliseconds for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: resourceSubsystem,<br>			Name:      "kv_cpu_time_ms_sum",<br>			Help:      "Counter of the KV CPU time cost in milliseconds for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-052 [pkg/mcs/resourcemanager/server/metrics.go:156](../../../pkg/mcs/resourcemanager/server/metrics.go#L156) sqlCPUCost = | CounterVec | namespace / resourceSubsystem / "sql_cpu_time_ms_sum" | "Counter of the SQL CPU time cost in milliseconds for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: resourceSubsystem,<br>			Name:      "sql_cpu_time_ms_sum",<br>			Help:      "Counter of the SQL CPU time cost in milliseconds for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-053 [pkg/mcs/resourcemanager/server/metrics.go:163](../../../pkg/mcs/resourcemanager/server/metrics.go#L163) requestCount = | CounterVec | namespace / resourceSubsystem / "request_count" | "The number of read/write requests for all resource groups." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: resourceSubsystem,<br>			Name:      "request_count",<br>			Help:      "The number of read/write requests for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-054 [pkg/mcs/resourcemanager/server/metrics.go:171](../../../pkg/mcs/resourcemanager/server/metrics.go#L171) availableRUCounter = | GaugeVec | namespace / ruSubsystem / "available_ru" | "Counter of the available RU for all resource groups." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "available_ru",<br>			Help:      "Counter of the available RU for all resource groups.",<br>		}, []string{resourceGroupNameLabel, newResourceGroupNameLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-055 [pkg/mcs/resourcemanager/server/metrics.go:179](../../../pkg/mcs/resourcemanager/server/metrics.go#L179) resourceGroupConfigGauge = | GaugeVec | namespace / serverSubsystem / "group_config" | "Config of the resource group." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "group_config",<br>			Help:      "Config of the resource group.",<br>		}, []string{newResourceGroupNameLabel, typeLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-056 [pkg/mcs/resourcemanager/server/metrics.go:187](../../../pkg/mcs/resourcemanager/server/metrics.go#L187) sampledRequestUnitPerSec = | GaugeVec | namespace / ruSubsystem / "sampled_request_unit_per_sec" | "Gauge of the sampled RU/s for all resource groups." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "sampled_request_unit_per_sec",<br>			Help:      "Gauge of the sampled RU/s for all resource groups.",<br>		}, []string{newResourceGroupNameLabel, keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-057 [pkg/mcs/resourcemanager/server/metrics.go:195](../../../pkg/mcs/resourcemanager/server/metrics.go#L195) overrideSettings = | GaugeVec | namespace / ruSubsystem / "override_settings" | "Gauge of the override settings for all resource groups." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "override_settings",<br>			Help:      "Gauge of the override settings for all resource groups.",<br>		}, []string{newResourceGroupNameLabel, keyspaceNameLabel, typeLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-058 [pkg/mcs/resourcemanager/server/metrics.go:203](../../../pkg/mcs/resourcemanager/server/metrics.go#L203) serviceLimit = | GaugeVec | namespace / ruSubsystem / "service_limit" | "Gauge of the total RU limit of specific keyspace." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "service_limit",<br>			Help:      "Gauge of the total RU limit of specific keyspace.",<br>		}, []string{keyspaceNameLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-059 [pkg/mcs/resourcemanager/server/metrics.go:211](../../../pkg/mcs/resourcemanager/server/metrics.go#L211) pushRUMetricsDuration = | Histogram | namespace / ruSubsystem / "push_ru_metrics_duration_seconds" | "The duration of pushing RU metrics to Prometheus." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: ruSubsystem,<br>			Name:      "push_ru_metrics_duration_seconds",<br>			Help:      "The duration of pushing RU metrics to Prometheus.",<br>			Buckets:   prometheus.DefBuckets,<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-060 [pkg/mcs/router/server/metrics.go:27](../../../pkg/mcs/router/server/metrics.go#L27) queryRegionDuration = | Histogram | namespace / serverSubsystem / "query_region_duration_seconds" | "Bucketed histogram of processing time (s) of region query requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "query_region_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of region query requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-061 [pkg/mcs/router/server/metrics.go:36](../../../pkg/mcs/router/server/metrics.go#L36) regionRequestCounter = | CounterVec | namespace / serverSubsystem / "region_request_cnt" | "Counter of region request." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "region_request_cnt",<br>			Help:      "Counter of region request.",<br>		}, []string{"request", "caller_id", "caller_component", "event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-062 [pkg/mcs/router/server/metrics.go:44](../../../pkg/mcs/router/server/metrics.go#L44) regionSyncerStatus = | GaugeVec | namespace / serverSubsystem / "region_syncer_status" | "Inner status of the region syncer." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "region_syncer_status",<br>			Help:      "Inner status of the region syncer.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-063 [pkg/mcs/scheduling/server/metrics.go:26](../../../pkg/mcs/scheduling/server/metrics.go#L26) storeHeartbeatHandleDuration = | HistogramVec | namespace / serverSubsystem / "handle_store_heartbeat_duration_seconds" | "Bucketed histogram of processing time (s) of handled store heartbeat requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "handle_store_heartbeat_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled store heartbeat requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 29), // 0.1ms ~ 7hours<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-064 [pkg/mcs/scheduling/server/metrics.go:35](../../../pkg/mcs/scheduling/server/metrics.go#L35) storeHeartbeatCounter = | CounterVec | namespace / serverSubsystem / "store_heartbeat_total" | "Counter of store heartbeat requests." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "store_heartbeat_total",<br>			Help:      "Counter of store heartbeat requests.",<br>		}, []string{"address", "store", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-065 [pkg/mcs/scheduling/server/metrics.go:44](../../../pkg/mcs/scheduling/server/metrics.go#L44) regionHeartbeatHandleDuration = | HistogramVec | namespace / serverSubsystem / "handle_region_heartbeat_duration_seconds" | "Bucketed histogram of processing time (s) of handled region heartbeat requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "handle_region_heartbeat_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled region heartbeat requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 29), // 0.1ms ~ 7hours<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-066 [pkg/mcs/scheduling/server/metrics.go:53](../../../pkg/mcs/scheduling/server/metrics.go#L53) regionHeartbeatCounter = | CounterVec | namespace / serverSubsystem / "region_heartbeat_total" | "Counter of region heartbeat requests." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "region_heartbeat_total",<br>			Help:      "Counter of region heartbeat requests.",<br>		}, []string{"address", "store", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-067 [pkg/mcs/scheduling/server/metrics.go:61](../../../pkg/mcs/scheduling/server/metrics.go#L61) regionBucketsHandleDuration = | HistogramVec | namespace / serverSubsystem / "handle_region_buckets_duration_seconds" | "Bucketed histogram of processing time (s) of handled region buckets requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "handle_region_buckets_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled region buckets requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 29), // 0.1ms ~ 7hours<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-068 [pkg/mcs/scheduling/server/metrics.go:70](../../../pkg/mcs/scheduling/server/metrics.go#L70) regionBucketsCounter = | CounterVec | namespace / serverSubsystem / "region_buckets_total" | "Counter of region buckets requests." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "region_buckets_total",<br>			Help:      "Counter of region buckets requests.",<br>		}, []string{"address", "store", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-069 [pkg/mcs/scheduling/server/metrics.go:78](../../../pkg/mcs/scheduling/server/metrics.go#L78) regionBucketsReportInterval = | HistogramVec | namespace / serverSubsystem / "region_buckets_report_interval_seconds" | "Bucketed histogram of processing time (s) of region buckets report intervals." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: serverSubsystem,<br>			Name:      "region_buckets_report_interval_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of region buckets report intervals.",<br>			Buckets:   prometheus.LinearBuckets(0, 30, 20), // 1s ~ 17m<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-070 [pkg/mcs/tso/server/metrics.go:22](../../../pkg/mcs/tso/server/metrics.go#L22) timeJumpBackCounter = | Counter | namespace / "monitor" / "time_jump_back_total" | "Counter of system time jumps backward." | prometheus.NewCounter(<br>		prometheus.CounterOpts{<br>			Namespace: namespace,<br>			Subsystem: "monitor",<br>			Name:      "time_jump_back_total",<br>			Help:      "Counter of system time jumps backward.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-071 [pkg/mcs/tso/server/metrics.go:30](../../../pkg/mcs/tso/server/metrics.go#L30) metaDataGauge = | GaugeVec | namespace / "cluster" / "metadata" | "Record critical metadata." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: namespace,<br>			Subsystem: "cluster",<br>			Name:      "metadata",<br>			Help:      "Record critical metadata.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-072 [pkg/mcs/tso/server/metrics.go:38](../../../pkg/mcs/tso/server/metrics.go#L38) tsoHandleDuration = | HistogramVec | namespace / "server" / "handle_tso_duration_seconds" | "Bucketed histogram of processing time (s) of handled tso requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: namespace,<br>			Subsystem: "server",<br>			Name:      "handle_tso_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled tso requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{"group"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-073 [pkg/member/metrics.go:21](../../../pkg/member/metrics.go#L21) ServiceMemberGauge = | GaugeVec | "service" / "member" / "role" | "The leader/primary of services" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "service",<br>			Subsystem: "member",<br>			Name:      "role",<br>			Help:      "The leader/primary of services",<br>		}, []string{"service"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-074 [pkg/member/metrics.go:29](../../../pkg/member/metrics.go#L29) memberLeaderPriorityGauge = | GaugeVec | "pd" / "server" / "member_leader_priority" | "Etcd leader priority of this PD instance." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "member_leader_priority",<br>			Help:      "Etcd leader priority of this PD instance.",<br>		}, []string{}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-075 [pkg/progress/metrics.go:20](../../../pkg/progress/metrics.go#L20) storesProgressGauge = | GaugeVec | "pd" / "cluster" / "progress" | "The current progress of corresponding action" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "progress",<br>			Help:      "The current progress of corresponding action",<br>		}, []string{"address", "store", "action"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-076 [pkg/progress/metrics.go:28](../../../pkg/progress/metrics.go#L28) storesSpeedGauge = | GaugeVec | "pd" / "cluster" / "speed" | "The current speed of corresponding action" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "speed",<br>			Help:      "The current speed of corresponding action",<br>		}, []string{"address", "store", "action"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-077 [pkg/progress/metrics.go:36](../../../pkg/progress/metrics.go#L36) storesETAGauge = | GaugeVec | "pd" / "cluster" / "eta" | "The ETA of corresponding action" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "eta",<br>			Help:      "The ETA of corresponding action",<br>		}, []string{"address", "store", "action"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-078 [pkg/ratelimit/metrics.go:27](../../../pkg/ratelimit/metrics.go#L27) runnerTaskMaxWaitingDuration = | GaugeVec | "pd" / "ratelimit" / "runner_task_max_waiting_duration_seconds" | "The duration of tasks waiting in the runner." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "ratelimit",<br>			Name:      "runner_task_max_waiting_duration_seconds",<br>			Help:      "The duration of tasks waiting in the runner.",<br>		}, []string{nameStr}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-079 [pkg/ratelimit/metrics.go:34](../../../pkg/ratelimit/metrics.go#L34) runnerPendingTasks = | GaugeVec | "pd" / "ratelimit" / "runner_pending_tasks" | "The number of pending tasks in the runner." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "ratelimit",<br>			Name:      "runner_pending_tasks",<br>			Help:      "The number of pending tasks in the runner.",<br>		}, []string{nameStr, taskStr}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-080 [pkg/ratelimit/metrics.go:41](../../../pkg/ratelimit/metrics.go#L41) runnerFailedTasks = | CounterVec | "pd" / "ratelimit" / "runner_failed_tasks_total" | "The number of failed tasks in the runner." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "ratelimit",<br>			Name:      "runner_failed_tasks_total",<br>			Help:      "The number of failed tasks in the runner.",<br>		}, []string{nameStr, taskStr}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-081 [pkg/ratelimit/metrics.go:48](../../../pkg/ratelimit/metrics.go#L48) runnerSucceededTasks = | CounterVec | "pd" / "ratelimit" / "runner_success_tasks_total" | "The number of tasks in the runner." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "ratelimit",<br>			Name:      "runner_success_tasks_total",<br>			Help:      "The number of tasks in the runner.",<br>		}, []string{nameStr, taskStr}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-082 [pkg/ratelimit/metrics.go:55](../../../pkg/ratelimit/metrics.go#L55) runnerTaskExecutionDuration = | HistogramVec | "pd" / "ratelimit" / "runner_task_execution_duration_seconds" | "Bucketed histogram of processing time (s) of finished tasks." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "ratelimit",<br>			Name:      "runner_task_execution_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of finished tasks.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{nameStr, taskStr}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-083 [pkg/replication/metrics.go:20](../../../pkg/replication/metrics.go#L20) drTickCounter = | Counter | "pd" / "replication" / "dr_tick_total" | "Counter of background state check count" | prometheus.NewCounter(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "replication",<br>			Name:      "dr_tick_total",<br>			Help:      "Counter of background state check count",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-084 [pkg/replication/metrics.go:28](../../../pkg/replication/metrics.go#L28) drStateGauge = | Gauge | "pd" / "replication" / "dr_state" | "State of DR" | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "replication",<br>			Name:      "dr_state",<br>			Help:      "State of DR",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-085 [pkg/replication/metrics.go:36](../../../pkg/replication/metrics.go#L36) drStateIDGauge = | Gauge | "pd" / "replication" / "dr_state_id" | "State ID of DR" | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "replication",<br>			Name:      "dr_state_id",<br>			Help:      "State ID of DR",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-086 [pkg/replication/metrics.go:44](../../../pkg/replication/metrics.go#L44) drRecoveredRegionGauge = | Gauge | "pd" / "replication" / "dr_recovered_region" | "Number of recovered regions" | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "replication",<br>			Name:      "dr_recovered_region",<br>			Help:      "Number of recovered regions",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-087 [pkg/replication/metrics.go:52](../../../pkg/replication/metrics.go#L52) drRecoverProgressGauge = | Gauge | "pd" / "replication" / "dr_recover_progress" | "Progress of sync_recover process" | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "replication",<br>			Name:      "dr_recover_progress",<br>			Help:      "Progress of sync_recover process",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 主题8；78/102边界；逐项未验收 |
| M-088 [pkg/schedule/affinity/metrics.go:20](../../../pkg/schedule/affinity/metrics.go#L20) affinityStatusGauge = | GaugeVec | "pd" / "affinity" / "status" | "Status of the affinity manager." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "affinity",<br>			Name:      "status",<br>			Help:      "Status of the affinity manager.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-089 [pkg/schedule/checker/metrics.go:20](../../../pkg/schedule/checker/metrics.go#L20) checkerCounter = | CounterVec | "pd" / "checker" / "event_count" | "Counter of checker events." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "event_count",<br>			Help:      "Counter of checker events.",<br>		}, []string{"type", "name"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-090 [pkg/schedule/checker/metrics.go:27](../../../pkg/schedule/checker/metrics.go#L27) regionListGauge = | GaugeVec | "pd" / "checker" / "region_list" | "Number of region about different type." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "region_list",<br>			Help:      "Number of region about different type.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-091 [pkg/schedule/checker/metrics.go:35](../../../pkg/schedule/checker/metrics.go#L35) patrolCheckRegionsGauge = | Gauge | "pd" / "checker" / "patrol_regions_time" | "Time spent of patrol checks region." | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "patrol_regions_time",<br>			Help:      "Time spent of patrol checks region.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-092 [pkg/schedule/checker/metrics.go:43](../../../pkg/schedule/checker/metrics.go#L43) patrolPhaseDuration = | HistogramVec | "pd" / "checker" / "patrol_phase_duration_seconds" | "Bucketed histogram of duration for patrol phases." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "patrol_phase_duration_seconds",<br>			Help:      "Bucketed histogram of duration for patrol phases.",<br>			Buckets:   prometheus.DefBuckets,<br>		}, []string{"phase"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-093 [pkg/schedule/checker/metrics.go:52](../../../pkg/schedule/checker/metrics.go#L52) checkRegionDuration = | HistogramVec | "pd" / "checker" / "check_region_duration_seconds" | "Bucketed histogram of duration for checking region in checkers." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "check_region_duration_seconds",<br>			Help:      "Bucketed histogram of duration for checking region in checkers.",<br>			Buckets:   prometheus.DefBuckets,<br>		}, []string{"checker"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-094 [pkg/schedule/checker/metrics.go:61](../../../pkg/schedule/checker/metrics.go#L61) patrolRegionChannelSize = | Gauge | "pd" / "checker" / "patrol_region_channel_size" | "Size of patrol region channel." | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "patrol_region_channel_size",<br>			Help:      "Size of patrol region channel.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-095 [pkg/schedule/checker/metrics.go:68](../../../pkg/schedule/checker/metrics.go#L68) splitScatterPendingExpiredCounter = | CounterVec | "pd" / "checker" / "split_scatter_pending_expired_total" | "Counter of expired split-scatter pending entries by whether they were selected for dispatch." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "split_scatter_pending_expired_total",<br>			Help:      "Counter of expired split-scatter pending entries by whether they were selected for dispatch.",<br>		}, []string{"attempted"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-096 [pkg/schedule/checker/metrics.go:75](../../../pkg/schedule/checker/metrics.go#L75) splitScatterPendingDroppedCounter = | Counter | "pd" / "checker" / "split_scatter_pending_dropped_total" | "Counter of split-scatter pending entries dropped by the capacity limit." | prometheus.NewCounter(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "split_scatter_pending_dropped_total",<br>			Help:      "Counter of split-scatter pending entries dropped by the capacity limit.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-097 [pkg/schedule/checker/metrics.go:82](../../../pkg/schedule/checker/metrics.go#L82) splitScatterPendingGauge = | Gauge | "pd" / "checker" / "split_scatter_pending" | "Number of split-scatter pending entries." | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "checker",<br>			Name:      "split_scatter_pending",<br>			Help:      "Number of split-scatter pending entries.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-098 [pkg/schedule/filter/metrics.go:20](../../../pkg/schedule/filter/metrics.go#L20) filterSourceCounter = | CounterVec | "pd" / "schedule" / "source_filter" | "Counter of the source filter" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "source_filter",<br>			Help:      "Counter of the source filter",<br>		}, []string{"scope", "type", "source"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-099 [pkg/schedule/filter/metrics.go:27](../../../pkg/schedule/filter/metrics.go#L27) filterTargetCounter = | CounterVec | "pd" / "schedule" / "target_filter" | "Counter of the target filter" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "target_filter",<br>			Help:      "Counter of the target filter",<br>		}, []string{"scope", "type", "target"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-100 [pkg/schedule/hbstream/metric.go:21](../../../pkg/schedule/hbstream/metric.go#L21) heartbeatStreamCounter = | CounterVec | "pd" / "hbstream" / "region_message" | "Counter of message hbstream sent." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "hbstream",<br>			Name:      "region_message",<br>			Help:      "Counter of message hbstream sent.",<br>		}, []string{"address", "store", "type", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-101 [pkg/schedule/labeler/metrics.go:21](../../../pkg/schedule/labeler/metrics.go#L21) LabelerEventCounter = | CounterVec | "pd" / "schedule" / "labeler_event_counter" | "Counter of the scheduler label." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "labeler_event_counter",<br>			Help:      "Counter of the scheduler label.",<br>		}, []string{"type", "event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-102 [pkg/schedule/labeler/metrics.go:29](../../../pkg/schedule/labeler/metrics.go#L29) newRegionLabelerDuration = | Histogram | "pd" / "schedule" / "new_region_labeler_duration_seconds" | "Bucketed histogram of processing time (s) of creating new region labeler." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "new_region_labeler_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of creating new region labeler.",<br>			Buckets:   prometheus.ExponentialBuckets(0.001, 2, 13), // 1ms ~ 4s<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-103 [pkg/schedule/metrics.go:20](../../../pkg/schedule/metrics.go#L20) hotSpotStatusGauge = | GaugeVec | "pd" / "hotspot" / "status" | "Status of the hotspot." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "hotspot",<br>			Name:      "status",<br>			Help:      "Status of the hotspot.",<br>		}, []string{"address", "store", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-104 [pkg/schedule/operator/metrics.go:24](../../../pkg/schedule/operator/metrics.go#L24) operatorStepDuration = | HistogramVec | "pd" / "schedule" / "finish_operator_steps_duration_seconds" | "Bucketed histogram of processing time (s) of finished operator step." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "finish_operator_steps_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of finished operator step.",<br>			Buckets:   []float64{0.5, 1, 2, 4, 8, 16, 20, 40, 60, 90, 120, 180, 240, 300, 480, 600, 720, 900, 1200, 1800, 3600},<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-105 [pkg/schedule/operator/metrics.go:33](../../../pkg/schedule/operator/metrics.go#L33) operatorLimitCounter = | CounterVec | "pd" / "schedule" / "operator_limit" | "Counter of operator meeting limit" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "operator_limit",<br>			Help:      "Counter of operator meeting limit",<br>		}, []string{"type", "name"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-106 [pkg/schedule/operator/metrics.go:42](../../../pkg/schedule/operator/metrics.go#L42) OperatorExceededStoreLimitCounter = | CounterVec | "pd" / "schedule" / "operator_exceeded_store_limit" | "Counter of operator meeting store limit" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "operator_exceeded_store_limit",<br>			Help:      "Counter of operator meeting store limit",<br>		}, []string{"desc"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-107 [pkg/schedule/operator/metrics.go:51](../../../pkg/schedule/operator/metrics.go#L51) operatorCounter = | CounterVec | "pd" / "schedule" / "operators_count" | "Counter of schedule operators." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "operators_count",<br>			Help:      "Counter of schedule operators.",<br>		}, []string{"type", "event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-108 [pkg/schedule/operator/metrics.go:59](../../../pkg/schedule/operator/metrics.go#L59) operatorDuration = | HistogramVec | "pd" / "schedule" / "finish_operators_duration_seconds" | "Bucketed histogram of processing time (s) of finished operator." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "finish_operators_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of finished operator.",<br>			Buckets:   []float64{0.5, 1, 2, 4, 8, 16, 20, 40, 60, 90, 120, 180, 240, 300, 480, 600, 720, 900, 1200, 1800, 3600},<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-109 [pkg/schedule/operator/metrics.go:68](../../../pkg/schedule/operator/metrics.go#L68) operatorSizeHist = | HistogramVec | "pd" / "schedule" / "operator_region_size" | "Bucketed histogram of the operator region size." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "operator_region_size",<br>			Help:      "Bucketed histogram of the operator region size.",<br>			Buckets:   prometheus.ExponentialBuckets(1, 2, 20), // 1MB~1TB<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-110 [pkg/schedule/scatter/metrics.go:20](../../../pkg/schedule/scatter/metrics.go#L20) scatterCounter = | CounterVec | "pd" / "schedule" / "scatter_operators_count" | "Counter of region scatter operators." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "scatter_operators_count",<br>			Help:      "Counter of region scatter operators.",<br>		}, []string{"type", "event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-111 [pkg/schedule/scatter/metrics.go:28](../../../pkg/schedule/scatter/metrics.go#L28) scatterDistributionCounter = | CounterVec | "pd" / "schedule" / "scatter_distribution" | "Counter of the distribution in scatter." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "schedule",<br>			Name:      "scatter_distribution",<br>			Help:      "Counter of the distribution in scatter.",<br>		}, []string{"store", "is_leader", "engine"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-112 [pkg/schedule/schedulers/metrics.go:24](../../../pkg/schedule/schedulers/metrics.go#L24) schedulerStatusGauge = | GaugeVec | "pd" / "scheduler" / "status" | "Status of the scheduler." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "status",<br>			Help:      "Status of the scheduler.",<br>		}, []string{"kind", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-113 [pkg/schedule/schedulers/metrics.go:32](../../../pkg/schedule/schedulers/metrics.go#L32) schedulerCounter = | CounterVec | "pd" / "scheduler" / "event_count" | "Counter of scheduler events." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "event_count",<br>			Help:      "Counter of scheduler events.",<br>		}, []string{"type", "name"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-114 [pkg/schedule/schedulers/metrics.go:41](../../../pkg/schedule/schedulers/metrics.go#L41) opInfluenceStatus = | GaugeVec | "pd" / "scheduler" / "op_influence" | "Store status for schedule" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "op_influence",<br>			Help:      "Store status for schedule",<br>		}, []string{"scheduler", "store", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-115 [pkg/schedule/schedulers/metrics.go:50](../../../pkg/schedule/schedulers/metrics.go#L50) tolerantResourceStatus = | GaugeVec | "pd" / "scheduler" / "tolerant_resource" | "Store status for schedule" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "tolerant_resource",<br>			Help:      "Store status for schedule",<br>		}, []string{"scheduler"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-116 [pkg/schedule/schedulers/metrics.go:58](../../../pkg/schedule/schedulers/metrics.go#L58) balanceWitnessCounter = | CounterVec | "pd" / "scheduler" / "balance_witness" | "Counter of balance witness scheduler." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "balance_witness",<br>			Help:      "Counter of balance witness scheduler.",<br>		}, []string{"type", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-117 [pkg/schedule/schedulers/metrics.go:67](../../../pkg/schedule/schedulers/metrics.go#L67) hotSchedulerResultCounter = | CounterVec | "pd" / "scheduler" / "hot_region" | "Counter of hot region scheduler." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "hot_region",<br>			Help:      "Counter of hot region scheduler.",<br>		}, []string{"type", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-118 [pkg/schedule/schedulers/metrics.go:75](../../../pkg/schedule/schedulers/metrics.go#L75) balanceDirectionCounter = | CounterVec | "pd" / "scheduler" / "balance_direction" | "Counter of direction of balance related schedulers." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "balance_direction",<br>			Help:      "Counter of direction of balance related schedulers.",<br>		}, []string{"type", "store", "direction"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-119 [pkg/schedule/schedulers/metrics.go:84](../../../pkg/schedule/schedulers/metrics.go#L84) hotDirectionCounter = | CounterVec | "pd" / "scheduler" / "hot_region_direction" | "Counter of hot region scheduler." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "hot_region_direction",<br>			Help:      "Counter of hot region scheduler.",<br>		}, []string{"type", "rw", "store", "direction", "dim"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-120 [pkg/schedule/schedulers/metrics.go:92](../../../pkg/schedule/schedulers/metrics.go#L92) hotPeerHist = | HistogramVec | "pd" / "scheduler" / "hot_peer" | "Bucketed histogram of the scheduling hot peer." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "hot_peer",<br>			Help:      "Bucketed histogram of the scheduling hot peer.",<br>			Buckets:   prometheus.ExponentialBuckets(1, 2, 30),<br>		}, []string{"type", "rw", "dim"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-121 [pkg/schedule/schedulers/metrics.go:101](../../../pkg/schedule/schedulers/metrics.go#L101) evictedSlowStoreStatusGauge = | GaugeVec | "pd" / "scheduler" / "evicted_slow_store_status" | "Store evicted status due to slow" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "evicted_slow_store_status",<br>			Help:      "Store evicted status due to slow",<br>		}, []string{"store", "slow_type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-122 [pkg/schedule/schedulers/metrics.go:109](../../../pkg/schedule/schedulers/metrics.go#L109) evictedStoppingStoreStatusGauge = | GaugeVec | "pd" / "scheduler" / "evicted_stopping_store_status" | "Store evicted status due to stopping" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "evicted_stopping_store_status",<br>			Help:      "Store evicted status due to stopping",<br>		}, []string{"store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-123 [pkg/schedule/schedulers/metrics.go:117](../../../pkg/schedule/schedulers/metrics.go#L117) slowStoreTriggerLimitGauge = | GaugeVec | "pd" / "scheduler" / "slow_store_trigger_limit" | "slow store trigger limit" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "slow_store_trigger_limit",<br>			Help:      "slow store trigger limit",<br>		}, []string{"store", "slow_type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-124 [pkg/schedule/schedulers/metrics.go:125](../../../pkg/schedule/schedulers/metrics.go#L125) storeSlowTrendEvictedStatusGauge = | GaugeVec | "pd" / "scheduler" / "store_slow_trend_evicted_status" | "Store evicted by slow trend status for schedule" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "store_slow_trend_evicted_status",<br>			Help:      "Store evicted by slow trend status for schedule",<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-125 [pkg/schedule/schedulers/metrics.go:133](../../../pkg/schedule/schedulers/metrics.go#L133) storeSlowTrendActionStatusGauge = | GaugeVec | "pd" / "scheduler" / "store_slow_trend_action_status" | "Store trend scheduler calculating actions" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "store_slow_trend_action_status",<br>			Help:      "Store trend scheduler calculating actions",<br>		}, []string{"type", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-126 [pkg/schedule/schedulers/metrics.go:141](../../../pkg/schedule/schedulers/metrics.go#L141) storeSlowTrendMiscGauge = | GaugeVec | "pd" / "scheduler" / "store_slow_trend_misc" | "Store trend internal uncatalogued values" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "store_slow_trend_misc",<br>			Help:      "Store trend internal uncatalogued values",<br>		}, []string{"type", "dim"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-127 [pkg/schedule/schedulers/metrics.go:150](../../../pkg/schedule/schedulers/metrics.go#L150) HotPendingSum = | GaugeVec | "pd" / "scheduler" / "hot_pending_sum" | "Pending influence sum of store in hot region scheduler." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "hot_pending_sum",<br>			Help:      "Pending influence sum of store in hot region scheduler.",<br>		}, []string{"store", "rw", "dim"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-128 [pkg/schedule/schedulers/metrics.go:158](../../../pkg/schedule/schedulers/metrics.go#L158) ruleStatusGauge = | GaugeVec | "pd" / "rule_manager" / "status" | "Status of the rule." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "rule_manager",<br>			Name:      "status",<br>			Help:      "Status of the rule.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-129 [pkg/schedule/schedulers/metrics.go:166](../../../pkg/schedule/schedulers/metrics.go#L166) regionLabelStatusGauge = | GaugeVec | "pd" / "region_label" / "status" | "Status of the region labeler." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "region_label",<br>			Name:      "status",<br>			Help:      "Status of the region labeler.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-130 [pkg/schedule/schedulers/metrics.go:174](../../../pkg/schedule/schedulers/metrics.go#L174) balanceRangeGauge = | GaugeVec | "pd" / "balance_range" / "store" | "Store status for balance range schedule" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "balance_range",<br>			Name:      "store",<br>			Help:      "Store status for balance range schedule",<br>		}, []string{"store", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-131 [pkg/schedule/schedulers/metrics.go:181](../../../pkg/schedule/schedulers/metrics.go#L181) balanceRangeJobGauge = | GaugeVec | "pd" / "balance_range" / "job" | "job status for balance range schedule" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "balance_range",<br>			Name:      "job",<br>			Help:      "job status for balance range schedule",<br>		}, []string{}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-132 [pkg/statistics/buckets/metric.go:22](../../../pkg/statistics/buckets/metric.go#L22) bucketsTaskDuration = | HistogramVec | "pd" / "scheduler" / "bucket_task_duration" | "Bucketed histogram of processing time (s) of bucket task." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "bucket_task_duration",<br>			Help:      "Bucketed histogram of processing time (s) of bucket task.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 23), // 0.1ms ~ 14m<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-133 [pkg/statistics/metrics.go:26](../../../pkg/statistics/metrics.go#L26) hotCacheStatusGauge = | GaugeVec | "pd" / "hotcache" / "status" | "Status of the hotspot." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "hotcache",<br>			Name:      "status",<br>			Help:      "Status of the hotspot.",<br>		}, []string{"name", "store", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-134 [pkg/statistics/metrics.go:34](../../../pkg/statistics/metrics.go#L34) storeStatusGauge = | GaugeVec | "pd" / "scheduler" / "store_status" | "Store status for schedule" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "store_status",<br>			Help:      "Store status for schedule",<br>		}, []string{"address", "store", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-135 [pkg/statistics/metrics.go:42](../../../pkg/statistics/metrics.go#L42) regionStatusGauge = | GaugeVec | "pd" / "regions" / "status" | "Status of the regions." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "regions",<br>			Name:      "status",<br>			Help:      "Status of the regions.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-136 [pkg/statistics/metrics.go:50](../../../pkg/statistics/metrics.go#L50) clusterStatusGauge = | GaugeVec | "pd" / "cluster" / "status" | "Status of the cluster." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "status",<br>			Help:      "Status of the cluster.",<br>		}, []string{"type", "engine", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-137 [pkg/statistics/metrics.go:58](../../../pkg/statistics/metrics.go#L58) placementStatusGauge = | GaugeVec | "pd" / "cluster" / "placement_status" | "Status of the cluster placement." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "placement_status",<br>			Help:      "Status of the cluster placement.",<br>		}, []string{"type", "name", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-138 [pkg/statistics/metrics.go:66](../../../pkg/statistics/metrics.go#L66) configStatusGauge = | GaugeVec | "pd" / "config" / "status" | "Status of the scheduling configurations." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "config",<br>			Name:      "status",<br>			Help:      "Status of the scheduling configurations.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-139 [pkg/statistics/metrics.go:75](../../../pkg/statistics/metrics.go#L75) StoreLimitGauge = | GaugeVec | "pd" / "cluster" / "store_limit" | "Status of the store limit." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "store_limit",<br>			Help:      "Status of the store limit.",<br>		}, []string{"store", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-140 [pkg/statistics/metrics.go:83](../../../pkg/statistics/metrics.go#L83) regionLabelLevelGauge = | GaugeVec | "pd" / "regions" / "label_level" | "Number of regions in the different label level." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "regions",<br>			Name:      "label_level",<br>			Help:      "Number of regions in the different label level.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-141 [pkg/statistics/metrics.go:91](../../../pkg/statistics/metrics.go#L91) regionAbnormalPeerDuration = | HistogramVec | "pd" / "regions" / "abnormal_peer_duration_seconds" | "Bucketed histogram of processing time (s) of handled success cmds." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "regions",<br>			Name:      "abnormal_peer_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled success cmds.",<br>			Buckets:   prometheus.ExponentialBuckets(1, 1.4, 30), // 1s ~ 6.72 hours<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-142 [pkg/statistics/metrics.go:100](../../../pkg/statistics/metrics.go#L100) hotPeerSummary = | GaugeVec | "pd" / "scheduler" / "hot_peers_summary" | "Hot peers summary for each store" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "hot_peers_summary",<br>			Help:      "Hot peers summary for each store",<br>		}, []string{"type", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-143 [pkg/storage/kv/metrics.go:20](../../../pkg/storage/kv/metrics.go#L20) txnCounter = | CounterVec | "pd" / "txn" / "txns_count" | "Counter of txns." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "txn",<br>			Name:      "txns_count",<br>			Help:      "Counter of txns.",<br>		}, []string{"result"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-144 [pkg/storage/kv/metrics.go:28](../../../pkg/storage/kv/metrics.go#L28) txnDuration = | HistogramVec | "pd" / "txn" / "handle_txns_duration_seconds" | "Bucketed histogram of processing time (s) of handled txns." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "txn",<br>			Name:      "handle_txns_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled txns.",<br>			// 1ms ~ 65s， according to the latency distribution of Txn in production<br>			Buckets: prometheus.ExponentialBuckets(0.001, 2, 17),<br>		}, []string{"result"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-145 [pkg/syncer/metrics.go:19](../../../pkg/syncer/metrics.go#L19) var regionSyncerStatus = | GaugeVec | "pd" / "region_syncer" / "status" | "Inner status of the region syncer." | prometheus.NewGaugeVec(<br>	prometheus.GaugeOpts{<br>		Namespace: "pd",<br>		Subsystem: "region_syncer",<br>		Name:      "status",<br>		Help:      "Inner status of the region syncer.",<br>	}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-146 [pkg/tso/metrics.go:33](../../../pkg/tso/metrics.go#L33) tsoCounter = | CounterVec | pdNamespace / "tso" / "events" | "Counter of tso events" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: pdNamespace,<br>			Subsystem: "tso",<br>			Name:      "events",<br>			Help:      "Counter of tso events",<br>		}, []string{typeLabel, groupLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-147 [pkg/tso/metrics.go:41](../../../pkg/tso/metrics.go#L41) tsoGauge = | GaugeVec | pdNamespace / "cluster" / "tso" | "Record of tso metadata." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: pdNamespace,<br>			Subsystem: "cluster",<br>			Name:      "tso",<br>			Help:      "Record of tso metadata.",<br>		}, []string{typeLabel, groupLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-148 [pkg/tso/metrics.go:49](../../../pkg/tso/metrics.go#L49) tsoGap = | GaugeVec | pdNamespace / "cluster" / "tso_gap_millionseconds" | "The minimal (non-zero) TSO gap" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: pdNamespace,<br>			Subsystem: "cluster",<br>			Name:      "tso_gap_millionseconds",<br>			Help:      "The minimal (non-zero) TSO gap",<br>		}, []string{groupLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-149 [pkg/tso/metrics.go:57](../../../pkg/tso/metrics.go#L57) tsoOpDuration = | HistogramVec | pdNamespace / "cluster" / "tso_operation_duration_seconds" | "Bucketed histogram of processing time(s) of the TSO operations." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: pdNamespace,<br>			Subsystem: "cluster",<br>			Name:      "tso_operation_duration_seconds",<br>			Help:      "Bucketed histogram of processing time(s) of the TSO operations.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{typeLabel, groupLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-150 [pkg/tso/metrics.go:66](../../../pkg/tso/metrics.go#L66) tsoAllocatorRole = | GaugeVec | pdNamespace / "tso" / "role" | "Indicate the PD server role info, whether it's a TSO allocator." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: pdNamespace,<br>			Subsystem: "tso",<br>			Name:      "role",<br>			Help:      "Indicate the PD server role info, whether it's a TSO allocator.",<br>		}, []string{groupLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-151 [pkg/tso/metrics.go:75](../../../pkg/tso/metrics.go#L75) keyspaceGroupStateGauge = | GaugeVec | tsoNamespace / "keyspace_group" / "state" | "Gauge of the Keyspace Group states." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: tsoNamespace,<br>			Subsystem: "keyspace_group",<br>			Name:      "state",<br>			Help:      "Gauge of the Keyspace Group states.",<br>		}, []string{typeLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-152 [pkg/tso/metrics.go:83](../../../pkg/tso/metrics.go#L83) keyspaceGroupOpDuration = | HistogramVec | tsoNamespace / "keyspace_group" / "operation_duration_seconds" | "Bucketed histogram of processing time(s) of the Keyspace Group operations." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: tsoNamespace,<br>			Subsystem: "keyspace_group",<br>			Name:      "operation_duration_seconds",<br>			Help:      "Bucketed histogram of processing time(s) of the Keyspace Group operations.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{typeLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-153 [pkg/tso/metrics.go:93](../../../pkg/tso/metrics.go#L93) keyspaceGroupKeyspaceCountGauge = | GaugeVec | tsoNamespace / "keyspace_group" / "keyspace_list_length" | "The length of keyspace list in each TSO keyspace group." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: tsoNamespace,<br>			Subsystem: "keyspace_group",<br>			Name:      "keyspace_list_length",<br>			Help:      "The length of keyspace list in each TSO keyspace group.",<br>		}, []string{groupLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-154 [pkg/utils/etcdutil/metrics.go:26](../../../pkg/utils/etcdutil/metrics.go#L26) etcdStateGauge = | GaugeVec | "pd" / "server" / "etcd_client" | "Etcd client states." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "etcd_client",<br>			Help:      "Etcd client states.",<br>		}, []string{sourceLabel, typeLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-155 [pkg/utils/etcdutil/metrics.go:34](../../../pkg/utils/etcdutil/metrics.go#L34) etcdEndpointLatency = | HistogramVec | "pd" / "server" / "etcd_endpoint_latency_seconds" | "Bucketed histogram of latency of health check." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "etcd_endpoint_latency_seconds",<br>			Help:      "Bucketed histogram of latency of health check.",<br>			Buckets:   []float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10},<br>		}, []string{sourceLabel, endpointLabel}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-156 [server/cluster/metrics.go:20](../../../server/cluster/metrics.go#L20) healthStatusGauge = | GaugeVec | "pd" / "cluster" / "health_status" | "Status of the cluster." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "health_status",<br>			Help:      "Status of the cluster.",<br>		}, []string{"name"}) | [cluster_metrics_worker.rs](../../../server/cluster/cluster_metrics_worker.rs)；15见[task-015](task-015.md)，86/101导出仍待验收 |
| M-157 [server/cluster/metrics.go:28](../../../server/cluster/metrics.go#L28) regionEventCounter = | CounterVec | "pd" / "cluster" / "region_event" | "Counter of the region event" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "region_event",<br>			Help:      "Counter of the region event",<br>		}, []string{"event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-158 [server/cluster/metrics.go:36](../../../server/cluster/metrics.go#L36) updateStoreStatsGauge = | Gauge | "pd" / "cluster" / "update_stores_stats_time" | "Time spent of updating store stats." | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "update_stores_stats_time",<br>			Help:      "Time spent of updating store stats.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-159 [server/cluster/metrics.go:44](../../../server/cluster/metrics.go#L44) clusterStateCPUGauge = | Gauge | "pd" / "server" / "cluster_state_cpu_usage" | "CPU usage to determine the cluster state" | prometheus.NewGauge(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "cluster_state_cpu_usage",<br>			Help:      "CPU usage to determine the cluster state",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-160 [server/cluster/metrics.go:51](../../../server/cluster/metrics.go#L51) clusterStateCurrent = | GaugeVec | "pd" / "server" / "cluster_state_current" | "Current state of the cluster" | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "cluster_state_current",<br>			Help:      "Current state of the cluster",<br>		}, []string{"state"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-161 [server/cluster/metrics.go:59](../../../server/cluster/metrics.go#L59) storeSyncConfigEvent = | CounterVec | "pd" / "cluster" / "store_sync" | "The state of store sync config" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "store_sync",<br>			Help:      "The state of store sync config",<br>		}, []string{"address", "state"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-162 [server/cluster/metrics.go:67](../../../server/cluster/metrics.go#L67) storeTriggerNetworkSlowEvict = | CounterVec | "pd" / "cluster" / "store_trigger_network_slow_evict" | "The count of store trigger network slow evict" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "store_trigger_network_slow_evict",<br>			Help:      "The count of store trigger network slow evict",<br>		}, []string{"store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-163 [server/cluster/metrics.go:75](../../../server/cluster/metrics.go#L75) raftClusterStartDuration = | HistogramVec | "pd" / "cluster" / "raftcluster_start_duration_seconds" | "Bucketed histogram of processing time (s) of raft cluster start." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "raftcluster_start_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of raft cluster start.",<br>			Buckets:   prometheus.ExponentialBuckets(0.001, 2, 13), // 1ms ~ 4s<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-164 [server/config/metrics.go:19](../../../server/config/metrics.go#L19) var schedulingAllowanceStatusGauge = | GaugeVec | "pd" / "scheduling" / "allowance_status" | "Status of the scheduling allowance." | prometheus.NewGaugeVec(<br>	prometheus.GaugeOpts{<br>		Namespace: "pd",<br>		Subsystem: "scheduling",<br>		Name:      "allowance_status",<br>		Help:      "Status of the scheduling allowance.",<br>	}, []string{"kind"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 47/90；逐项未验收 |
| M-165 [server/metrics.go:22](../../../server/metrics.go#L22) timeJumpBackCounter = | Counter | "pd" / "monitor" / "time_jump_back_total" | "Counter of system time jumps backward." | prometheus.NewCounter(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "monitor",<br>			Name:      "time_jump_back_total",<br>			Help:      "Counter of system time jumps backward.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-166 [server/metrics.go:29](../../../server/metrics.go#L29) bucketReportCounter = | CounterVec | "pd" / "server" / "bucket_report" | "Counter of bucket report." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "bucket_report",<br>			Help:      "Counter of bucket report.",<br>		}, []string{"address", "store", "type", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-167 [server/metrics.go:36](../../../server/metrics.go#L36) regionHeartbeatCounter = | CounterVec | "pd" / "scheduler" / "region_heartbeat" | "Counter of region heartbeat." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "region_heartbeat",<br>			Help:      "Counter of region heartbeat.",<br>		}, []string{"address", "store", "type", "status"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-168 [server/metrics.go:44](../../../server/metrics.go#L44) regionHeartbeatLatency = | HistogramVec | "pd" / "scheduler" / "region_heartbeat_latency_seconds" | "Bucketed histogram of latency (s) of receiving heartbeat." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "region_heartbeat_latency_seconds",<br>			Help:      "Bucketed histogram of latency (s) of receiving heartbeat.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-169 [server/metrics.go:53](../../../server/metrics.go#L53) metadataGauge = | GaugeVec | "pd" / "cluster" / "metadata" | "Record critical metadata." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "cluster",<br>			Name:      "metadata",<br>			Help:      "Record critical metadata.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-170 [server/metrics.go:61](../../../server/metrics.go#L61) etcdStateGauge = | GaugeVec | "pd" / "server" / "etcd_state" | "Etcd raft states." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "etcd_state",<br>			Help:      "Etcd raft states.",<br>		}, []string{"type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-171 [server/metrics.go:69](../../../server/metrics.go#L69) tsoProxyHandleDuration = | Histogram | "pd" / "server" / "handle_tso_proxy_duration_seconds" | "Bucketed histogram of processing time (s) of handled tso proxy requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "handle_tso_proxy_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled tso proxy requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-172 [server/metrics.go:78](../../../server/metrics.go#L78) tsoProxyBatchSize = | Histogram | "pd" / "server" / "handle_tso_proxy_batch_size" | "Bucketed histogram of the batch size of handled tso proxy requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "handle_tso_proxy_batch_size",<br>			Help:      "Bucketed histogram of the batch size of handled tso proxy requests.",<br>			Buckets:   prometheus.ExponentialBuckets(1, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-173 [server/metrics.go:87](../../../server/metrics.go#L87) tsoProxyForwardTimeoutCounter = | Counter | "pd" / "server" / "tso_proxy_forward_timeout_total" | "Counter of timeouts when tso proxy forwarding tso requests to tso service." | prometheus.NewCounter(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "tso_proxy_forward_timeout_total",<br>			Help:      "Counter of timeouts when tso proxy forwarding tso requests to tso service.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-174 [server/metrics.go:94](../../../server/metrics.go#L94) tsoForwardStreamCounter = | Counter | "pd" / "server" / "tso_forward_stream_total" | "Counter of TSO streams forwarded to the independent TSO service." | prometheus.NewCounter(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "tso_forward_stream_total",<br>			Help:      "Counter of TSO streams forwarded to the independent TSO service.",<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-175 [server/metrics.go:102](../../../server/metrics.go#L102) tsoHandleDuration = | Histogram | "pd" / "server" / "handle_tso_duration_seconds" | "Bucketed histogram of processing time (s) of handled tso requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "handle_tso_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled tso requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-176 [server/metrics.go:111](../../../server/metrics.go#L111) tsoBatchSize = | Histogram | "pd" / "server" / "handle_tso_batch_size" | "Bucketed histogram of the batch size of handled tso requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "handle_tso_batch_size",<br>			Help:      "Bucketed histogram of the batch size of handled tso requests.",<br>			Buckets:   prometheus.ExponentialBuckets(1, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-177 [server/metrics.go:120](../../../server/metrics.go#L120) queryRegionDuration = | Histogram | "pd" / "server" / "query_region_duration_seconds" | "Bucketed histogram of processing time (s) of region query requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "query_region_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of region query requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-178 [server/metrics.go:129](../../../server/metrics.go#L129) bucketReportLatency = | HistogramVec | "pd" / "server" / "handle_bucket_report_duration_seconds" | "Bucketed histogram of processing time (s) of handled bucket report requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "handle_bucket_report_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled bucket report requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 29), // 0.1ms ~ 7hours<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-179 [server/metrics.go:138](../../../server/metrics.go#L138) bucketReportInterval = | HistogramVec | "pd" / "server" / "bucket_report_interval_seconds" | "Bucketed histogram of processing time (s) of handled bucket report requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "bucket_report_interval_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled bucket report requests.",<br>			Buckets:   prometheus.LinearBuckets(0, 30, 20), // 1s ~ 17m<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-180 [server/metrics.go:147](../../../server/metrics.go#L147) regionHeartbeatHandleDuration = | HistogramVec | "pd" / "scheduler" / "handle_region_heartbeat_duration_seconds" | "Bucketed histogram of processing time (s) of handled region heartbeat requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "handle_region_heartbeat_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled region heartbeat requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 29), // 0.1ms ~ 7hours<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-181 [server/metrics.go:157](../../../server/metrics.go#L157) storeHeartbeatHandleDuration = | HistogramVec | "pd" / "scheduler" / "handle_store_heartbeat_duration_seconds" | "Bucketed histogram of processing time (s) of handled store heartbeat requests." | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "scheduler",<br>			Name:      "handle_store_heartbeat_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled store heartbeat requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0001, 2, 29), // 0.1ms ~ 7hours<br>		}, []string{"address", "store"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-182 [server/metrics.go:166](../../../server/metrics.go#L166) serviceAuditHistogram = | HistogramVec | "pd" / "service" / "audit_handling_seconds" | "PD server service handling performance metrics" | prometheus.NewHistogramVec(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "service",<br>			Name:      "audit_handling_seconds",<br>			Help:      "PD server service handling performance metrics",<br>			Buckets:   prometheus.DefBuckets,<br>		}, []string{"service", "method"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-183 [server/metrics.go:175](../../../server/metrics.go#L175) serviceAuditCounter = | CounterVec | "pd" / "service" / "audit_requests_total" | "Total number of service requests for audit" | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "service",<br>			Name:      "audit_requests_total",<br>			Help:      "Total number of service requests for audit",<br>		}, []string{"service", "method", "caller_component"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-184 [server/metrics.go:183](../../../server/metrics.go#L183) apiConcurrencyGauge = | GaugeVec | "pd" / "server" / "api_concurrency" | "Concurrency number of the api." | prometheus.NewGaugeVec(<br>		prometheus.GaugeOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "api_concurrency",<br>			Help:      "Concurrency number of the api.",<br>		}, []string{"kind", "api"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-185 [server/metrics.go:191](../../../server/metrics.go#L191) forwardFailCounter = | CounterVec | "pd" / "server" / "forward_fail_total" | "Counter of forward fail." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "forward_fail_total",<br>			Help:      "Counter of forward fail.",<br>		}, []string{"request", "type"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-186 [server/metrics.go:198](../../../server/metrics.go#L198) forwardTsoDuration = | Histogram | "pd" / "server" / "forward_tso_duration_seconds" | "Bucketed histogram of processing time (s) of handled forward tso requests." | prometheus.NewHistogram(<br>		prometheus.HistogramOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "forward_tso_duration_seconds",<br>			Help:      "Bucketed histogram of processing time (s) of handled forward tso requests.",<br>			Buckets:   prometheus.ExponentialBuckets(0.0005, 2, 13),<br>		}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |
| M-187 [server/metrics.go:207](../../../server/metrics.go#L207) regionRequestCounter = | CounterVec | "pd" / "server" / "region_request_cnt" | "Counter of region request." | prometheus.NewCounterVec(<br>		prometheus.CounterOpts{<br>			Namespace: "pd",<br>			Subsystem: "server",<br>			Name:      "region_request_cnt",<br>			Help:      "Counter of region request.",<br>		}, []string{"request", "caller_id", "caller_component", "event"}) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / 15/86/101；逐项未验收 |

## 注册、采集、认证、TLS、限流与审计入口

Go collector/runtime/cgroup 指标依赖 Go 运行时，owner主题9/86/102须明确接受或拒绝兼容；grpcprometheus、etcd内建指标属于外部依赖集合，不能从本地构造表推断完整 exposition。任务86/101必须用真实 scrape 固定实际激活集合及HELP/TYPE/labels，源码声明仅为下界。Go embedded-etcd监听/TLS/启动拓扑由主题10处理；外部etcd不能代替等价证据。认证以真实证书/allowed CN和各路由保护为准，不把服务label、audit或限流误作认证。

| 来源/符号 | 注册/安全/调用原文 | 当前Rust候选 | owner |
| --- | --- | --- | --- |
| [client/resource_group/controller/metrics/metrics.go:186](../../../client/resource_group/controller/metrics/metrics.go#L186) | FailedTokenRequestDuration = TokenRequestDuration.WithLabelValues("fail") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:187](../../../client/resource_group/controller/metrics/metrics.go#L187) | SuccessfulTokenRequestDuration = TokenRequestDuration.WithLabelValues("success") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:243](../../../client/resource_group/controller/metrics/metrics.go#L243) | prometheus.MustRegister(ResourceGroupStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:244](../../../client/resource_group/controller/metrics/metrics.go#L244) | prometheus.MustRegister(SuccessfulRequestDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:245](../../../client/resource_group/controller/metrics/metrics.go#L245) | prometheus.MustRegister(FailedRequestCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:246](../../../client/resource_group/controller/metrics/metrics.go#L246) | prometheus.MustRegister(FailedLimitReserveDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:247](../../../client/resource_group/controller/metrics/metrics.go#L247) | prometheus.MustRegister(GroupRunningKVRequestCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:248](../../../client/resource_group/controller/metrics/metrics.go#L248) | prometheus.MustRegister(RequestRetryCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:249](../../../client/resource_group/controller/metrics/metrics.go#L249) | prometheus.MustRegister(TokenRequestDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:250](../../../client/resource_group/controller/metrics/metrics.go#L250) | prometheus.MustRegister(ResourceGroupTokenRequestCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:251](../../../client/resource_group/controller/metrics/metrics.go#L251) | prometheus.MustRegister(LowTokenRequestNotifyCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:252](../../../client/resource_group/controller/metrics/metrics.go#L252) | prometheus.MustRegister(TokenConsumedHistogram) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:253](../../../client/resource_group/controller/metrics/metrics.go#L253) | prometheus.MustRegister(RequestSourceRUCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:254](../../../client/resource_group/controller/metrics/metrics.go#L254) | prometheus.MustRegister(CopReadPrechargeCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:255](../../../client/resource_group/controller/metrics/metrics.go#L255) | prometheus.MustRegister(CopReadNoPrechargeCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:256](../../../client/resource_group/controller/metrics/metrics.go#L256) | prometheus.MustRegister(PagingPrechargeBytesCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:257](../../../client/resource_group/controller/metrics/metrics.go#L257) | prometheus.MustRegister(PagingActualBytesCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go:258](../../../client/resource_group/controller/metrics/metrics.go#L258) | prometheus.MustRegister(PagingPredictionResidualBytes) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [cmd/pd-server/main.go:24](../../../cmd/pd-server/main.go#L24) | grpcprometheus "github.com/grpc-ecosystem/go-grpc-prometheus" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [cmd/pd-server/main.go:103](../../../cmd/pd-server/main.go#L103) | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [cmd/pd-server/main.go:124](../../../cmd/pd-server/main.go#L124) | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [cmd/pd-server/main.go:145](../../../cmd/pd-server/main.go#L145) | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [cmd/pd-server/main.go:166](../../../cmd/pd-server/main.go#L166) | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [cmd/pd-server/main.go:200](../../../cmd/pd-server/main.go#L200) | cmd.Flags().StringP("cacert", "", "", "path of file that contains list of trusted TLS CAs") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [cmd/pd-server/main.go:220](../../../cmd/pd-server/main.go#L220) | schedulers.Register() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [cmd/pd-server/main.go:269](../../../cmd/pd-server/main.go#L269) | grpcprometheus.EnableHandlingTimeHistogram() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/audit/audit.go:15](../../../pkg/audit/audit.go#L15) | package audit | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [pkg/audit/audit.go:107](../../../pkg/audit/audit.go#L107) | b.histogramVec.WithLabelValues(requestInfo.ServiceLabel, "HTTP").Observe(duration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题9；81/86/92/102接线 |
| [pkg/audit/audit.go:109](../../../pkg/audit/audit.go#L109) | b.counterVec.WithLabelValues(requestInfo.ServiceLabel, "HTTP", requestInfo.CallerID).Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题9；81/86/92/102接线 |
| [pkg/audit/audit.go:135](../../../pkg/audit/audit.go#L135) | log.Info("audit log", zap.String("service-info", requestInfo.String())) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [pkg/basicserver/metrics.go:52](../../../pkg/basicserver/metrics.go#L52) | prometheus.DefaultRegisterer.Unregister(collectors.NewGoCollector()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/basicserver/metrics.go:53](../../../pkg/basicserver/metrics.go#L53) | prometheus.MustRegister(collectors.NewGoCollector(collectors.WithGoCollectorRuntimeMetrics(collectors.MetricsGC, collectors.MetricsMemory, collectors.MetricsScheduler))) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/basicserver/metrics.go:55](../../../pkg/basicserver/metrics.go#L55) | prometheus.MustRegister(ServerMaxProcsGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/basicserver/metrics.go:56](../../../pkg/basicserver/metrics.go#L56) | prometheus.MustRegister(ServerMemoryLimit) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/basicserver/metrics.go:57](../../../pkg/basicserver/metrics.go#L57) | prometheus.MustRegister(ServerInfoGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:60](../../../pkg/core/metrics.go#L60) | waitRegionsLockDurationSum    = AcquireRegionsLockWaitDurationSum.WithLabelValues("WaitRegionsLock") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:61](../../../pkg/core/metrics.go#L61) | waitRegionsLockCount          = AcquireRegionsLockWaitCount.WithLabelValues("WaitRegionsLock") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:62](../../../pkg/core/metrics.go#L62) | waitSubRegionsLockDurationSum = AcquireRegionsLockWaitDurationSum.WithLabelValues("WaitSubRegionsLock") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:63](../../../pkg/core/metrics.go#L63) | waitSubRegionsLockCount       = AcquireRegionsLockWaitCount.WithLabelValues("WaitSubRegionsLock") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:66](../../../pkg/core/metrics.go#L66) | preCheckDurationSum       = HeartbeatBreakdownHandleDurationSum.WithLabelValues("PreCheck") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:67](../../../pkg/core/metrics.go#L67) | preCheckCount             = HeartbeatBreakdownHandleCount.WithLabelValues("PreCheck") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:68](../../../pkg/core/metrics.go#L68) | asyncHotStatsDurationSum  = HeartbeatBreakdownHandleDurationSum.WithLabelValues("AsyncHotStatsDuration") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:69](../../../pkg/core/metrics.go#L69) | asyncHotStatsCount        = HeartbeatBreakdownHandleCount.WithLabelValues("AsyncHotStatsDuration") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:70](../../../pkg/core/metrics.go#L70) | regionGuideDurationSum    = HeartbeatBreakdownHandleDurationSum.WithLabelValues("RegionGuide") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:71](../../../pkg/core/metrics.go#L71) | regionGuideCount          = HeartbeatBreakdownHandleCount.WithLabelValues("RegionGuide") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:72](../../../pkg/core/metrics.go#L72) | checkOverlapsDurationSum  = HeartbeatBreakdownHandleDurationSum.WithLabelValues("SaveCache_CheckOverlaps") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:73](../../../pkg/core/metrics.go#L73) | checkOverlapsCount        = HeartbeatBreakdownHandleCount.WithLabelValues("SaveCache_CheckOverlaps") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:74](../../../pkg/core/metrics.go#L74) | validateRegionDurationSum = HeartbeatBreakdownHandleDurationSum.WithLabelValues("SaveCache_InvalidRegion") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:75](../../../pkg/core/metrics.go#L75) | validateRegionCount       = HeartbeatBreakdownHandleCount.WithLabelValues("SaveCache_InvalidRegion") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:76](../../../pkg/core/metrics.go#L76) | setRegionDurationSum      = HeartbeatBreakdownHandleDurationSum.WithLabelValues("SaveCache_SetRegion") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:77](../../../pkg/core/metrics.go#L77) | setRegionCount            = HeartbeatBreakdownHandleCount.WithLabelValues("SaveCache_SetRegion") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:78](../../../pkg/core/metrics.go#L78) | updateSubTreeDurationSum  = HeartbeatBreakdownHandleDurationSum.WithLabelValues("SaveCache_UpdateSubTree") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:79](../../../pkg/core/metrics.go#L79) | updateSubTreeCount        = HeartbeatBreakdownHandleCount.WithLabelValues("SaveCache_UpdateSubTree") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:80](../../../pkg/core/metrics.go#L80) | regionCollectDurationSum  = HeartbeatBreakdownHandleDurationSum.WithLabelValues("CollectRegionStats") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:81](../../../pkg/core/metrics.go#L81) | regionCollectCount        = HeartbeatBreakdownHandleCount.WithLabelValues("CollectRegionStats") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:82](../../../pkg/core/metrics.go#L82) | otherDurationSum          = HeartbeatBreakdownHandleDurationSum.WithLabelValues("Other") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:83](../../../pkg/core/metrics.go#L83) | otherCount                = HeartbeatBreakdownHandleCount.WithLabelValues("Other") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:103](../../../pkg/core/metrics.go#L103) | queryRegionByKeysDuration     = queryRegionDuration.WithLabelValues("by-keys") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:104](../../../pkg/core/metrics.go#L104) | queryRegionByPrevKeysDuration = queryRegionDuration.WithLabelValues("by-prev-keys") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:105](../../../pkg/core/metrics.go#L105) | queryRegionByIDsDuration      = queryRegionDuration.WithLabelValues("by-ids") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:107](../../../pkg/core/metrics.go#L107) | queryRegionKeysCount     = queryRegionCount.WithLabelValues("keys") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:108](../../../pkg/core/metrics.go#L108) | queryRegionPrevKeysCount = queryRegionCount.WithLabelValues("prev-keys") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:109](../../../pkg/core/metrics.go#L109) | queryRegionIDsCount      = queryRegionCount.WithLabelValues("ids") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:119](../../../pkg/core/metrics.go#L119) | versionStaleCounter     = bucketEventCounter.WithLabelValues("version_stale") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:120](../../../pkg/core/metrics.go#L120) | versionNotChangeCounter = bucketEventCounter.WithLabelValues("version_no_change") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:122](../../../pkg/core/metrics.go#L122) | RegionCacheMissCounter = bucketEventCounter.WithLabelValues("region_cache_miss") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:124](../../../pkg/core/metrics.go#L124) | UpdateFailedCounter = bucketEventCounter.WithLabelValues("update_failed") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:126](../../../pkg/core/metrics.go#L126) | UpdateSuccessCounter = bucketEventCounter.WithLabelValues("update_success") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:130](../../../pkg/core/metrics.go#L130) | prometheus.MustRegister(HeartbeatBreakdownHandleDurationSum) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:131](../../../pkg/core/metrics.go#L131) | prometheus.MustRegister(HeartbeatBreakdownHandleCount) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:132](../../../pkg/core/metrics.go#L132) | prometheus.MustRegister(AcquireRegionsLockWaitDurationSum) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:133](../../../pkg/core/metrics.go#L133) | prometheus.MustRegister(AcquireRegionsLockWaitCount) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:134](../../../pkg/core/metrics.go#L134) | prometheus.MustRegister(queryRegionDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:135](../../../pkg/core/metrics.go#L135) | prometheus.MustRegister(queryRegionCount) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/core/metrics.go:136](../../../pkg/core/metrics.go#L136) | prometheus.MustRegister(bucketEventCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:106](../../../pkg/election/metrics.go#L106) | prometheus.MustRegister(keepAliveResponseInterval) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:107](../../../pkg/election/metrics.go#L107) | prometheus.MustRegister(renewalTerminationTotal) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:108](../../../pkg/election/metrics.go#L108) | prometheus.MustRegister(localTTLRemaining) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:109](../../../pkg/election/metrics.go#L109) | prometheus.MustRegister(keepAliveRequestDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:110](../../../pkg/election/metrics.go#L110) | prometheus.MustRegister(keepAliveTickInterval) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:126](../../../pkg/election/metrics.go#L126) | responseInterval:                keepAliveResponseInterval.WithLabelValues(purpose), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:127](../../../pkg/election/metrics.go#L127) | keepAliveRequestSuccessLatency:  keepAliveRequestDuration.WithLabelValues(purpose, metricsResultSuccess), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:128](../../../pkg/election/metrics.go#L128) | keepAliveRequestErrorLatency:    keepAliveRequestDuration.WithLabelValues(purpose, metricsResultError), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:129](../../../pkg/election/metrics.go#L129) | keepAliveRequestCanceledLatency: keepAliveRequestDuration.WithLabelValues(purpose, metricsResultCanceled), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:130](../../../pkg/election/metrics.go#L130) | tickInterval:                    keepAliveTickInterval.WithLabelValues(purpose), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:131](../../../pkg/election/metrics.go#L131) | invalidTTL:                      renewalTerminationTotal.WithLabelValues(purpose, reasonInvalidTTL), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:132](../../../pkg/election/metrics.go#L132) | leaseExpired:                    renewalTerminationTotal.WithLabelValues(purpose, reasonLeaseExpired), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:133](../../../pkg/election/metrics.go#L133) | contextCanceled:                 renewalTerminationTotal.WithLabelValues(purpose, reasonContextCanceled), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:232](../../../pkg/election/metrics.go#L232) | m.keepAliveRequestSuccessLatency.Observe(duration.Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:236](../../../pkg/election/metrics.go#L236) | m.keepAliveRequestCanceledLatency.Observe(duration.Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/election/metrics.go:239](../../../pkg/election/metrics.go#L239) | m.keepAliveRequestErrorLatency.Observe(duration.Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/gc/metrics.go:78](../../../pkg/gc/metrics.go#L78) | gcStateCacheAccessHitCounter     = gcStateCacheAccessCounter.WithLabelValues("hit") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/gc/metrics.go:79](../../../pkg/gc/metrics.go#L79) | gcStateCacheAccessSlowHitCounter = gcStateCacheAccessCounter.WithLabelValues("slow_hit") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/gc/metrics.go:80](../../../pkg/gc/metrics.go#L80) | gcStateCacheAccessMissCounter    = gcStateCacheAccessCounter.WithLabelValues("miss") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/gc/metrics.go:84](../../../pkg/gc/metrics.go#L84) | prometheus.MustRegister(productionBarrierMetrics) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/gc/metrics.go:85](../../../pkg/gc/metrics.go#L85) | prometheus.MustRegister(gcSafePointGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/gc/metrics.go:86](../../../pkg/gc/metrics.go#L86) | prometheus.MustRegister(gcStateCacheAccessCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/id/metrics.go:30](../../../pkg/id/metrics.go#L30) | prometheus.MustRegister(idGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/keyspace/metrics.go:63](../../../pkg/keyspace/metrics.go#L63) | prometheus.MustRegister(createKeyspaceStepDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:64](../../../pkg/keyspace/metrics.go#L64) | createKeyspaceStepDurationTotal = createKeyspaceStepDuration.WithLabelValues(StepTotal) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:65](../../../pkg/keyspace/metrics.go#L65) | createKeyspaceStepDurationAllocateID = createKeyspaceStepDuration.WithLabelValues(StepAllocateID) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:66](../../../pkg/keyspace/metrics.go#L66) | createKeyspaceStepDurationGetConfig = createKeyspaceStepDuration.WithLabelValues(StepGetConfig) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:67](../../../pkg/keyspace/metrics.go#L67) | createKeyspaceStepDurationSaveKeyspace = createKeyspaceStepDuration.WithLabelValues(StepSaveKeyspaceMeta) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:68](../../../pkg/keyspace/metrics.go#L68) | createKeyspaceStepDurationSplitRegion = createKeyspaceStepDuration.WithLabelValues(StepSplitRegion) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:69](../../../pkg/keyspace/metrics.go#L69) | createKeyspaceStepDurationEnableKeyspace = createKeyspaceStepDuration.WithLabelValues(StepEnableKeyspace) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:70](../../../pkg/keyspace/metrics.go#L70) | createKeyspaceStepDurationUpdateKG = createKeyspaceStepDuration.WithLabelValues(StepUpdateKeyspaceGroup) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:126](../../../pkg/keyspace/metrics.go#L126) | createKeyspaceStepDurationTotal.Observe(time.Since(t.beginTime).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/keyspace/metrics.go:133](../../../pkg/keyspace/metrics.go#L133) | observer.Observe(duration.Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/maintenance/metrics.go:30](../../../pkg/maintenance/metrics.go#L30) | prometheus.MustRegister(maintenanceTaskInfo) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/maintenance/metrics.go:35](../../../pkg/maintenance/metrics.go#L35) | maintenanceTaskInfo.WithLabelValues(taskType, taskID).Set(value) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/mcs/resourcemanager/server/metrics.go:250](../../../pkg/mcs/resourcemanager/server/metrics.go#L250) | prometheus.MustRegister(readRequestUnitCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:251](../../../pkg/mcs/resourcemanager/server/metrics.go#L251) | prometheus.MustRegister(writeRequestUnitCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:252](../../../pkg/mcs/resourcemanager/server/metrics.go#L252) | prometheus.MustRegister(activeRequestUnitCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:253](../../../pkg/mcs/resourcemanager/server/metrics.go#L253) | prometheus.MustRegister(requestUnitV2Cost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:254](../../../pkg/mcs/resourcemanager/server/metrics.go#L254) | prometheus.MustRegister(tikvRequestUnitV2Cost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:255](../../../pkg/mcs/resourcemanager/server/metrics.go#L255) | prometheus.MustRegister(tidbRequestUnitV2Cost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:256](../../../pkg/mcs/resourcemanager/server/metrics.go#L256) | prometheus.MustRegister(tiflashRequestUnitV2Cost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:257](../../../pkg/mcs/resourcemanager/server/metrics.go#L257) | prometheus.MustRegister(sqlLayerRequestUnitCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:258](../../../pkg/mcs/resourcemanager/server/metrics.go#L258) | prometheus.MustRegister(readByteCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:259](../../../pkg/mcs/resourcemanager/server/metrics.go#L259) | prometheus.MustRegister(writeByteCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:260](../../../pkg/mcs/resourcemanager/server/metrics.go#L260) | prometheus.MustRegister(crossAZTrafficCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:261](../../../pkg/mcs/resourcemanager/server/metrics.go#L261) | prometheus.MustRegister(kvCPUCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:262](../../../pkg/mcs/resourcemanager/server/metrics.go#L262) | prometheus.MustRegister(sqlCPUCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:263](../../../pkg/mcs/resourcemanager/server/metrics.go#L263) | prometheus.MustRegister(requestCount) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:264](../../../pkg/mcs/resourcemanager/server/metrics.go#L264) | prometheus.MustRegister(availableRUCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:265](../../../pkg/mcs/resourcemanager/server/metrics.go#L265) | prometheus.MustRegister(readRequestUnitMaxPerSecCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:266](../../../pkg/mcs/resourcemanager/server/metrics.go#L266) | prometheus.MustRegister(writeRequestUnitMaxPerSecCost) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:267](../../../pkg/mcs/resourcemanager/server/metrics.go#L267) | prometheus.MustRegister(resourceGroupConfigGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:268](../../../pkg/mcs/resourcemanager/server/metrics.go#L268) | prometheus.MustRegister(sampledRequestUnitPerSec) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:269](../../../pkg/mcs/resourcemanager/server/metrics.go#L269) | prometheus.MustRegister(overrideSettings) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:270](../../../pkg/mcs/resourcemanager/server/metrics.go#L270) | prometheus.MustRegister(serviceLimit) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:271](../../../pkg/mcs/resourcemanager/server/metrics.go#L271) | prometheus.MustRegister(pushRUMetricsDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:383](../../../pkg/mcs/resourcemanager/server/metrics.go#L383) | RRUMetrics:                 readRequestUnitCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:384](../../../pkg/mcs/resourcemanager/server/metrics.go#L384) | WRUMetrics:                 writeRequestUnitCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:385](../../../pkg/mcs/resourcemanager/server/metrics.go#L385) | ActiveRUMetrics:            activeRequestUnitCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:386](../../../pkg/mcs/resourcemanager/server/metrics.go#L386) | TotalRUV2Metrics:           requestUnitV2Cost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:387](../../../pkg/mcs/resourcemanager/server/metrics.go#L387) | TiKVRUV2Metrics:            tikvRequestUnitV2Cost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:388](../../../pkg/mcs/resourcemanager/server/metrics.go#L388) | TiDBRUV2Metrics:            tidbRequestUnitV2Cost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:389](../../../pkg/mcs/resourcemanager/server/metrics.go#L389) | TiFlashRUV2Metrics:         tiflashRequestUnitV2Cost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:390](../../../pkg/mcs/resourcemanager/server/metrics.go#L390) | SQLLayerRUMetrics:          sqlLayerRequestUnitCost.WithLabelValues(groupName, groupName, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:391](../../../pkg/mcs/resourcemanager/server/metrics.go#L391) | ReadByteMetrics:            readByteCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:392](../../../pkg/mcs/resourcemanager/server/metrics.go#L392) | WriteByteMetrics:           writeByteCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:393](../../../pkg/mcs/resourcemanager/server/metrics.go#L393) | ReadCrossAZTrafficMetrics:  crossAZTrafficCost.WithLabelValues(groupName, "read", keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:394](../../../pkg/mcs/resourcemanager/server/metrics.go#L394) | WriteCrossAZTrafficMetrics: crossAZTrafficCost.WithLabelValues(groupName, "write", keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:395](../../../pkg/mcs/resourcemanager/server/metrics.go#L395) | KvCPUMetrics:               kvCPUCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:396](../../../pkg/mcs/resourcemanager/server/metrics.go#L396) | SQLCPUMetrics:              sqlCPUCost.WithLabelValues(groupName, groupName, ruLabelType, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:397](../../../pkg/mcs/resourcemanager/server/metrics.go#L397) | ReadRequestCountMetrics:    requestCount.WithLabelValues(groupName, groupName, readTypeLabel, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:398](../../../pkg/mcs/resourcemanager/server/metrics.go#L398) | WriteRequestCountMetrics:   requestCount.WithLabelValues(groupName, groupName, writeTypeLabel, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:493](../../../pkg/mcs/resourcemanager/server/metrics.go#L493) | availableRUCounter:                 availableRUCounter.WithLabelValues(groupName, groupName, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:494](../../../pkg/mcs/resourcemanager/server/metrics.go#L494) | priorityResourceGroupConfigGauge:   resourceGroupConfigGauge.WithLabelValues(groupName, priorityLabel, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:495](../../../pkg/mcs/resourcemanager/server/metrics.go#L495) | ruPerSecResourceGroupConfigGauge:   resourceGroupConfigGauge.WithLabelValues(groupName, ruPerSecLabel, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:496](../../../pkg/mcs/resourcemanager/server/metrics.go#L496) | ruCapacityResourceGroupConfigGauge: resourceGroupConfigGauge.WithLabelValues(groupName, ruCapacityLabel, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:497](../../../pkg/mcs/resourcemanager/server/metrics.go#L497) | sampledRequestUnitPerSecGauge:      sampledRequestUnitPerSec.WithLabelValues(groupName, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:498](../../../pkg/mcs/resourcemanager/server/metrics.go#L498) | overrideFillRateGauge:              overrideSettings.WithLabelValues(groupName, keyspaceName, fillRateLabel), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:499](../../../pkg/mcs/resourcemanager/server/metrics.go#L499) | overrideBurstLimitGauge:            overrideSettings.WithLabelValues(groupName, keyspaceName, burstLimitLabel), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:534](../../../pkg/mcs/resourcemanager/server/metrics.go#L534) | serviceLimit.WithLabelValues(keyspaceName).Set(limit) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:584](../../../pkg/mcs/resourcemanager/server/metrics.go#L584) | rruMaxMetrics: readRequestUnitMaxPerSecCost.WithLabelValues(groupName, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go:585](../../../pkg/mcs/resourcemanager/server/metrics.go#L585) | wruMaxMetrics: writeRequestUnitMaxPerSecCost.WithLabelValues(groupName, keyspaceName), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/router/server/metrics.go:54](../../../pkg/mcs/router/server/metrics.go#L54) | prometheus.MustRegister(regionRequestCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/router/server/metrics.go:55](../../../pkg/mcs/router/server/metrics.go#L55) | prometheus.MustRegister(queryRegionDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/router/server/metrics.go:56](../../../pkg/mcs/router/server/metrics.go#L56) | prometheus.MustRegister(regionSyncerStatus) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:89](../../../pkg/mcs/scheduling/server/metrics.go#L89) | prometheus.MustRegister(storeHeartbeatHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:90](../../../pkg/mcs/scheduling/server/metrics.go#L90) | prometheus.MustRegister(storeHeartbeatCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:91](../../../pkg/mcs/scheduling/server/metrics.go#L91) | prometheus.MustRegister(regionHeartbeatHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:92](../../../pkg/mcs/scheduling/server/metrics.go#L92) | prometheus.MustRegister(regionHeartbeatCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:93](../../../pkg/mcs/scheduling/server/metrics.go#L93) | prometheus.MustRegister(regionBucketsHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:94](../../../pkg/mcs/scheduling/server/metrics.go#L94) | prometheus.MustRegister(regionBucketsCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go:95](../../../pkg/mcs/scheduling/server/metrics.go#L95) | prometheus.MustRegister(regionBucketsReportInterval) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/tso/server/metrics.go:49](../../../pkg/mcs/tso/server/metrics.go#L49) | prometheus.MustRegister(timeJumpBackCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/tso/server/metrics.go:50](../../../pkg/mcs/tso/server/metrics.go#L50) | prometheus.MustRegister(metaDataGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/mcs/tso/server/metrics.go:51](../../../pkg/mcs/tso/server/metrics.go#L51) | prometheus.MustRegister(tsoHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/member/metrics.go:39](../../../pkg/member/metrics.go#L39) | prometheus.MustRegister(ServiceMemberGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/member/metrics.go:40](../../../pkg/member/metrics.go#L40) | prometheus.MustRegister(memberLeaderPriorityGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/progress/metrics.go:46](../../../pkg/progress/metrics.go#L46) | prometheus.MustRegister(storesProgressGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/progress/metrics.go:47](../../../pkg/progress/metrics.go#L47) | prometheus.MustRegister(storesSpeedGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/progress/metrics.go:48](../../../pkg/progress/metrics.go#L48) | prometheus.MustRegister(storesETAGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/ratelimit/metrics.go:15](../../../pkg/ratelimit/metrics.go#L15) | package ratelimit | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [pkg/ratelimit/metrics.go:30](../../../pkg/ratelimit/metrics.go#L30) | Subsystem: "ratelimit", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [pkg/ratelimit/metrics.go:37](../../../pkg/ratelimit/metrics.go#L37) | Subsystem: "ratelimit", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [pkg/ratelimit/metrics.go:44](../../../pkg/ratelimit/metrics.go#L44) | Subsystem: "ratelimit", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [pkg/ratelimit/metrics.go:51](../../../pkg/ratelimit/metrics.go#L51) | Subsystem: "ratelimit", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [pkg/ratelimit/metrics.go:58](../../../pkg/ratelimit/metrics.go#L58) | Subsystem: "ratelimit", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [pkg/ratelimit/metrics.go:66](../../../pkg/ratelimit/metrics.go#L66) | prometheus.MustRegister(runnerTaskMaxWaitingDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/ratelimit/metrics.go:67](../../../pkg/ratelimit/metrics.go#L67) | prometheus.MustRegister(runnerPendingTasks) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/ratelimit/metrics.go:68](../../../pkg/ratelimit/metrics.go#L68) | prometheus.MustRegister(runnerFailedTasks) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/ratelimit/metrics.go:69](../../../pkg/ratelimit/metrics.go#L69) | prometheus.MustRegister(runnerTaskExecutionDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/ratelimit/metrics.go:70](../../../pkg/ratelimit/metrics.go#L70) | prometheus.MustRegister(runnerSucceededTasks) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/replication/metrics.go:62](../../../pkg/replication/metrics.go#L62) | prometheus.MustRegister(drTickCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/replication/metrics.go:63](../../../pkg/replication/metrics.go#L63) | prometheus.MustRegister(drStateGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/replication/metrics.go:64](../../../pkg/replication/metrics.go#L64) | prometheus.MustRegister(drStateIDGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/replication/metrics.go:65](../../../pkg/replication/metrics.go#L65) | prometheus.MustRegister(drRecoveredRegionGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/replication/metrics.go:66](../../../pkg/replication/metrics.go#L66) | prometheus.MustRegister(drRecoverProgressGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 主题8；78/102边界 |
| [pkg/schedule/affinity/metrics.go:31](../../../pkg/schedule/affinity/metrics.go#L31) | groupCount           = affinityStatusGauge.WithLabelValues("group-count") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/affinity/metrics.go:32](../../../pkg/schedule/affinity/metrics.go#L32) | regionCount          = affinityStatusGauge.WithLabelValues("region-count") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/affinity/metrics.go:33](../../../pkg/schedule/affinity/metrics.go#L33) | affinityRegionCount  = affinityStatusGauge.WithLabelValues("affinity-region-count") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/affinity/metrics.go:34](../../../pkg/schedule/affinity/metrics.go#L34) | regionSplitDenyCount = affinityStatusGauge.WithLabelValues("split-deny-count") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/affinity/metrics.go:38](../../../pkg/schedule/affinity/metrics.go#L38) | prometheus.MustRegister(affinityStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:93](../../../pkg/schedule/checker/metrics.go#L93) | splitScatterPendingExpiredAttemptedCounter   = splitScatterPendingExpiredCounter.WithLabelValues("true") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:94](../../../pkg/schedule/checker/metrics.go#L94) | splitScatterPendingExpiredUnattemptedCounter = splitScatterPendingExpiredCounter.WithLabelValues("false") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:98](../../../pkg/schedule/checker/metrics.go#L98) | prometheus.MustRegister(checkerCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:99](../../../pkg/schedule/checker/metrics.go#L99) | prometheus.MustRegister(regionListGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:100](../../../pkg/schedule/checker/metrics.go#L100) | prometheus.MustRegister(patrolCheckRegionsGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:101](../../../pkg/schedule/checker/metrics.go#L101) | prometheus.MustRegister(patrolPhaseDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:102](../../../pkg/schedule/checker/metrics.go#L102) | prometheus.MustRegister(checkRegionDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:103](../../../pkg/schedule/checker/metrics.go#L103) | prometheus.MustRegister(patrolRegionChannelSize) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:104](../../../pkg/schedule/checker/metrics.go#L104) | prometheus.MustRegister(splitScatterPendingExpiredCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:105](../../../pkg/schedule/checker/metrics.go#L105) | prometheus.MustRegister(splitScatterPendingDroppedCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:106](../../../pkg/schedule/checker/metrics.go#L106) | prometheus.MustRegister(splitScatterPendingGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:172](../../../pkg/schedule/checker/metrics.go#L172) | m.patrolPhaseHistograms[phase] = patrolPhaseDuration.WithLabelValues(phase) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:176](../../../pkg/schedule/checker/metrics.go#L176) | m.checkRegionHistograms[checker] = checkRegionDuration.WithLabelValues(checker) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:182](../../../pkg/schedule/checker/metrics.go#L182) | return checkerCounter.WithLabelValues(ruleChecker, event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:186](../../../pkg/schedule/checker/metrics.go#L186) | return checkerCounter.WithLabelValues(jointStateChecker, event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:190](../../../pkg/schedule/checker/metrics.go#L190) | return checkerCounter.WithLabelValues(mergeChecker, event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:194](../../../pkg/schedule/checker/metrics.go#L194) | return checkerCounter.WithLabelValues(replicaChecker, event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:198](../../../pkg/schedule/checker/metrics.go#L198) | return checkerCounter.WithLabelValues(splitScatterChecker, event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:241](../../../pkg/schedule/checker/metrics.go#L241) | learnerCheckerPausedCounter = checkerCounter.WithLabelValues(learnerChecker, "paused") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:281](../../../pkg/schedule/checker/metrics.go#L281) | splitCheckerCounter       = checkerCounter.WithLabelValues(splitChecker, "check") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:282](../../../pkg/schedule/checker/metrics.go#L282) | splitCheckerPausedCounter = checkerCounter.WithLabelValues(splitChecker, "paused") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:297](../../../pkg/schedule/checker/metrics.go#L297) | affinityCheckerCounter                        = checkerCounter.WithLabelValues(affinityChecker, "check") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:298](../../../pkg/schedule/checker/metrics.go#L298) | affinityCheckerPausedCounter                  = checkerCounter.WithLabelValues(affinityChecker, "paused") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:299](../../../pkg/schedule/checker/metrics.go#L299) | affinityCheckerPlacementRulesDisabledCounter  = checkerCounter.WithLabelValues(affinityChecker, "placement-rules-disabled") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:300](../../../pkg/schedule/checker/metrics.go#L300) | affinityCheckerRegionNoLeaderCounter          = checkerCounter.WithLabelValues(affinityChecker, "region-no-leader") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:301](../../../pkg/schedule/checker/metrics.go#L301) | affinityCheckerGroupSchedulingDisabledCounter = checkerCounter.WithLabelValues(affinityChecker, "group-scheduling-disabled") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:302](../../../pkg/schedule/checker/metrics.go#L302) | affinityCheckerNewOpCounter                   = checkerCounter.WithLabelValues(affinityChecker, "new-operator") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:303](../../../pkg/schedule/checker/metrics.go#L303) | affinityCheckerCreateOpFailedCounter          = checkerCounter.WithLabelValues(affinityChecker, "create-operator-failed") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:304](../../../pkg/schedule/checker/metrics.go#L304) | affinityCheckerUnhealthyRegionCounter         = checkerCounter.WithLabelValues(affinityChecker, "unhealthy-region") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:305](../../../pkg/schedule/checker/metrics.go#L305) | affinityCheckerAbnormalReplicaCounter         = checkerCounter.WithLabelValues(affinityChecker, "abnormal-replica") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:306](../../../pkg/schedule/checker/metrics.go#L306) | affinityCheckerLearnerPeerConflictCounter     = checkerCounter.WithLabelValues(affinityChecker, "learner-peer-conflict") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:309](../../../pkg/schedule/checker/metrics.go#L309) | affinityMergeCheckerCounter                     = checkerCounter.WithLabelValues(affinityChecker, "merge-check") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:310](../../../pkg/schedule/checker/metrics.go#L310) | affinityMergeCheckerDisabledCounter             = checkerCounter.WithLabelValues(affinityChecker, "merge-disabled") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:311](../../../pkg/schedule/checker/metrics.go#L311) | affinityMergeCheckerGlobalDisabledCounter       = checkerCounter.WithLabelValues(affinityChecker, "merge-global-disabled") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:312](../../../pkg/schedule/checker/metrics.go#L312) | affinityMergeCheckerSkipStartupCounter          = checkerCounter.WithLabelValues(affinityChecker, "merge-skip-startup") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:313](../../../pkg/schedule/checker/metrics.go#L313) | affinityMergeCheckerSkipCachedCounter           = checkerCounter.WithLabelValues(affinityChecker, "merge-skip-cached") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:314](../../../pkg/schedule/checker/metrics.go#L314) | affinityMergeCheckerNoNeedCounter               = checkerCounter.WithLabelValues(affinityChecker, "merge-no-need") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:315](../../../pkg/schedule/checker/metrics.go#L315) | affinityMergeCheckerNoTargetCounter             = checkerCounter.WithLabelValues(affinityChecker, "merge-no-target") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:316](../../../pkg/schedule/checker/metrics.go#L316) | affinityMergeCheckerTargetTooBigCounter         = checkerCounter.WithLabelValues(affinityChecker, "merge-target-too-big") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:317](../../../pkg/schedule/checker/metrics.go#L317) | affinityMergeCheckerNewOpCounter                = checkerCounter.WithLabelValues(affinityChecker, "merge-new-operator") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:318](../../../pkg/schedule/checker/metrics.go#L318) | affinityMergeCheckerAdjNotExistCounter          = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-not-exist") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:319](../../../pkg/schedule/checker/metrics.go#L319) | affinityMergeCheckerAdjDifferentGroupCounter    = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-different-group") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:320](../../../pkg/schedule/checker/metrics.go#L320) | affinityMergeCheckerAdjNotAffinityCounter       = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-not-affinity") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:321](../../../pkg/schedule/checker/metrics.go#L321) | affinityMergeCheckerAdjDisallowMergeCounter     = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-disallow-merge") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:322](../../../pkg/schedule/checker/metrics.go#L322) | affinityMergeCheckerAdjAbnormalPeerStoreCounter = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-abnormal-peerstore") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:323](../../../pkg/schedule/checker/metrics.go#L323) | affinityMergeCheckerAdjUnhealthyCounter         = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-unhealthy") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/checker/metrics.go:324](../../../pkg/schedule/checker/metrics.go#L324) | affinityMergeCheckerAdjAbnormalReplicaCounter   = checkerCounter.WithLabelValues(affinityChecker, "merge-adj-abnormal-replica") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/filter/metrics.go:37](../../../pkg/schedule/filter/metrics.go#L37) | prometheus.MustRegister(filterSourceCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/filter/metrics.go:38](../../../pkg/schedule/filter/metrics.go#L38) | prometheus.MustRegister(filterTargetCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/labeler/metrics.go:40](../../../pkg/schedule/labeler/metrics.go#L40) | prometheus.MustRegister(LabelerEventCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/labeler/metrics.go:41](../../../pkg/schedule/labeler/metrics.go#L41) | prometheus.MustRegister(newRegionLabelerDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/metrics.go:30](../../../pkg/schedule/metrics.go#L30) | prometheus.MustRegister(hotSpotStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:79](../../../pkg/schedule/operator/metrics.go#L79) | prometheus.MustRegister(operatorStepDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:80](../../../pkg/schedule/operator/metrics.go#L80) | prometheus.MustRegister(operatorLimitCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:81](../../../pkg/schedule/operator/metrics.go#L81) | prometheus.MustRegister(OperatorExceededStoreLimitCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:82](../../../pkg/schedule/operator/metrics.go#L82) | prometheus.MustRegister(operatorCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:83](../../../pkg/schedule/operator/metrics.go#L83) | prometheus.MustRegister(operatorDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:84](../../../pkg/schedule/operator/metrics.go#L84) | prometheus.MustRegister(operatorSizeHist) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/operator/metrics.go:89](../../../pkg/schedule/operator/metrics.go#L89) | operatorLimitCounter.WithLabelValues(typ.String(), kind.String()).Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/scatter/metrics.go:38](../../../pkg/schedule/scatter/metrics.go#L38) | prometheus.MustRegister(scatterCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/scatter/metrics.go:39](../../../pkg/schedule/scatter/metrics.go#L39) | prometheus.MustRegister(scatterDistributionCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:191](../../../pkg/schedule/schedulers/metrics.go#L191) | prometheus.MustRegister(schedulerStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:192](../../../pkg/schedule/schedulers/metrics.go#L192) | prometheus.MustRegister(ruleStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:193](../../../pkg/schedule/schedulers/metrics.go#L193) | prometheus.MustRegister(regionLabelStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:194](../../../pkg/schedule/schedulers/metrics.go#L194) | prometheus.MustRegister(schedulerCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:195](../../../pkg/schedule/schedulers/metrics.go#L195) | prometheus.MustRegister(balanceWitnessCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:196](../../../pkg/schedule/schedulers/metrics.go#L196) | prometheus.MustRegister(hotSchedulerResultCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:197](../../../pkg/schedule/schedulers/metrics.go#L197) | prometheus.MustRegister(hotDirectionCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:198](../../../pkg/schedule/schedulers/metrics.go#L198) | prometheus.MustRegister(balanceDirectionCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:199](../../../pkg/schedule/schedulers/metrics.go#L199) | prometheus.MustRegister(opInfluenceStatus) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:200](../../../pkg/schedule/schedulers/metrics.go#L200) | prometheus.MustRegister(tolerantResourceStatus) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:201](../../../pkg/schedule/schedulers/metrics.go#L201) | prometheus.MustRegister(hotPeerHist) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:202](../../../pkg/schedule/schedulers/metrics.go#L202) | prometheus.MustRegister(evictedSlowStoreStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:203](../../../pkg/schedule/schedulers/metrics.go#L203) | prometheus.MustRegister(evictedStoppingStoreStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:204](../../../pkg/schedule/schedulers/metrics.go#L204) | prometheus.MustRegister(slowStoreTriggerLimitGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:205](../../../pkg/schedule/schedulers/metrics.go#L205) | prometheus.MustRegister(storeSlowTrendEvictedStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:206](../../../pkg/schedule/schedulers/metrics.go#L206) | prometheus.MustRegister(storeSlowTrendActionStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:207](../../../pkg/schedule/schedulers/metrics.go#L207) | prometheus.MustRegister(storeSlowTrendMiscGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:208](../../../pkg/schedule/schedulers/metrics.go#L208) | prometheus.MustRegister(HotPendingSum) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:209](../../../pkg/schedule/schedulers/metrics.go#L209) | prometheus.MustRegister(balanceRangeGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:210](../../../pkg/schedule/schedulers/metrics.go#L210) | prometheus.MustRegister(balanceRangeJobGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:230](../../../pkg/schedule/schedulers/metrics.go#L230) | return schedulerCounter.WithLabelValues(types.BalanceLeaderScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:234](../../../pkg/schedule/schedulers/metrics.go#L234) | return schedulerCounter.WithLabelValues(types.BalanceRegionScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:238](../../../pkg/schedule/schedulers/metrics.go#L238) | return schedulerCounter.WithLabelValues(types.EvictLeaderScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:242](../../../pkg/schedule/schedulers/metrics.go#L242) | return schedulerCounter.WithLabelValues(types.GrantHotRegionScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:246](../../../pkg/schedule/schedulers/metrics.go#L246) | return schedulerCounter.WithLabelValues(types.GrantHotRegionScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:250](../../../pkg/schedule/schedulers/metrics.go#L250) | return schedulerCounter.WithLabelValues(types.BalanceHotRegionScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:254](../../../pkg/schedule/schedulers/metrics.go#L254) | return schedulerCounter.WithLabelValues(types.LabelScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:258](../../../pkg/schedule/schedulers/metrics.go#L258) | return schedulerCounter.WithLabelValues(types.RandomMergeScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:262](../../../pkg/schedule/schedulers/metrics.go#L262) | return schedulerCounter.WithLabelValues(types.ScatterRangeScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:266](../../../pkg/schedule/schedulers/metrics.go#L266) | return schedulerCounter.WithLabelValues(types.ShuffleHotRegionScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:270](../../../pkg/schedule/schedulers/metrics.go#L270) | return schedulerCounter.WithLabelValues(types.ShuffleLeaderScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:274](../../../pkg/schedule/schedulers/metrics.go#L274) | return schedulerCounter.WithLabelValues(types.ShuffleRegionScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:278](../../../pkg/schedule/schedulers/metrics.go#L278) | return schedulerCounter.WithLabelValues(types.TransferWitnessLeaderScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:282](../../../pkg/schedule/schedulers/metrics.go#L282) | return schedulerCounter.WithLabelValues(types.BalanceRangeScheduler.String(), event) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:311](../../../pkg/schedule/schedulers/metrics.go#L311) | evictSlowStoreCounter     = schedulerCounter.WithLabelValues(types.EvictSlowStoreScheduler.String(), "schedule") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go:312](../../../pkg/schedule/schedulers/metrics.go#L312) | evictStoppingStoreCounter = schedulerCounter.WithLabelValues(types.EvictStoppingStoreScheduler.String(), "schedule") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:111](../../../pkg/statistics/metrics.go#L111) | regionMissVoterPeerDuration = regionAbnormalPeerDuration.WithLabelValues("miss-voter-peer") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:112](../../../pkg/statistics/metrics.go#L112) | regionDownPeerDuration      = regionAbnormalPeerDuration.WithLabelValues("down-peer") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:116](../../../pkg/statistics/metrics.go#L116) | prometheus.MustRegister(hotCacheStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:117](../../../pkg/statistics/metrics.go#L117) | prometheus.MustRegister(storeStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:118](../../../pkg/statistics/metrics.go#L118) | prometheus.MustRegister(regionStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:119](../../../pkg/statistics/metrics.go#L119) | prometheus.MustRegister(clusterStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:120](../../../pkg/statistics/metrics.go#L120) | prometheus.MustRegister(placementStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:121](../../../pkg/statistics/metrics.go#L121) | prometheus.MustRegister(configStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:122](../../../pkg/statistics/metrics.go#L122) | prometheus.MustRegister(StoreLimitGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:123](../../../pkg/statistics/metrics.go#L123) | prometheus.MustRegister(regionLabelLevelGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:124](../../../pkg/statistics/metrics.go#L124) | prometheus.MustRegister(regionAbnormalPeerDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/statistics/metrics.go:125](../../../pkg/statistics/metrics.go#L125) | prometheus.MustRegister(hotPeerSummary) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/storage/kv/metrics.go:40](../../../pkg/storage/kv/metrics.go#L40) | prometheus.MustRegister(txnCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/storage/kv/metrics.go:41](../../../pkg/storage/kv/metrics.go#L41) | prometheus.MustRegister(txnDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/syncer/metrics.go:28](../../../pkg/syncer/metrics.go#L28) | prometheus.MustRegister(regionSyncerStatus) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:106](../../../pkg/tso/metrics.go#L106) | prometheus.MustRegister(tsoCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:107](../../../pkg/tso/metrics.go#L107) | prometheus.MustRegister(tsoGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:108](../../../pkg/tso/metrics.go#L108) | prometheus.MustRegister(tsoGap) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:109](../../../pkg/tso/metrics.go#L109) | prometheus.MustRegister(tsoOpDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:110](../../../pkg/tso/metrics.go#L110) | prometheus.MustRegister(tsoAllocatorRole) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:111](../../../pkg/tso/metrics.go#L111) | prometheus.MustRegister(keyspaceGroupStateGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:112](../../../pkg/tso/metrics.go#L112) | prometheus.MustRegister(keyspaceGroupOpDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:113](../../../pkg/tso/metrics.go#L113) | prometheus.MustRegister(keyspaceGroupKeyspaceCountGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:155](../../../pkg/tso/metrics.go#L155) | syncEvent:                    tsoCounter.WithLabelValues("sync", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:156](../../../pkg/tso/metrics.go#L156) | skipSyncEvent:                tsoCounter.WithLabelValues("skip_sync", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:157](../../../pkg/tso/metrics.go#L157) | syncOKEvent:                  tsoCounter.WithLabelValues("sync_ok", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:158](../../../pkg/tso/metrics.go#L158) | errSaveSyncTSEvent:           tsoCounter.WithLabelValues("err_save_sync_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:159](../../../pkg/tso/metrics.go#L159) | errLeaseResetTSEvent:         tsoCounter.WithLabelValues("err_lease_reset_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:160](../../../pkg/tso/metrics.go#L160) | errResetSmallPhysicalTSEvent: tsoCounter.WithLabelValues("err_reset_physical_small_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:161](../../../pkg/tso/metrics.go#L161) | errResetSmallLogicalTSEvent:  tsoCounter.WithLabelValues("err_reset_logical_small_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:162](../../../pkg/tso/metrics.go#L162) | errResetLargeTSEvent:         tsoCounter.WithLabelValues("err_reset_large_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:163](../../../pkg/tso/metrics.go#L163) | errSaveResetTSEvent:          tsoCounter.WithLabelValues("err_save_reset_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:164](../../../pkg/tso/metrics.go#L164) | resetTSOOKEvent:              tsoCounter.WithLabelValues("reset_tso_ok", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:165](../../../pkg/tso/metrics.go#L165) | saveEvent:                    tsoCounter.WithLabelValues("save", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:166](../../../pkg/tso/metrics.go#L166) | slowSaveEvent:                tsoCounter.WithLabelValues("slow_save", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:167](../../../pkg/tso/metrics.go#L167) | systemTimeSlowEvent:          tsoCounter.WithLabelValues("system_time_slow", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:168](../../../pkg/tso/metrics.go#L168) | skipSaveEvent:                tsoCounter.WithLabelValues("skip_save", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:169](../../../pkg/tso/metrics.go#L169) | errSaveUpdateTSEvent:         tsoCounter.WithLabelValues("err_save_update_ts", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:170](../../../pkg/tso/metrics.go#L170) | notLeaderAnymoreEvent:        tsoCounter.WithLabelValues("not_leader_anymore", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:171](../../../pkg/tso/metrics.go#L171) | logicalOverflowEvent:         tsoCounter.WithLabelValues("logical_overflow", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:172](../../../pkg/tso/metrics.go#L172) | exceededMaxRetryEvent:        tsoCounter.WithLabelValues("exceeded_max_retry", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:173](../../../pkg/tso/metrics.go#L173) | notAllowedSaveTimestampEvent: tsoCounter.WithLabelValues("not_allowed_save_timestamp", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:174](../../../pkg/tso/metrics.go#L174) | syncSaveDuration:             tsoOpDuration.WithLabelValues("sync_save", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:175](../../../pkg/tso/metrics.go#L175) | resetSaveDuration:            tsoOpDuration.WithLabelValues("reset_save", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:176](../../../pkg/tso/metrics.go#L176) | updateSaveDuration:           tsoOpDuration.WithLabelValues("update_save", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:177](../../../pkg/tso/metrics.go#L177) | notLeaderEvent:               tsoCounter.WithLabelValues("not_leader", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:178](../../../pkg/tso/metrics.go#L178) | globalTSOSyncEvent:           tsoCounter.WithLabelValues("global_tso_sync", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:179](../../../pkg/tso/metrics.go#L179) | globalTSOEstimateEvent:       tsoCounter.WithLabelValues("global_tso_estimate", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:180](../../../pkg/tso/metrics.go#L180) | globalTSOPersistEvent:        tsoCounter.WithLabelValues("global_tso_persist", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:181](../../../pkg/tso/metrics.go#L181) | errGlobalTSOPersistEvent:     tsoCounter.WithLabelValues("global_tso_persist_err", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:182](../../../pkg/tso/metrics.go#L182) | precheckLogicalOverflowEvent: tsoCounter.WithLabelValues("precheck_logical_overflow", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:183](../../../pkg/tso/metrics.go#L183) | tsoPhysicalGauge:             tsoGauge.WithLabelValues("tso", groupID), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:184](../../../pkg/tso/metrics.go#L184) | tsoPhysicalGapGauge:          tsoGap.WithLabelValues(groupLabel), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:203](../../../pkg/tso/metrics.go#L203) | splitSourceGauge:        keyspaceGroupStateGauge.WithLabelValues("split-source"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:204](../../../pkg/tso/metrics.go#L204) | splitTargetGauge:        keyspaceGroupStateGauge.WithLabelValues("split-target"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:205](../../../pkg/tso/metrics.go#L205) | mergeSourceGauge:        keyspaceGroupStateGauge.WithLabelValues("merge-source"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:206](../../../pkg/tso/metrics.go#L206) | mergeTargetGauge:        keyspaceGroupStateGauge.WithLabelValues("merge-target"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:207](../../../pkg/tso/metrics.go#L207) | splitDuration:           keyspaceGroupOpDuration.WithLabelValues("split"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:208](../../../pkg/tso/metrics.go#L208) | mergeDuration:           keyspaceGroupOpDuration.WithLabelValues("merge"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:209](../../../pkg/tso/metrics.go#L209) | finishSplitSendDuration: keyspaceGroupOpDuration.WithLabelValues("finish-split-send"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:210](../../../pkg/tso/metrics.go#L210) | finishSplitDuration:     keyspaceGroupOpDuration.WithLabelValues("finish-split"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:211](../../../pkg/tso/metrics.go#L211) | finishMergeSendDuration: keyspaceGroupOpDuration.WithLabelValues("finish-merge-send"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:212](../../../pkg/tso/metrics.go#L212) | finishMergeDuration:     keyspaceGroupOpDuration.WithLabelValues("finish-merge"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/tso/metrics.go:223](../../../pkg/tso/metrics.go#L223) | gauge := keyspaceGroupKeyspaceCountGauge.WithLabelValues(labelVal) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/utils/etcdutil/metrics.go:45](../../../pkg/utils/etcdutil/metrics.go#L45) | prometheus.MustRegister(etcdStateGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/utils/etcdutil/metrics.go:46](../../../pkg/utils/etcdutil/metrics.go#L46) | prometheus.MustRegister(etcdEndpointLatency) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/utils/grpcutil/grpcutil.go:50](../../../pkg/utils/grpcutil/grpcutil.go#L50) | type TLSConfig struct { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:58](../../../pkg/utils/grpcutil/grpcutil.go#L58) | CertAllowedCNs []string &#96;toml:"cert-allowed-cn" json:"cert-allowed-cn"&#96; | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:66](../../../pkg/utils/grpcutil/grpcutil.go#L66) | func (s TLSConfig) ToTLSInfo() (*transport.TLSInfo, error) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:71](../../../pkg/utils/grpcutil/grpcutil.go#L71) | return &transport.TLSInfo{ | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:75](../../../pkg/utils/grpcutil/grpcutil.go#L75) | AllowedCNs:    s.CertAllowedCNs, | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:80](../../../pkg/utils/grpcutil/grpcutil.go#L80) | func (s TLSConfig) ToClientTLSConfig() (*tls.Config, error) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:100](../../../pkg/utils/grpcutil/grpcutil.go#L100) | tlsInfo, err := s.ToTLSInfo() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:105](../../../pkg/utils/grpcutil/grpcutil.go#L105) | return nil, errs.ErrTLSConfig.Wrap(err).GenWithStackByCause() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:110](../../../pkg/utils/grpcutil/grpcutil.go#L110) | return nil, errs.ErrTLSConfig.Wrap(err).GenWithStackByCause() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:116](../../../pkg/utils/grpcutil/grpcutil.go#L116) | func (s TLSConfig) ToServerTLSConfig() (*tls.Config, error) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:121](../../../pkg/utils/grpcutil/grpcutil.go#L121) | tlsInfo := transport.TLSInfo{ | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:125](../../../pkg/utils/grpcutil/grpcutil.go#L125) | AllowedCNs:    s.CertAllowedCNs, | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:130](../../../pkg/utils/grpcutil/grpcutil.go#L130) | return nil, errs.ErrTLSConfig.Wrap(err).GenWithStackByCause() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:133](../../../pkg/utils/grpcutil/grpcutil.go#L133) | tlsConfig.ClientAuth = tls.VerifyClientCertIfGiven | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:154](../../../pkg/utils/grpcutil/grpcutil.go#L154) | creds := credentials.NewTLS(tlsCfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:218](../../../pkg/utils/grpcutil/grpcutil.go#L218) | func establish(ctx context.Context, addr string, tlsConfig *TLSConfig, do ...grpc.DialOption) (*grpc.ClientConn, error) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:219](../../../pkg/utils/grpcutil/grpcutil.go#L219) | tlsCfg, err := tlsConfig.ToClientTLSConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/grpcutil.go:236](../../../pkg/utils/grpcutil/grpcutil.go#L236) | func CreateClientConn(ctx context.Context, addr string, tlsConfig *TLSConfig, do ...grpc.DialOption) *grpc.ClientConn { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/grpcutil/metrics.go:60](../../../pkg/utils/grpcutil/metrics.go#L60) | counter.WithLabelValues(method, callerID, callerComponent, string(event)).Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [pkg/utils/metricutil/metricutil.go:125](../../../pkg/utils/metricutil/metricutil.go#L125) | AuthToken:         os.Getenv("PYROSCOPE_AUTH_TOKEN"), //nolint:staticcheck // Preserve token authentication for existing deployments. | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/metricutil/metricutil.go:127](../../../pkg/utils/metricutil/metricutil.go#L127) | BasicAuthUser:     os.Getenv("PYROSCOPE_BASIC_AUTH_USER"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [pkg/utils/metricutil/metricutil.go:128](../../../pkg/utils/metricutil/metricutil.go#L128) | BasicAuthPassword: os.Getenv("PYROSCOPE_BASIC_AUTH_PASSWORD"), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/api/middleware.go:28](../../../server/api/middleware.go#L28) | "github.com/tikv/pd/pkg/audit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:45](../../../server/api/middleware.go#L45) | handlers: []negroni.Handler{newRequestInfoMiddleware(s), newAuditMiddleware(s), newRateLimitMiddleware(s)}, | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:63](../../../server/api/middleware.go#L63) | if !rm.svr.GetServiceMiddlewarePersistOptions().IsAuditEnabled() && | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:64](../../../server/api/middleware.go#L64) | !rm.svr.GetServiceMiddlewarePersistOptions().IsRateLimitEnabled() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:84](../../../server/api/middleware.go#L84) | func captureRequestBodyForAudit( | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:87](../../../server/api/middleware.go#L87) | labels *audit.BackendLabels, | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:89](../../../server/api/middleware.go#L89) | if requestInfo.BodyParam != "" \|\| !auditNeedsRequestBody(labels) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:96](../../../server/api/middleware.go#L96) | func auditNeedsRequestBody(labels *audit.BackendLabels) bool { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:97](../../../server/api/middleware.go#L97) | return labels != nil && slices.Contains(labels.Labels, audit.LocalLogLabel) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:100](../../../server/api/middleware.go#L100) | func prepareRequestForAudit( | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:104](../../../server/api/middleware.go#L104) | labels *audit.BackendLabels, | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:107](../../../server/api/middleware.go#L107) | if captureRequestBodyForAudit(r, &requestInfo, labels) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:205](../../../server/api/middleware.go#L205) | type auditMiddleware struct { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:209](../../../server/api/middleware.go#L209) | func newAuditMiddleware(s *server.Server) negroni.Handler { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:210](../../../server/api/middleware.go#L210) | return &auditMiddleware{svr: s} | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:214](../../../server/api/middleware.go#L214) | func (s *auditMiddleware) ServeHTTP(w http.ResponseWriter, r *http.Request, next http.HandlerFunc) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:215](../../../server/api/middleware.go#L215) | if !s.svr.GetServiceMiddlewarePersistOptions().IsAuditEnabled() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:224](../../../server/api/middleware.go#L224) | labels := s.svr.GetServiceAuditBackendLabels(requestInfo.ServiceLabel) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:229](../../../server/api/middleware.go#L229) | r = prepareRequestForAudit(r, requestInfo, ok, labels) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:231](../../../server/api/middleware.go#L231) | beforeNextBackends := make([]audit.Backend, 0) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:232](../../../server/api/middleware.go#L232) | afterNextBackends := make([]audit.Backend, 0) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:233](../../../server/api/middleware.go#L233) | for _, backend := range s.svr.GetAuditBackend() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/middleware.go:255](../../../server/api/middleware.go#L255) | type rateLimitMiddleware struct { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:259](../../../server/api/middleware.go#L259) | func newRateLimitMiddleware(s *server.Server) negroni.Handler { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:260](../../../server/api/middleware.go#L260) | return &rateLimitMiddleware{svr: s} | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:264](../../../server/api/middleware.go#L264) | func (s *rateLimitMiddleware) ServeHTTP(w http.ResponseWriter, r *http.Request, next http.HandlerFunc) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:265](../../../server/api/middleware.go#L265) | if !s.svr.GetServiceMiddlewarePersistOptions().IsRateLimitEnabled() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:275](../../../server/api/middleware.go#L275) | rateLimiter := s.svr.GetServiceRateLimiter() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/middleware.go:276](../../../server/api/middleware.go#L276) | if done, err := rateLimiter.Allow(requestInfo.ServiceLabel); err == nil { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/router.go:29](../../../server/api/router.go#L29) | "github.com/tikv/pd/pkg/audit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:30](../../../server/api/router.go#L30) | "github.com/tikv/pd/pkg/ratelimit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/router.go:105](../../../server/api/router.go#L105) | setAuditBackend := func(labels ...string) createRouteOption { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:108](../../../server/api/router.go#L108) | svr.SetServiceAuditBackendLabels(route.GetName(), labels) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:114](../../../server/api/router.go#L114) | localLog := audit.LocalLogLabel | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:116](../../../server/api/router.go#L116) | prometheus := audit.PrometheusHistogram | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:118](../../../server/api/router.go#L118) | setRateLimitAllowList := func() createRouteOption { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/router.go:120](../../../server/api/router.go#L120) | svr.UpdateServiceRateLimiter(route.GetName(), ratelimit.AddLabelAllowList()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/router.go:142](../../../server/api/router.go#L142) | registerFunc(apiRouter, "/operators", operatorHandler.GetOperators, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:143](../../../server/api/router.go#L143) | registerFunc(apiRouter, "/operators", operatorHandler.CreateOperator, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:144](../../../server/api/router.go#L144) | registerFunc(apiRouter, "/operators", operatorHandler.DeleteOperators, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:145](../../../server/api/router.go#L145) | registerFunc(apiRouter, "/operators/records", operatorHandler.GetOperatorRecords, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:146](../../../server/api/router.go#L146) | registerFunc(apiRouter, "/operators/{region_id}", operatorHandler.GetOperatorsByRegion, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:147](../../../server/api/router.go#L147) | registerFunc(apiRouter, "/operators/{region_id}", operatorHandler.DeleteOperatorByRegion, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:150](../../../server/api/router.go#L150) | registerFunc(apiRouter, "/checker/{name}", checkerHandler.PauseOrResumeChecker, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:151](../../../server/api/router.go#L151) | registerFunc(apiRouter, "/checker/{name}", checkerHandler.GetCheckerStatus, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:154](../../../server/api/router.go#L154) | registerFunc(apiRouter, "/schedulers", schedulerHandler.GetSchedulers, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:155](../../../server/api/router.go#L155) | registerFunc(apiRouter, "/schedulers", schedulerHandler.CreateScheduler, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:156](../../../server/api/router.go#L156) | registerFunc(apiRouter, "/schedulers/{name}", schedulerHandler.DeleteScheduler, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:157](../../../server/api/router.go#L157) | registerFunc(apiRouter, "/schedulers/{name}", schedulerHandler.PauseOrResumeScheduler, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:160](../../../server/api/router.go#L160) | registerFunc(clusterRouter, "/schedulers/diagnostic/{name}", diagnosticHandler.getDiagnosticResult, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:163](../../../server/api/router.go#L163) | registerPrefix(apiRouter, "/scheduler-config", "HandleSchedulerConfig", schedulerConfigHandler.handleSchedulerConfig, setMethods(http.MethodPost, http.MethodDelete, http.MethodPut, http.MethodPatch), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:164](../../../server/api/router.go#L164) | registerPrefix(apiRouter, "/scheduler-config", "GetSchedulerConfig", schedulerConfigHandler.handleSchedulerConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:167](../../../server/api/router.go#L167) | registerFunc(apiRouter, "/cluster", clusterHandler.GetCluster, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:168](../../../server/api/router.go#L168) | registerFunc(apiRouter, "/cluster/status", clusterHandler.GetClusterStatus, setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:171](../../../server/api/router.go#L171) | registerFunc(apiRouter, "/config", confHandler.GetConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:172](../../../server/api/router.go#L172) | registerFunc(apiRouter, "/config", confHandler.SetConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:173](../../../server/api/router.go#L173) | registerFunc(apiRouter, "/config/default", confHandler.GetDefaultConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:174](../../../server/api/router.go#L174) | registerFunc(apiRouter, "/config/schedule", confHandler.GetScheduleConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:175](../../../server/api/router.go#L175) | registerFunc(apiRouter, "/config/schedule", confHandler.SetScheduleConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:176](../../../server/api/router.go#L176) | registerFunc(apiRouter, "/config/pd-server", confHandler.GetPDServerConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:177](../../../server/api/router.go#L177) | registerFunc(apiRouter, "/config/replicate", confHandler.GetReplicationConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:178](../../../server/api/router.go#L178) | registerFunc(apiRouter, "/config/replicate", confHandler.SetReplicationConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:179](../../../server/api/router.go#L179) | registerFunc(apiRouter, "/config/label-property", confHandler.GetLabelPropertyConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:180](../../../server/api/router.go#L180) | registerFunc(apiRouter, "/config/label-property", confHandler.SetLabelPropertyConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:181](../../../server/api/router.go#L181) | registerFunc(apiRouter, "/config/cluster-version", confHandler.GetClusterVersion, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:182](../../../server/api/router.go#L182) | registerFunc(apiRouter, "/config/cluster-version", confHandler.SetClusterVersion, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:183](../../../server/api/router.go#L183) | registerFunc(apiRouter, "/config/replication-mode", confHandler.GetReplicationModeConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:184](../../../server/api/router.go#L184) | registerFunc(apiRouter, "/config/replication-mode", confHandler.SetReplicationModeConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:189](../../../server/api/router.go#L189) | registerFunc(ruleRouter, "/config/rules", rulesHandler.GetAllRules, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:190](../../../server/api/router.go#L190) | registerFunc(ruleRouter, "/config/rules", rulesHandler.SetAllRules, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:191](../../../server/api/router.go#L191) | registerFunc(ruleRouter, "/config/rules/batch", rulesHandler.BatchRules, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:192](../../../server/api/router.go#L192) | registerFunc(ruleRouter, "/config/rules/group/{group}", rulesHandler.GetRuleByGroup, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:193](../../../server/api/router.go#L193) | registerFunc(ruleRouter, "/config/rules/region/{region}", rulesHandler.GetRulesByRegion, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:194](../../../server/api/router.go#L194) | registerFunc(ruleRouter, "/config/rules/region/{region}/detail", rulesHandler.CheckRegionPlacementRule, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:195](../../../server/api/router.go#L195) | registerFunc(ruleRouter, "/config/rules/key/{key}", rulesHandler.GetRulesByKey, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:196](../../../server/api/router.go#L196) | registerFunc(ruleRouter, "/config/rule/{group}/{id}", rulesHandler.GetRuleByGroupAndID, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:197](../../../server/api/router.go#L197) | registerFunc(ruleRouter, "/config/rule", rulesHandler.SetRule, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:198](../../../server/api/router.go#L198) | registerFunc(ruleRouter, "/config/rule/{group}/{id}", rulesHandler.DeleteRuleByGroup, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:200](../../../server/api/router.go#L200) | registerFunc(ruleRouter, "/config/rule_group/{id}", rulesHandler.GetGroupConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:201](../../../server/api/router.go#L201) | registerFunc(ruleRouter, "/config/rule_group", rulesHandler.SetGroupConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:202](../../../server/api/router.go#L202) | registerFunc(ruleRouter, "/config/rule_group/{id}", rulesHandler.DeleteGroupConfig, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:203](../../../server/api/router.go#L203) | registerFunc(ruleRouter, "/config/rule_groups", rulesHandler.GetAllGroupConfigs, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:205](../../../server/api/router.go#L205) | registerFunc(ruleRouter, "/config/placement-rule", rulesHandler.GetPlacementRules, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:206](../../../server/api/router.go#L206) | registerFunc(ruleRouter, "/config/placement-rule", rulesHandler.SetPlacementRules, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:209](../../../server/api/router.go#L209) | registerFunc(ruleRouter, "/config/placement-rule/{group}", rulesHandler.GetPlacementRuleByGroup, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:210](../../../server/api/router.go#L210) | registerFunc(ruleRouter, "/config/placement-rule/{group}", rulesHandler.SetPlacementRuleByGroup, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:211](../../../server/api/router.go#L211) | registerFunc(ruleRouter, "/config/placement-rule/{group}", rulesHandler.DeletePlacementRuleByGroup, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:214](../../../server/api/router.go#L214) | registerFunc(clusterRouter, "/config/region-label/rules", regionLabelHandler.GetAllRegionLabelRules, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:215](../../../server/api/router.go#L215) | registerFunc(clusterRouter, "/config/region-label/rules/ids", regionLabelHandler.GetRegionLabelRulesByIDs, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:217](../../../server/api/router.go#L217) | registerFunc(escapeRouter, "/config/region-label/rule/{id}", regionLabelHandler.GetRegionLabelRuleByID, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:218](../../../server/api/router.go#L218) | registerFunc(escapeRouter, "/config/region-label/rule/{id}", regionLabelHandler.DeleteRegionLabelRule, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:219](../../../server/api/router.go#L219) | registerFunc(clusterRouter, "/config/region-label/rule", regionLabelHandler.SetRegionLabelRule, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:220](../../../server/api/router.go#L220) | registerFunc(clusterRouter, "/config/region-label/rules", regionLabelHandler.PatchRegionLabelRules, setMethods(http.MethodPatch), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:221](../../../server/api/router.go#L221) | registerFunc(clusterRouter, "/region/id/{id}/label/{key}", regionLabelHandler.GetRegionLabelByKey, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:222](../../../server/api/router.go#L222) | registerFunc(clusterRouter, "/region/id/{id}/labels", regionLabelHandler.GetRegionLabels, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:225](../../../server/api/router.go#L225) | registerFunc(clusterRouter, "/store/{id}", storeHandler.GetStore, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:226](../../../server/api/router.go#L226) | registerFunc(clusterRouter, "/store/{id}", storeHandler.DeleteStore, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:227](../../../server/api/router.go#L227) | registerFunc(clusterRouter, "/store/{id}/state", storeHandler.SetStoreState, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:228](../../../server/api/router.go#L228) | registerFunc(clusterRouter, "/store/{id}/label", storeHandler.SetStoreLabel, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:229](../../../server/api/router.go#L229) | registerFunc(clusterRouter, "/store/{id}/label", storeHandler.DeleteStoreLabel, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:230](../../../server/api/router.go#L230) | registerFunc(clusterRouter, "/store/{id}/weight", storeHandler.SetStoreWeight, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:231](../../../server/api/router.go#L231) | registerFunc(clusterRouter, "/store/{id}/limit", storeHandler.SetStoreLimit, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:234](../../../server/api/router.go#L234) | registerFunc(clusterRouter, "/stores", storesHandler.GetAllStores, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:235](../../../server/api/router.go#L235) | registerFunc(clusterRouter, "/stores/remove-tombstone", storesHandler.RemoveTombStone, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:236](../../../server/api/router.go#L236) | registerFunc(clusterRouter, "/stores/limit", storesHandler.GetAllStoresLimit, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:237](../../../server/api/router.go#L237) | registerFunc(clusterRouter, "/stores/limit", storesHandler.SetAllStoresLimit, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:238](../../../server/api/router.go#L238) | registerFunc(clusterRouter, "/stores/progress", storesHandler.GetStoresProgress, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:239](../../../server/api/router.go#L239) | registerFunc(clusterRouter, "/stores/check", storesHandler.GetStoresByState, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:242](../../../server/api/router.go#L242) | registerFunc(clusterRouter, "/labels", labelsHandler.GetLabels, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:243](../../../server/api/router.go#L243) | registerFunc(clusterRouter, "/labels/stores", labelsHandler.GetStoresByLabel, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:246](../../../server/api/router.go#L246) | registerFunc(apiRouter, "/hotspot/regions/write", hotStatusHandler.GetHotWriteRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:247](../../../server/api/router.go#L247) | registerFunc(apiRouter, "/hotspot/regions/read", hotStatusHandler.GetHotReadRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:248](../../../server/api/router.go#L248) | registerFunc(apiRouter, "/hotspot/regions/history", hotStatusHandler.GetHistoryHotRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:249](../../../server/api/router.go#L249) | registerFunc(apiRouter, "/hotspot/stores", hotStatusHandler.GetHotStores, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:250](../../../server/api/router.go#L250) | registerFunc(apiRouter, "/hotspot/buckets", hotStatusHandler.GetHotBuckets, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:253](../../../server/api/router.go#L253) | registerFunc(regionReadRouter, "/region/id/{id}", regionHandler.GetRegionByID, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:254](../../../server/api/router.go#L254) | registerFunc(regionReadEscapeRouter, "/region/key/{key}", regionHandler.GetRegion, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:258](../../../server/api/router.go#L258) | registerFunc(regionReadRouter, "/regions", regionsAllHandler.GetRegions, setMethods(http.MethodGet), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:261](../../../server/api/router.go#L261) | registerFunc(regionReadRouter, "/regions/key", regionsHandler.ScanRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:262](../../../server/api/router.go#L262) | registerFunc(regionReadRouter, "/regions/count", regionsHandler.GetRegionCount, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:263](../../../server/api/router.go#L263) | registerFunc(regionReadRouter, "/regions/store/{id}", regionsHandler.GetStoreRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:264](../../../server/api/router.go#L264) | registerFunc(regionReadRouter, "/regions/keyspace/id/{id}", regionsHandler.GetKeyspaceRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:265](../../../server/api/router.go#L265) | registerFunc(regionReadRouter, "/regions/writeflow", regionsHandler.GetTopWriteFlowRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:266](../../../server/api/router.go#L266) | registerFunc(regionReadRouter, "/regions/writequery", regionsHandler.GetTopWriteQueryRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:267](../../../server/api/router.go#L267) | registerFunc(regionReadRouter, "/regions/readflow", regionsHandler.GetTopReadFlowRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:268](../../../server/api/router.go#L268) | registerFunc(regionReadRouter, "/regions/readquery", regionsHandler.GetTopReadQueryRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:269](../../../server/api/router.go#L269) | registerFunc(regionReadRouter, "/regions/confver", regionsHandler.GetTopConfVerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:270](../../../server/api/router.go#L270) | registerFunc(regionReadRouter, "/regions/version", regionsHandler.GetTopVersionRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:271](../../../server/api/router.go#L271) | registerFunc(regionReadRouter, "/regions/size", regionsHandler.GetTopSizeRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:272](../../../server/api/router.go#L272) | registerFunc(regionReadRouter, "/regions/keys", regionsHandler.GetTopKeysRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:273](../../../server/api/router.go#L273) | registerFunc(regionReadRouter, "/regions/cpu", regionsHandler.GetTopCPURegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:274](../../../server/api/router.go#L274) | registerFunc(clusterRouter, "/regions/check/miss-peer", regionsHandler.GetMissPeerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:275](../../../server/api/router.go#L275) | registerFunc(clusterRouter, "/regions/check/extra-peer", regionsHandler.GetExtraPeerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:276](../../../server/api/router.go#L276) | registerFunc(clusterRouter, "/regions/check/pending-peer", regionsHandler.GetPendingPeerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:277](../../../server/api/router.go#L277) | registerFunc(clusterRouter, "/regions/check/down-peer", regionsHandler.GetDownPeerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:278](../../../server/api/router.go#L278) | registerFunc(clusterRouter, "/regions/check/learner-peer", regionsHandler.GetLearnerPeerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:279](../../../server/api/router.go#L279) | registerFunc(clusterRouter, "/regions/check/empty-region", regionsHandler.GetEmptyRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:280](../../../server/api/router.go#L280) | registerFunc(clusterRouter, "/regions/check/offline-peer", regionsHandler.GetOfflinePeerRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:281](../../../server/api/router.go#L281) | registerFunc(clusterRouter, "/regions/check/oversized-region", regionsHandler.GetOverSizedRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:282](../../../server/api/router.go#L282) | registerFunc(clusterRouter, "/regions/check/undersized-region", regionsHandler.GetUndersizedRegions, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:284](../../../server/api/router.go#L284) | registerFunc(regionReadRouter, "/regions/check/hist-size", regionsHandler.GetSizeHistogram, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:285](../../../server/api/router.go#L285) | registerFunc(regionReadRouter, "/regions/check/hist-keys", regionsHandler.GetKeysHistogram, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:286](../../../server/api/router.go#L286) | registerFunc(regionReadRouter, "/regions/sibling/{id}", regionsHandler.GetRegionSiblings, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:287](../../../server/api/router.go#L287) | registerFunc(clusterRouter, "/regions/accelerate-schedule", regionsHandler.AccelerateRegionsScheduleInRange, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:288](../../../server/api/router.go#L288) | registerFunc(clusterRouter, "/regions/accelerate-schedule/batch", regionsHandler.AccelerateRegionsScheduleInRanges, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:289](../../../server/api/router.go#L289) | registerFunc(clusterRouter, "/regions/scatter", regionsHandler.ScatterRegions, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:290](../../../server/api/router.go#L290) | registerFunc(clusterRouter, "/regions/split", regionsHandler.SplitRegions, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:291](../../../server/api/router.go#L291) | registerFunc(regionReadRouter, "/regions/range-holes", regionsHandler.GetRangeHoles, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:292](../../../server/api/router.go#L292) | registerFunc(clusterRouter, "/regions/replicated", regionsHandler.CheckRegionsReplicated, setMethods(http.MethodGet), setQueries("startKey", "{startKey}", "endKey", "{endKey}"), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:294](../../../server/api/router.go#L294) | registerFunc(apiRouter, "/version", newVersionHandler(rd).GetVersion, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:295](../../../server/api/router.go#L295) | registerFunc(apiRouter, "/status", newStatusHandler(svr, rd).GetPDStatus, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:298](../../../server/api/router.go#L298) | registerFunc(apiRouter, "/members", memberHandler.GetMembers, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:299](../../../server/api/router.go#L299) | registerFunc(apiRouter, "/members/name/{name}", memberHandler.DeleteMemberByName, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:300](../../../server/api/router.go#L300) | registerFunc(apiRouter, "/members/id/{id}", memberHandler.DeleteMemberByID, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:301](../../../server/api/router.go#L301) | registerFunc(apiRouter, "/members/name/{name}", memberHandler.SetMemberPropertyByName, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:304](../../../server/api/router.go#L304) | registerFunc(apiRouter, "/leader", leaderHandler.GetLeader, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:305](../../../server/api/router.go#L305) | registerFunc(apiRouter, "/leader/resign", leaderHandler.ResignLeader, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:306](../../../server/api/router.go#L306) | registerFunc(apiRouter, "/leader/transfer/{next_leader}", leaderHandler.TransferLeader, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:309](../../../server/api/router.go#L309) | registerFunc(clusterRouter, "/stats/region", statsHandler.GetRegionStatus, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:312](../../../server/api/router.go#L312) | registerFunc(regionResetRouter, "/admin/cache/region/{id}", adminHandler.DeleteRegionCache, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:313](../../../server/api/router.go#L313) | registerFunc(clusterRouter, "/admin/storage/region/{id}", adminHandler.DeleteRegionStorage, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:314](../../../server/api/router.go#L314) | registerFunc(regionResetRouter, "/admin/cache/regions", adminHandler.DeleteAllRegionCache, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:315](../../../server/api/router.go#L315) | registerFunc(apiRouter, "/admin/persist-file/{file_name}", adminHandler.SavePersistFile, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:316](../../../server/api/router.go#L316) | registerFunc(apiRouter, "/admin/cluster/markers/snapshot-recovering", adminHandler.isSnapshotRecovering, setMethods(http.MethodGet), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:317](../../../server/api/router.go#L317) | registerFunc(apiRouter, "/admin/cluster/markers/snapshot-recovering", adminHandler.markSnapshotRecovering, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:318](../../../server/api/router.go#L318) | registerFunc(apiRouter, "/admin/cluster/markers/snapshot-recovering", adminHandler.unmarkSnapshotRecovering, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:319](../../../server/api/router.go#L319) | registerFunc(apiRouter, "/admin/cluster/markers/pitr-restore-mode", adminHandler.isPitrRestoreMode, setMethods(http.MethodGet), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:320](../../../server/api/router.go#L320) | registerFunc(apiRouter, "/admin/cluster/markers/pitr-restore-mode", adminHandler.markPitrRestoreMode, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:321](../../../server/api/router.go#L321) | registerFunc(apiRouter, "/admin/cluster/markers/pitr-restore-mode", adminHandler.unmarkPitrRestoreMode, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:322](../../../server/api/router.go#L322) | registerFunc(apiRouter, "/admin/base-alloc-id", adminHandler.recoverAllocID, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:325](../../../server/api/router.go#L325) | registerFunc(apiRouter, "/service-middleware/config", serviceMiddlewareHandler.GetServiceMiddlewareConfig, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:326](../../../server/api/router.go#L326) | registerFunc(apiRouter, "/service-middleware/config", serviceMiddlewareHandler.SetServiceMiddlewareConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:327](../../../server/api/router.go#L327) | registerFunc(apiRouter, "/service-middleware/config/rate-limit", serviceMiddlewareHandler.SetRateLimitConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus), setRateLimitAllowList()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:328](../../../server/api/router.go#L328) | registerFunc(apiRouter, "/service-middleware/config/grpc-rate-limit", serviceMiddlewareHandler.SetGRPCRateLimitConfig, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus), setRateLimitAllowList()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:331](../../../server/api/router.go#L331) | registerFunc(apiRouter, "/admin/log", logHandler.SetLogLevel, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:333](../../../server/api/router.go#L333) | registerFunc(clusterRouter, "/replication_mode/status", replicationModeHandler.GetReplicationModeStatus, setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:336](../../../server/api/router.go#L336) | registerFunc(apiRouter, "/plugin", pluginHandler.loadPlugin, setMethods(http.MethodPost), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:337](../../../server/api/router.go#L337) | registerFunc(apiRouter, "/plugin", pluginHandler.unloadPlugin, setMethods(http.MethodDelete), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:340](../../../server/api/router.go#L340) | registerFunc(apiRouter, "/health", healthHandler.GetHealthStatus, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:341](../../../server/api/router.go#L341) | registerFunc(apiRouter, "/ping", healthHandler.Ping, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:346](../../../server/api/router.go#L346) | registerFunc(apiRouter, "/metric/query", metricHandler.queryMetric, setMethods(http.MethodGet, http.MethodPost), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:347](../../../server/api/router.go#L347) | registerFunc(apiRouter, "/metric/query_range", metricHandler.queryMetric, setMethods(http.MethodGet, http.MethodPost), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:351](../../../server/api/router.go#L351) | registerFunc(apiRouter, "/debug/pprof/profile", pprof.Profile, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:352](../../../server/api/router.go#L352) | registerFunc(apiRouter, "/debug/pprof/trace", pprof.Trace, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:353](../../../server/api/router.go#L353) | registerFunc(apiRouter, "/debug/pprof/symbol", pprof.Symbol, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:354](../../../server/api/router.go#L354) | registerFunc(apiRouter, "/debug/pprof/heap", pprofHandler.PProfHeap, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:355](../../../server/api/router.go#L355) | registerFunc(apiRouter, "/debug/pprof/mutex", pprofHandler.PProfMutex, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:356](../../../server/api/router.go#L356) | registerFunc(apiRouter, "/debug/pprof/allocs", pprofHandler.PProfAllocs, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:357](../../../server/api/router.go#L357) | registerFunc(apiRouter, "/debug/pprof/block", pprofHandler.PProfBlock, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:358](../../../server/api/router.go#L358) | registerFunc(apiRouter, "/debug/pprof/goroutine", pprofHandler.PProfGoroutine, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:359](../../../server/api/router.go#L359) | registerFunc(apiRouter, "/debug/pprof/threadcreate", pprofHandler.PProfThreadcreate, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:360](../../../server/api/router.go#L360) | registerFunc(apiRouter, "/debug/pprof/zip", pprofHandler.PProfZip, setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:364](../../../server/api/router.go#L364) | registerFunc(apiRouter, "/gc/safepoint", serviceGCSafepointHandler.GetGCSafePoint, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:365](../../../server/api/router.go#L365) | registerFunc(apiRouter, "/gc/safepoint/{service_id}", serviceGCSafepointHandler.DeleteGCSafePoint, setMethods(http.MethodDelete), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:369](../../../server/api/router.go#L369) | registerFunc(clusterRouter, "/min-resolved-ts", minResolvedTSHandler.GetMinResolvedTS, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:370](../../../server/api/router.go#L370) | registerFunc(clusterRouter, "/min-resolved-ts/{store_id}", minResolvedTSHandler.GetStoreMinResolvedTS, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:375](../../../server/api/router.go#L375) | unsafeOperationHandler.RemoveFailedStores, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:377](../../../server/api/router.go#L377) | unsafeOperationHandler.AbortFailedStoresRemoval, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:379](../../../server/api/router.go#L379) | unsafeOperationHandler.GetFailedStoresRemovalStatus, setMethods(http.MethodGet), setAuditBackend(prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:383](../../../server/api/router.go#L383) | registerFunc(apiRouter, "/tso/allocator/transfer/{name}", tsoHandler.TransferLocalTSOAllocator, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:386](../../../server/api/router.go#L386) | registerFunc(apiRouter, "/admin/reset-ts", tsoAdminHandler.ResetTS, setMethods(http.MethodPost), setAuditBackend(localLog, prometheus)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/router.go:394](../../../server/api/router.go#L394) | }), setAuditBackend(localLog)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/service_middleware.go:29](../../../server/api/service_middleware.go#L29) | "github.com/tikv/pd/pkg/ratelimit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:115](../../../server/api/service_middleware.go#L115) | case "audit": | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/service_middleware.go:116](../../../server/api/service_middleware.go#L116) | return h.updateAudit(cfg, kp[len(kp)-1], value) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/service_middleware.go:118](../../../server/api/service_middleware.go#L118) | return h.svr.UpdateRateLimit(&cfg.RateLimitConfig, kp[len(kp)-1], value) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:120](../../../server/api/service_middleware.go#L120) | return h.svr.UpdateGRPCRateLimit(&cfg.GRPCRateLimitConfig, kp[len(kp)-1], value) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:125](../../../server/api/service_middleware.go#L125) | func (h *serviceMiddlewareHandler) updateAudit(config *config.ServiceMiddlewareConfig, key string, value any) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/service_middleware.go:126](../../../server/api/service_middleware.go#L126) | updated, found, err := jsonutil.AddKeyValue(&config.AuditConfig, key, value) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/service_middleware.go:136](../../../server/api/service_middleware.go#L136) | err = h.svr.SetAuditConfig(config.AuditConfig) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/api/service_middleware.go:151](../../../server/api/service_middleware.go#L151) | func (h *serviceMiddlewareHandler) SetRateLimitConfig(w http.ResponseWriter, r *http.Request) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:189](../../../server/api/service_middleware.go#L189) | if h.svr.IsInRateLimitAllowList(serviceLabel) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:193](../../../server/api/service_middleware.go#L193) | oldCfg := h.svr.GetRateLimitConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:215](../../../server/api/service_middleware.go#L215) | status := h.svr.UpdateServiceRateLimiter(serviceLabel, ratelimit.UpdateDimensionConfig(&cfg)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:216](../../../server/api/service_middleware.go#L216) | if status&ratelimit.LimiterDeleted != 0 { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:218](../../../server/api/service_middleware.go#L218) | delete(cfg.RateLimitConfig.LimiterConfig, serviceLabel) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:219](../../../server/api/service_middleware.go#L219) | if err := h.svr.SetRateLimitConfig(cfg.RateLimitConfig); err != nil { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:221](../../../server/api/service_middleware.go#L221) | h.svr.UpdateServiceRateLimiter(serviceLabel, ratelimit.UpdateDimensionConfig(&old)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:228](../../../server/api/service_middleware.go#L228) | err := h.svr.UpdateRateLimitConfig("limiter-config", serviceLabel, cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:247](../../../server/api/service_middleware.go#L247) | func (h *serviceMiddlewareHandler) SetGRPCRateLimitConfig(w http.ResponseWriter, r *http.Request) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:263](../../../server/api/service_middleware.go#L263) | oldCfg := h.svr.GetGRPCRateLimitConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:285](../../../server/api/service_middleware.go#L285) | status := h.svr.UpdateGRPCServiceRateLimiter(serviceLabel, ratelimit.UpdateDimensionConfig(&cfg)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:286](../../../server/api/service_middleware.go#L286) | if status&ratelimit.LimiterDeleted != 0 { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:288](../../../server/api/service_middleware.go#L288) | delete(cfg.GRPCRateLimitConfig.LimiterConfig, serviceLabel) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:289](../../../server/api/service_middleware.go#L289) | if err := h.svr.SetGRPCRateLimitConfig(cfg.GRPCRateLimitConfig); err != nil { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:291](../../../server/api/service_middleware.go#L291) | h.svr.UpdateGRPCServiceRateLimiter(serviceLabel, ratelimit.UpdateDimensionConfig(&old)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/api/service_middleware.go:298](../../../server/api/service_middleware.go#L298) | err := h.svr.UpdateGRPCRateLimitConfig("grpc-limiter-config", serviceLabel, cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/cluster/metrics.go:86](../../../server/cluster/metrics.go#L86) | prometheus.MustRegister(regionEventCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/cluster/metrics.go:87](../../../server/cluster/metrics.go#L87) | prometheus.MustRegister(healthStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（15的健康family生命周期见[task-015](task-015.md)，全量导出归86/101） | 15/86/101 |
| [server/cluster/metrics.go:88](../../../server/cluster/metrics.go#L88) | prometheus.MustRegister(clusterStateCPUGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/cluster/metrics.go:89](../../../server/cluster/metrics.go#L89) | prometheus.MustRegister(clusterStateCurrent) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/cluster/metrics.go:90](../../../server/cluster/metrics.go#L90) | prometheus.MustRegister(storeSyncConfigEvent) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/cluster/metrics.go:91](../../../server/cluster/metrics.go#L91) | prometheus.MustRegister(updateStoreStatsGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/cluster/metrics.go:92](../../../server/cluster/metrics.go#L92) | prometheus.MustRegister(storeTriggerNetworkSlowEvict) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/cluster/metrics.go:93](../../../server/cluster/metrics.go#L93) | prometheus.MustRegister(raftClusterStartDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/config/metrics.go:28](../../../server/config/metrics.go#L28) | prometheus.MustRegister(schedulingAllowanceStatusGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 47/90 |
| [server/grpc_service.go:46](../../../server/grpc_service.go#L46) | "github.com/tikv/pd/pkg/ratelimit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:68](../../../server/grpc_service.go#L68) | errRegionHeartbeatSend   = forwardFailCounter.WithLabelValues("region_heartbeat", "send") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:69](../../../server/grpc_service.go#L69) | errRegionHeartbeatClient = forwardFailCounter.WithLabelValues("region_heartbeat", "client") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:70](../../../server/grpc_service.go#L70) | errRegionHeartbeatStream = forwardFailCounter.WithLabelValues("region_heartbeat", "stream") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:71](../../../server/grpc_service.go#L71) | errRegionHeartbeatRecv   = forwardFailCounter.WithLabelValues("region_heartbeat", "recv") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:72](../../../server/grpc_service.go#L72) | errScatterRegionSend     = forwardFailCounter.WithLabelValues("scatter_region", "send") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:73](../../../server/grpc_service.go#L73) | errSplitRegionsSend      = forwardFailCounter.WithLabelValues("split_regions", "send") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:74](../../../server/grpc_service.go#L74) | errStoreHeartbeatSend    = forwardFailCounter.WithLabelValues("store_heartbeat", "send") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:75](../../../server/grpc_service.go#L75) | errGetOperatorSend       = forwardFailCounter.WithLabelValues("get_operator", "send") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:283](../../../server/grpc_service.go#L283) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:306](../../../server/grpc_service.go#L306) | tsoHandleDuration.Observe(time.Since(start).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:435](../../../server/grpc_service.go#L435) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:499](../../../server/grpc_service.go#L499) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:618](../../../server/grpc_service.go#L618) | tsoBatchSize.Observe(float64(count)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:622](../../../server/grpc_service.go#L622) | tsoHandleDuration.Observe(time.Since(start).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:639](../../../server/grpc_service.go#L639) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:679](../../../server/grpc_service.go#L679) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:704](../../../server/grpc_service.go#L704) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:748](../../../server/grpc_service.go#L748) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:775](../../../server/grpc_service.go#L775) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:827](../../../server/grpc_service.go#L827) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:887](../../../server/grpc_service.go#L887) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:928](../../../server/grpc_service.go#L928) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:982](../../../server/grpc_service.go#L982) | storeHeartbeatHandleDuration.WithLabelValues(storeAddress, storeLabel).Observe(time.Since(start).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1110](../../../server/grpc_service.go#L1110) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1198](../../../server/grpc_service.go#L1198) | bucketReportCounter.WithLabelValues(storeAddress, storeLabel, "report", "recv").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1203](../../../server/grpc_service.go#L1203) | bucketReportCounter.WithLabelValues(storeAddress, storeLabel, "report", "err").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1206](../../../server/grpc_service.go#L1206) | bucketReportInterval.WithLabelValues(storeAddress, storeLabel).Observe(float64(buckets.GetPeriodInMs() / 1000)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1207](../../../server/grpc_service.go#L1207) | bucketReportLatency.WithLabelValues(storeAddress, storeLabel).Observe(time.Since(start).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1208](../../../server/grpc_service.go#L1208) | bucketReportCounter.WithLabelValues(storeAddress, storeLabel, "report", "ok").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1302](../../../server/grpc_service.go#L1302) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1382](../../../server/grpc_service.go#L1382) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "recv").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1383](../../../server/grpc_service.go#L1383) | regionHeartbeatLatency.WithLabelValues(storeAddress, storeLabel).Observe(float64(time.Now().Unix()) - float64(request.GetInterval().GetEndTimestamp())) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1386](../../../server/grpc_service.go#L1386) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "bind").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1396](../../../server/grpc_service.go#L1396) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "invalid-leader").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1402](../../../server/grpc_service.go#L1402) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "invalid-region").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1412](../../../server/grpc_service.go#L1412) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "no-peer").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1420](../../../server/grpc_service.go#L1420) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "err").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1425](../../../server/grpc_service.go#L1425) | regionHeartbeatHandleDuration.WithLabelValues(storeAddress, storeLabel).Observe(time.Since(start).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1426](../../../server/grpc_service.go#L1426) | regionHeartbeatCounter.WithLabelValues(storeAddress, storeLabel, "report", "ok").Inc() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1515](../../../server/grpc_service.go#L1515) | failpoint.Inject("rateLimit", func() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1516](../../../server/grpc_service.go#L1516) | failpoint.Return(nil, errs.ErrGRPCRateLimitExceeded(errs.ErrRateLimitExceeded)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1518](../../../server/grpc_service.go#L1518) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1548](../../../server/grpc_service.go#L1548) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1578](../../../server/grpc_service.go#L1578) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1607](../../../server/grpc_service.go#L1607) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1665](../../../server/grpc_service.go#L1665) | queryRegionDuration.Observe(time.Since(start).Seconds()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/grpc_service.go:1677](../../../server/grpc_service.go#L1677) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1706](../../../server/grpc_service.go#L1706) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1736](../../../server/grpc_service.go#L1736) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1778](../../../server/grpc_service.go#L1778) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1842](../../../server/grpc_service.go#L1842) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1876](../../../server/grpc_service.go#L1876) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1911](../../../server/grpc_service.go#L1911) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1939](../../../server/grpc_service.go#L1939) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:1976](../../../server/grpc_service.go#L1976) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2090](../../../server/grpc_service.go#L2090) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2106](../../../server/grpc_service.go#L2106) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2299](../../../server/grpc_service.go#L2299) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2361](../../../server/grpc_service.go#L2361) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2461](../../../server/grpc_service.go#L2461) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2505](../../../server/grpc_service.go#L2505) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2557](../../../server/grpc_service.go#L2557) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2663](../../../server/grpc_service.go#L2663) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2699](../../../server/grpc_service.go#L2699) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2733](../../../server/grpc_service.go#L2733) | done, err := s.rateLimitCheck() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2762](../../../server/grpc_service.go#L2762) | func (s *GrpcServer) rateLimitCheck() (done ratelimit.DoneFunc, err error) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2763](../../../server/grpc_service.go#L2763) | if s.GetServiceMiddlewarePersistOptions().IsGRPCRateLimitEnabled() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2765](../../../server/grpc_service.go#L2765) | limiter := s.GetGRPCRateLimiter() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/grpc_service.go:2769](../../../server/grpc_service.go#L2769) | err = errs.ErrGRPCRateLimitExceeded(err) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/metrics.go:166](../../../server/metrics.go#L166) | serviceAuditHistogram = prometheus.NewHistogramVec( | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:170](../../../server/metrics.go#L170) | Name:      "audit_handling_seconds", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:175](../../../server/metrics.go#L175) | serviceAuditCounter = prometheus.NewCounterVec( | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:179](../../../server/metrics.go#L179) | Name:      "audit_requests_total", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:180](../../../server/metrics.go#L180) | Help:      "Total number of service requests for audit", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:217](../../../server/metrics.go#L217) | prometheus.MustRegister(timeJumpBackCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:218](../../../server/metrics.go#L218) | prometheus.MustRegister(regionHeartbeatCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:219](../../../server/metrics.go#L219) | prometheus.MustRegister(regionHeartbeatLatency) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:220](../../../server/metrics.go#L220) | prometheus.MustRegister(metadataGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:221](../../../server/metrics.go#L221) | prometheus.MustRegister(etcdStateGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:222](../../../server/metrics.go#L222) | prometheus.MustRegister(tsoProxyHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:223](../../../server/metrics.go#L223) | prometheus.MustRegister(tsoProxyBatchSize) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:224](../../../server/metrics.go#L224) | prometheus.MustRegister(tsoProxyForwardTimeoutCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:225](../../../server/metrics.go#L225) | prometheus.MustRegister(tsoForwardStreamCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:226](../../../server/metrics.go#L226) | prometheus.MustRegister(tsoHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:227](../../../server/metrics.go#L227) | prometheus.MustRegister(tsoBatchSize) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:228](../../../server/metrics.go#L228) | prometheus.MustRegister(queryRegionDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:229](../../../server/metrics.go#L229) | prometheus.MustRegister(regionHeartbeatHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:230](../../../server/metrics.go#L230) | prometheus.MustRegister(storeHeartbeatHandleDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:231](../../../server/metrics.go#L231) | prometheus.MustRegister(bucketReportCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:232](../../../server/metrics.go#L232) | prometheus.MustRegister(bucketReportLatency) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:233](../../../server/metrics.go#L233) | prometheus.MustRegister(serviceAuditHistogram) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:234](../../../server/metrics.go#L234) | prometheus.MustRegister(serviceAuditCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/metrics.go:235](../../../server/metrics.go#L235) | prometheus.MustRegister(bucketReportInterval) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:236](../../../server/metrics.go#L236) | prometheus.MustRegister(apiConcurrencyGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:237](../../../server/metrics.go#L237) | prometheus.MustRegister(forwardFailCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:238](../../../server/metrics.go#L238) | prometheus.MustRegister(forwardTsoDuration) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/metrics.go:239](../../../server/metrics.go#L239) | prometheus.MustRegister(regionRequestCounter) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 15/86/101 |
| [server/server.go:56](../../../server/server.go#L56) | "github.com/tikv/pd/pkg/audit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:72](../../../server/server.go#L72) | "github.com/tikv/pd/pkg/ratelimit" | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:125](../../../server/server.go#L125) | etcdTermGauge           = etcdStateGauge.WithLabelValues("term") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:126](../../../server/server.go#L126) | etcdAppliedIndexGauge   = etcdStateGauge.WithLabelValues("appliedIndex") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:127](../../../server/server.go#L127) | etcdCommittedIndexGauge = etcdStateGauge.WithLabelValues("committedIndex") | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:228](../../../server/server.go#L228) | serviceRateLimiter *ratelimit.Controller | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:232](../../../server/server.go#L232) | grpcServiceRateLimiter *ratelimit.Controller | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:236](../../../server/server.go#L236) | serviceAuditBackendLabels map[string]*audit.BackendLabels | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:238](../../../server/server.go#L238) | auditBackends []audit.Backend | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:292](../../../server/server.go#L292) | s.auditBackends = []audit.Backend{ | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:293](../../../server/server.go#L293) | audit.NewLocalLogBackend(true), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:294](../../../server/server.go#L294) | audit.NewPrometheusBackend(serviceAuditHistogram, serviceAuditCounter, false), | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:296](../../../server/server.go#L296) | s.serviceRateLimiter = ratelimit.NewController(s.ctx, "http", apiConcurrencyGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:297](../../../server/server.go#L297) | s.grpcServiceRateLimiter = ratelimit.NewController(s.ctx, "grpc", apiConcurrencyGauge) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:298](../../../server/server.go#L298) | s.serviceAuditBackendLabels = make(map[string]*audit.BackendLabels) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:392](../../../server/server.go#L392) | tlsConfig, err := s.cfg.Security.ToClientTLSConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:433](../../../server/server.go#L433) | tlsConfig, err := s.cfg.Security.ToClientTLSConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:493](../../../server/server.go#L493) | metadataGauge.WithLabelValues(fmt.Sprintf("cluster%d", clusterID)).Set(0) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:494](../../../server/server.go#L494) | bs.ServerInfoGauge.WithLabelValues(versioninfo.PDReleaseVersion, versioninfo.PDGitHash).Set(float64(time.Now().Unix())) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:550](../../../server/server.go#L550) | return s.cfg.Security.ToClientTLSConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:595](../../../server/server.go#L595) | s.serviceRateLimiter.Update(service, ratelimit.InitLimiter()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:598](../../../server/server.go#L598) | s.grpcServiceRateLimiter.Update(service, ratelimit.InitLimiter()) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:669](../../../server/server.go#L669) | s.grpcServiceRateLimiter.Close() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:670](../../../server/server.go#L670) | s.serviceRateLimiter.Close() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:718](../../../server/server.go#L718) | func (s *Server) SetServiceAuditBackendForHTTP(route *mux.Route, labels ...string) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:723](../../../server/server.go#L723) | s.SetServiceAuditBackendLabels(route.GetName(), labels) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1211](../../../server/server.go#L1211) | cfg.AuditConfig = *s.serviceMiddlewarePersistOptions.GetAuditConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1212](../../../server/server.go#L1212) | cfg.RateLimitConfig = *s.serviceMiddlewarePersistOptions.GetRateLimitConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1213](../../../server/server.go#L1213) | cfg.GRPCRateLimitConfig = *s.serviceMiddlewarePersistOptions.GetGRPCRateLimitConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1471](../../../server/server.go#L1471) | func (s *Server) GetAuditConfig() *config.AuditConfig { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1472](../../../server/server.go#L1472) | return s.serviceMiddlewarePersistOptions.GetAuditConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1476](../../../server/server.go#L1476) | func (s *Server) SetAuditConfig(cfg config.AuditConfig) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1477](../../../server/server.go#L1477) | old := s.serviceMiddlewarePersistOptions.GetAuditConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1478](../../../server/server.go#L1478) | s.serviceMiddlewarePersistOptions.SetAuditConfig(&cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1480](../../../server/server.go#L1480) | s.serviceMiddlewarePersistOptions.SetAuditConfig(old) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1481](../../../server/server.go#L1481) | log.Error("failed to update Audit config", | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1487](../../../server/server.go#L1487) | log.Info("audit config is updated", zap.Reflect("new", cfg), zap.Reflect("old", old)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1492](../../../server/server.go#L1492) | func (s *Server) UpdateRateLimitConfig(key, label string, value ratelimit.DimensionConfig) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1494](../../../server/server.go#L1494) | rateLimitCfg := make(map[string]ratelimit.DimensionConfig) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1495](../../../server/server.go#L1495) | for label, item := range cfg.RateLimitConfig.LimiterConfig { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1496](../../../server/server.go#L1496) | rateLimitCfg[label] = item | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1498](../../../server/server.go#L1498) | rateLimitCfg[label] = value | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1499](../../../server/server.go#L1499) | return s.UpdateRateLimit(&cfg.RateLimitConfig, key, &rateLimitCfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1503](../../../server/server.go#L1503) | func (s *Server) UpdateRateLimit(cfg *config.RateLimitConfig, key string, value any) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1514](../../../server/server.go#L1514) | err = s.SetRateLimitConfig(*cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1520](../../../server/server.go#L1520) | func (s *Server) GetRateLimitConfig() *config.RateLimitConfig { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1521](../../../server/server.go#L1521) | return s.serviceMiddlewarePersistOptions.GetRateLimitConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1525](../../../server/server.go#L1525) | func (s *Server) SetRateLimitConfig(cfg config.RateLimitConfig) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1526](../../../server/server.go#L1526) | old := s.serviceMiddlewarePersistOptions.GetRateLimitConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1527](../../../server/server.go#L1527) | s.serviceMiddlewarePersistOptions.SetRateLimitConfig(&cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1529](../../../server/server.go#L1529) | s.serviceMiddlewarePersistOptions.SetRateLimitConfig(old) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1541](../../../server/server.go#L1541) | func (s *Server) UpdateGRPCRateLimitConfig(key, label string, value ratelimit.DimensionConfig) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1543](../../../server/server.go#L1543) | rateLimitCfg := make(map[string]ratelimit.DimensionConfig) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1544](../../../server/server.go#L1544) | for label, item := range cfg.GRPCRateLimitConfig.LimiterConfig { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1545](../../../server/server.go#L1545) | rateLimitCfg[label] = item | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1547](../../../server/server.go#L1547) | rateLimitCfg[label] = value | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1548](../../../server/server.go#L1548) | return s.UpdateGRPCRateLimit(&cfg.GRPCRateLimitConfig, key, &rateLimitCfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1552](../../../server/server.go#L1552) | func (s *Server) UpdateGRPCRateLimit(cfg *config.GRPCRateLimitConfig, key string, value any) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1563](../../../server/server.go#L1563) | err = s.SetGRPCRateLimitConfig(*cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1569](../../../server/server.go#L1569) | func (s *Server) GetGRPCRateLimitConfig() *config.GRPCRateLimitConfig { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1570](../../../server/server.go#L1570) | return s.serviceMiddlewarePersistOptions.GetGRPCRateLimitConfig().Clone() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1574](../../../server/server.go#L1574) | func (s *Server) SetGRPCRateLimitConfig(cfg config.GRPCRateLimitConfig) error { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1575](../../../server/server.go#L1575) | old := s.serviceMiddlewarePersistOptions.GetGRPCRateLimitConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1576](../../../server/server.go#L1576) | s.serviceMiddlewarePersistOptions.SetGRPCRateLimitConfig(&cfg) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1578](../../../server/server.go#L1578) | s.serviceMiddlewarePersistOptions.SetGRPCRateLimitConfig(old) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1713](../../../server/server.go#L1713) | func (s *Server) GetTLSConfig() *grpcutil.TLSConfig { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:1714](../../../server/server.go#L1714) | return &s.cfg.Security.TLSConfig | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:1822](../../../server/server.go#L1822) | func (s *Server) GetAuditBackend() []audit.Backend { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1823](../../../server/server.go#L1823) | return s.auditBackends | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1827](../../../server/server.go#L1827) | func (s *Server) GetServiceAuditBackendLabels(serviceLabel string) *audit.BackendLabels { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1828](../../../server/server.go#L1828) | return s.serviceAuditBackendLabels[serviceLabel] | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1832](../../../server/server.go#L1832) | func (s *Server) SetServiceAuditBackendLabels(serviceLabel string, labels []string) { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1833](../../../server/server.go#L1833) | s.serviceAuditBackendLabels[serviceLabel] = &audit.BackendLabels{Labels: labels} | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 81；主题9审计领域 |
| [server/server.go:1837](../../../server/server.go#L1837) | func (s *Server) GetServiceRateLimiter() *ratelimit.Controller { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1838](../../../server/server.go#L1838) | return s.serviceRateLimiter | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1842](../../../server/server.go#L1842) | func (s *Server) IsInRateLimitAllowList(serviceLabel string) bool { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1843](../../../server/server.go#L1843) | return s.serviceRateLimiter.IsInAllowList(serviceLabel) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1847](../../../server/server.go#L1847) | func (s *Server) UpdateServiceRateLimiter(serviceLabel string, opts ...ratelimit.Option) ratelimit.UpdateStatus { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1848](../../../server/server.go#L1848) | return s.serviceRateLimiter.Update(serviceLabel, opts...) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1852](../../../server/server.go#L1852) | func (s *Server) GetGRPCRateLimiter() *ratelimit.Controller { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1853](../../../server/server.go#L1853) | return s.grpcServiceRateLimiter | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1857](../../../server/server.go#L1857) | func (s *Server) UpdateGRPCServiceRateLimiter(serviceLabel string, opts ...ratelimit.Option) ratelimit.UpdateStatus { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:1858](../../../server/server.go#L1858) | return s.grpcServiceRateLimiter.Update(serviceLabel, opts...) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2063](../../../server/server.go#L2063) | member.ServiceMemberGauge.WithLabelValues(PD).Set(0) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:2092](../../../server/server.go#L2092) | loadTTLStart := time.Now() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:2094](../../../server/server.go#L2094) | log.Warn("failed to load persistOptions from etcd", errs.ZapError(err), zap.Duration("cost", time.Since(loadTTLStart))) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:2097](../../../server/server.go#L2097) | loadTTLDuration := time.Since(loadTTLStart) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 82/83/84/85；主题10嵌入拓扑 |
| [server/server.go:2165](../../../server/server.go#L2165) | member.ServiceMemberGauge.WithLabelValues(PD).Set(1) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 90/91/92 |
| [server/server.go:2236](../../../server/server.go#L2236) | s.loadRateLimitConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2237](../../../server/server.go#L2237) | s.loadGRPCRateLimitConfig() | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2259](../../../server/server.go#L2259) | func (s *Server) loadRateLimitConfig() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2260](../../../server/server.go#L2260) | cfg := s.serviceMiddlewarePersistOptions.GetRateLimitConfig().LimiterConfig | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2263](../../../server/server.go#L2263) | s.serviceRateLimiter.Update(key, ratelimit.UpdateDimensionConfig(&value)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2267](../../../server/server.go#L2267) | func (s *Server) loadGRPCRateLimitConfig() { | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2268](../../../server/server.go#L2268) | cfg := s.serviceMiddlewarePersistOptions.GetGRPCRateLimitConfig().LimiterConfig | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |
| [server/server.go:2271](../../../server/server.go#L2271) | s.grpcServiceRateLimiter.Update(key, ratelimit.UpdateDimensionConfig(&value)) | [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) / [pkg/server/http.rs](../../../pkg/server/http.rs)（未证实等价） | 79/80/50 |

## 测试、调用方与注入形式

下列文本扫描覆盖所固定来源全部文件，包含 Inject/InjectCall/Enable及其它failpoint形式；测试文件是已有设计参考，不是本次执行记录。Go只读约束禁止调用make/failpoint工具。Rust spawn与Go协程仅为生命周期审阅线索，不视为故障覆盖。

| 来源 | 测试/注入声明原文 | owner |
| --- | --- | --- |
| [client/resource_group/controller/global_controller.go:319](../../../client/resource_group/controller/global_controller.go#L319) | failpoint.Inject("fastCleanup", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go:322](../../../client/resource_group/controller/global_controller.go#L322) | failpoint.Inject("acceleratedReportingPeriod", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go:332](../../../client/resource_group/controller/global_controller.go#L332) | failpoint.Inject("staleRevision", func(val failpoint.Value) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go:373](../../../client/resource_group/controller/global_controller.go#L373) | failpoint.Inject("watchStreamError", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go:413](../../../client/resource_group/controller/global_controller.go#L413) | failpoint.Inject("disableWatch", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go:421](../../../client/resource_group/controller/global_controller.go#L421) | failpoint.Inject("watchStreamError", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go:477](../../../client/resource_group/controller/global_controller.go#L477) | failpoint.Inject("watchStreamError", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:47](../../../client/resource_group/controller/global_controller_test.go#L47) | func TestMain(m *testing.M) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:66](../../../client/resource_group/controller/global_controller_test.go#L66) | failpoint.Inject("gerResourceGroupError", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:126](../../../client/resource_group/controller/global_controller_test.go#L126) | func TestSendTokenBucketRequestsStopsWhenContextIsCanceled(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:151](../../../client/resource_group/controller/global_controller_test.go#L151) | func TestControllerWithTwoGroupRequestConcurrency(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:156](../../../client/resource_group/controller/global_controller_test.go#L156) | re.NoError(failpoint.Enable("github.com/tikv/pd/client/resource_group/controller/triggerPeriodicReport", fmt.Sprintf("return(\"%s\")", defaultResourceGroupName))) | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:157](../../../client/resource_group/controller/global_controller_test.go#L157) | re.NoError(failpoint.Enable("github.com/tikv/pd/client/resource_group/controller/triggerLowRUReport", fmt.Sprintf("return(\"%s\")", "test-group"))) | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:159](../../../client/resource_group/controller/global_controller_test.go#L159) | re.NoError(failpoint.Disable("github.com/tikv/pd/client/resource_group/controller/triggerPeriodicReport")) | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:160](../../../client/resource_group/controller/global_controller_test.go#L160) | re.NoError(failpoint.Disable("github.com/tikv/pd/client/resource_group/controller/triggerLowRUReport")) | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:273](../../../client/resource_group/controller/global_controller_test.go#L273) | func TestTryGetController(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:338](../../../client/resource_group/controller/global_controller_test.go#L338) | func TestCleanUpResourceGroupJudgesWholeInterval(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:396](../../../client/resource_group/controller/global_controller_test.go#L396) | func TestGetResourceGroup(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:600](../../../client/resource_group/controller/global_controller_test.go#L600) | func TestGetResourceGroupRuntimeState(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:696](../../../client/resource_group/controller/global_controller_test.go#L696) | func TestTokenBucketsRequestWithKeyspaceID(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:748](../../../client/resource_group/controller/global_controller_test.go#L748) | func TestGetRUVersionDefault(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:758](../../../client/resource_group/controller/global_controller_test.go#L758) | func TestGetRUVersionAfterSet(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:777](../../../client/resource_group/controller/global_controller_test.go#L777) | func TestRUVersionFromControllerConfig(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:794](../../../client/resource_group/controller/global_controller_test.go#L794) | func TestRUVersionOverrideFromControllerConfig(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:811](../../../client/resource_group/controller/global_controller_test.go#L811) | func TestRUVersionDefaultFallback(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:828](../../../client/resource_group/controller/global_controller_test.go#L828) | func TestRUVersionNilPolicy(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:845](../../../client/resource_group/controller/global_controller_test.go#L845) | func TestRUVersionFromInitialControllerConfig(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go:876](../../../client/resource_group/controller/global_controller_test.go#L876) | func TestRUVersionWatchViaControllerConfig(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:414](../../../client/resource_group/controller/group_controller.go#L414) | failpoint.Inject("degradedModeRU", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:482](../../../client/resource_group/controller/group_controller.go#L482) | failpoint.Inject("acceleratedReportingPeriod", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:495](../../../client/resource_group/controller/group_controller.go#L495) | failpoint.Inject("acceleratedSpeedTrend", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:512](../../../client/resource_group/controller/group_controller.go#L512) | failpoint.Inject("acceleratedReportingPeriod", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:624](../../../client/resource_group/controller/group_controller.go#L624) | failpoint.Inject("triggerUpdate", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:631](../../../client/resource_group/controller/group_controller.go#L631) | failpoint.Inject("triggerPeriodicReport", func(val failpoint.Value) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:639](../../../client/resource_group/controller/group_controller.go#L639) | failpoint.Inject("triggerLowRUReport", func(val failpoint.Value) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go:767](../../../client/resource_group/controller/group_controller.go#L767) | failpoint.Inject("triggerUpdate", func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:80](../../../client/resource_group/controller/group_controller_test.go#L80) | func TestGroupControlBurstable(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:92](../../../client/resource_group/controller/group_controller_test.go#L92) | func TestCalcAvgSkipsNonPositiveDeltaDuration(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:117](../../../client/resource_group/controller/group_controller_test.go#L117) | func TestSmallPositiveConsumptionDoesNotBypassThreshold(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:131](../../../client/resource_group/controller/group_controller_test.go#L131) | func TestDirectReportConsumptionReportsOnlyConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:149](../../../client/resource_group/controller/group_controller_test.go#L149) | func TestRequestAndResponseConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:256](../../../client/resource_group/controller/group_controller_test.go#L256) | func TestRUCalculationDetail(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:391](../../../client/resource_group/controller/group_controller_test.go#L391) | func TestOnResponseWaitConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:430](../../../client/resource_group/controller/group_controller_test.go#L430) | func TestOnResponseWaitZeroSettlementObservesSuccessfulDuration(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:442](../../../client/resource_group/controller/group_controller_test.go#L442) | func TestPredictedReadBytesPreCharge(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:484](../../../client/resource_group/controller/group_controller_test.go#L484) | func TestPagingPrechargeDoesNotEnterReportedRequestConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:512](../../../client/resource_group/controller/group_controller_test.go#L512) | func TestPagingPrechargeRefundDoesNotEnterReportedResponseConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:553](../../../client/resource_group/controller/group_controller_test.go#L553) | func TestPredictedReadBytesRequiresCopRead(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:586](../../../client/resource_group/controller/group_controller_test.go#L586) | func TestPagingPreChargeTokenRefund(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:638](../../../client/resource_group/controller/group_controller_test.go#L638) | func TestPagingPreChargeNoRefundWhenActualExceedsEstimate(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:674](../../../client/resource_group/controller/group_controller_test.go#L674) | func TestOnResponseWaitPrechargedPositiveSettlementDebitsImmediately(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:719](../../../client/resource_group/controller/group_controller_test.go#L719) | func TestOnResponseImplPagingRefund(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:755](../../../client/resource_group/controller/group_controller_test.go#L755) | func TestNegativePagingSettlementDoesNotForceConsumptionReport(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:796](../../../client/resource_group/controller/group_controller_test.go#L796) | func TestPagingPreChargeRefundOnFailedRead(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:839](../../../client/resource_group/controller/group_controller_test.go#L839) | func TestFailedWriteDoesNotUsePagingRefundPath(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:872](../../../client/resource_group/controller/group_controller_test.go#L872) | func TestFailedWriteOnResponseDoesNotUsePagingRefundPath(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:905](../../../client/resource_group/controller/group_controller_test.go#L905) | func TestNonCopPredictedReadBytesResponseIgnoresPagingAccounting(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:943](../../../client/resource_group/controller/group_controller_test.go#L943) | func TestDeletePagingLabelsResetsSeries(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:985](../../../client/resource_group/controller/group_controller_test.go#L985) | func TestCopReadNoPrechargeGatedByIsCop(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1026](../../../client/resource_group/controller/group_controller_test.go#L1026) | func TestCopReadNoPrechargeCounterObservedOnRequest(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1044](../../../client/resource_group/controller/group_controller_test.go#L1044) | func TestNoPreChargeWithoutPredictedReadBytes(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1073](../../../client/resource_group/controller/group_controller_test.go#L1073) | func TestHandleTokenBucketUpdateEventCanceledByInitCounterNotify(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1104](../../../client/resource_group/controller/group_controller_test.go#L1104) | func TestHandleTokenBucketUpdateEventCleansNotifyOnTimer(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1150](../../../client/resource_group/controller/group_controller_test.go#L1150) | func TestResourceGroupThrottledError(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1163](../../../client/resource_group/controller/group_controller_test.go#L1163) | func TestPagingPrechargeNotObservedOnThrottle(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1188](../../../client/resource_group/controller/group_controller_test.go#L1188) | func TestAcquireTokensSignalAwareWait(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1255](../../../client/resource_group/controller/group_controller_test.go#L1255) | func TestAcquireTokensFallbackToTimer(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1289](../../../client/resource_group/controller/group_controller_test.go#L1289) | func TestAcquireTokensCancelKeepsLastMonotonic(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1303](../../../client/resource_group/controller/group_controller_test.go#L1303) | re.NoError(failpoint.EnableCall(fp, func() { | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go:1307](../../../client/resource_group/controller/group_controller_test.go#L1307) | defer func() { re.NoError(failpoint.Disable(fp)) }() | 主题8；78/102边界 |
| [client/resource_group/controller/limiter.go:575](../../../client/resource_group/controller/limiter.go#L575) | failpoint.InjectCall("waitReservationsBeforeSelect") | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:100](../../../client/resource_group/controller/limiter_test.go#L100) | func TestSimpleReserve(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:119](../../../client/resource_group/controller/limiter_test.go#L119) | func TestReconfig(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:152](../../../client/resource_group/controller/limiter_test.go#L152) | func TestLastStaysMonotonicOnStaleNow(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:231](../../../client/resource_group/controller/limiter_test.go#L231) | func TestRefundTokens(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:265](../../../client/resource_group/controller/limiter_test.go#L265) | func TestNotify(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:283](../../../client/resource_group/controller/limiter_test.go#L283) | func TestCancel(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:328](../../../client/resource_group/controller/limiter_test.go#L328) | func TestCancelErrorOfReservation(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:341](../../../client/resource_group/controller/limiter_test.go#L341) | func TestQPS(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:361](../../../client/resource_group/controller/limiter_test.go#L361) | func TestReconfiguredCh(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go:401](../../../client/resource_group/controller/limiter_test.go#L401) | func TestReconfiguredChWakesMultipleWaiters(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:61](../../../client/resource_group/controller/model_test.go#L61) | func TestGetRUValueFromConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:78](../../../client/resource_group/controller/model_test.go#L78) | func TestAdd(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:119](../../../client/resource_group/controller/model_test.go#L119) | func TestSub(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:159](../../../client/resource_group/controller/model_test.go#L159) | func TestUpdateDeltaConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:172](../../../client/resource_group/controller/model_test.go#L172) | func TestUpdateDeltaConsumptionReportsEmptyConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:183](../../../client/resource_group/controller/model_test.go#L183) | func TestUpdateDeltaConsumptionIgnoresDecreasedRequestUnits(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:234](../../../client/resource_group/controller/model_test.go#L234) | func TestRequestInfoPagingProvidersAreOptional(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:254](../../../client/resource_group/controller/model_test.go#L254) | func TestRequestInfoMissingPredictionProviderReturnsZeroHint(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:264](../../../client/resource_group/controller/model_test.go#L264) | func TestReportedConsumptionStripsPagingPrecharge(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:288](../../../client/resource_group/controller/model_test.go#L288) | func TestReportedConsumptionRestoresPagingActualUsage(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:312](../../../client/resource_group/controller/model_test.go#L312) | func TestReportedConsumptionReusesDeltaWithoutPagingPrecharge(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go:329](../../../client/resource_group/controller/model_test.go#L329) | func TestEqualRU(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:87](../../../client/resource_group/controller/request_source_metrics_test.go#L87) | func TestRequestSourceMetricsRecordAndCacheAccountingDeltas(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:139](../../../client/resource_group/controller/request_source_metrics_test.go#L139) | func TestRequestSourceMetricsSkipRejectedAndZeroDeltas(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:162](../../../client/resource_group/controller/request_source_metrics_test.go#L162) | func TestRequestSourceMetricsConcurrentLazyCreation(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:187](../../../client/resource_group/controller/request_source_metrics_test.go#L187) | func TestCleanupResourceGroupRemovesRequestSourceMetrics(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:241](../../../client/resource_group/controller/request_source_metrics_test.go#L241) | func TestCleanupDoesNotReexportExistingCachedHandle(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:301](../../../client/resource_group/controller/request_source_metrics_test.go#L301) | func TestCleanupPreventsRecreateRequestSourceMetrics(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:360](../../../client/resource_group/controller/request_source_metrics_test.go#L360) | func TestTombstoneCleanupRemovesExistingRequestSourceMetrics(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:421](../../../client/resource_group/controller/request_source_metrics_test.go#L421) | func TestRevivedResourceGroupCleanupRemovesExistingRequestSourceMetrics(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:501](../../../client/resource_group/controller/request_source_metrics_test.go#L501) | func TestGetOrCreateAfterCleanupReturnsFreshState(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:587](../../../client/resource_group/controller/request_source_metrics_test.go#L587) | func TestCleanupThenRecreateViaFullPath(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:658](../../../client/resource_group/controller/request_source_metrics_test.go#L658) | func TestStopCleansUpRequestSourceMetricsState(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go:725](../../../client/resource_group/controller/request_source_metrics_test.go#L725) | func TestFailedWriteMatchesControllerConsumption(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/util_test.go:29](../../../client/resource_group/controller/util_test.go#L29) | func TestDurationJSON(t *testing.T) { | 主题8；78/102边界 |
| [client/resource_group/controller/util_test.go:42](../../../client/resource_group/controller/util_test.go#L42) | func TestDurationTOML(t *testing.T) { | 主题8；78/102边界 |
| [cmd/pd-server/main_test.rs:6](../../../cmd/pd-server/main_test.rs#L6) | fn standalone_runtime_lifecycle() { | 90/91/92 |
| [pkg/audit/audit_test.go:36](../../../pkg/audit/audit_test.go#L36) | func TestMain(m *testing.M) { | 主题9；81/86/92/102接线 |
| [pkg/audit/audit_test.go:40](../../../pkg/audit/audit_test.go#L40) | func TestLabelMatcher(t *testing.T) { | 主题9；81/86/92/102接线 |
| [pkg/audit/audit_test.go:49](../../../pkg/audit/audit_test.go#L49) | func TestPrometheusBackend(t *testing.T) { | 主题9；81/86/92/102接线 |
| [pkg/audit/audit_test.go:108](../../../pkg/audit/audit_test.go#L108) | func TestLocalLogBackendUsingFile(t *testing.T) { | 主题9；81/86/92/102接线 |
| [pkg/encryption/config_test.go:26](../../../pkg/encryption/config_test.go#L26) | func TestAdjustDefaultValue(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/config_test.go:38](../../../pkg/encryption/config_test.go#L38) | func TestAdjustInvalidDataEncryptionMethod(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/config_test.go:44](../../../pkg/encryption/config_test.go#L44) | func TestAdjustNegativeRotationDuration(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/config_test.go:50](../../../pkg/encryption/config_test.go#L50) | func TestAdjustInvalidMasterKeyType(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/config_test.rs:4](../../../pkg/encryption/config_test.rs#L4) | fn encryption_defaults_and_methods() { | 主题8；78/102边界 |
| [pkg/encryption/config_test.rs:34](../../../pkg/encryption/config_test.rs#L34) | fn encryption_errors_preserve_code_precedence_and_partial_defaults() { | 主题8；78/102边界 |
| [pkg/encryption/crypter_test.go:27](../../../pkg/encryption/crypter_test.go#L27) | func TestEncryptionMethodSupported(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/crypter_test.go:36](../../../pkg/encryption/crypter_test.go#L36) | func TestKeyLength(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/crypter_test.go:53](../../../pkg/encryption/crypter_test.go#L53) | func TestNewIv(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/crypter_test.go:63](../../../pkg/encryption/crypter_test.go#L63) | func TestNewDataKey(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/crypter_test.go:81](../../../pkg/encryption/crypter_test.go#L81) | func TestAesGcmCrypter(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:40](../../../pkg/encryption/key_manager_test.go#L40) | func TestMain(m *testing.M) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:95](../../../pkg/encryption/key_manager_test.go#L95) | func TestNewKeyManagerBasic(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:117](../../../pkg/encryption/key_manager_test.go#L117) | func TestNewKeyManagerWithCustomConfig(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:155](../../../pkg/encryption/key_manager_test.go#L155) | func TestNewKeyManagerLoadKeys(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:196](../../../pkg/encryption/key_manager_test.go#L196) | func TestGetCurrentKey(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:239](../../../pkg/encryption/key_manager_test.go#L239) | func TestGetKey(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:294](../../../pkg/encryption/key_manager_test.go#L294) | func TestLoadKeyEmpty(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:328](../../../pkg/encryption/key_manager_test.go#L328) | func TestWatcher(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:407](../../../pkg/encryption/key_manager_test.go#L407) | func TestSetLeadershipWithEncryptionOff(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:430](../../../pkg/encryption/key_manager_test.go#L430) | func TestSetLeadershipWithEncryptionEnabling(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:483](../../../pkg/encryption/key_manager_test.go#L483) | func TestSetLeadershipWithEncryptionMethodChanged(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:559](../../../pkg/encryption/key_manager_test.go#L559) | func TestSetLeadershipWithCurrentKeyExposed(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:630](../../../pkg/encryption/key_manager_test.go#L630) | func TestSetLeadershipWithCurrentKeyExpired(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:705](../../../pkg/encryption/key_manager_test.go#L705) | func TestSetLeadershipWithMasterKeyChanged(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:772](../../../pkg/encryption/key_manager_test.go#L772) | func TestSetLeadershipMasterKeyWithCiphertextKey(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:850](../../../pkg/encryption/key_manager_test.go#L850) | func TestSetLeadershipWithEncryptionDisabling(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:906](../../../pkg/encryption/key_manager_test.go#L906) | func TestKeyRotation(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go:1002](../../../pkg/encryption/key_manager_test.go#L1002) | func TestKeyRotationConflict(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:28](../../../pkg/encryption/master_key_test.go#L28) | func TestPlaintextMasterKey(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:53](../../../pkg/encryption/master_key_test.go#L53) | func TestEncrypt(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:68](../../../pkg/encryption/master_key_test.go#L68) | func TestDecrypt(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:84](../../../pkg/encryption/master_key_test.go#L84) | func TestNewFileMasterKeyMissingPath(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:97](../../../pkg/encryption/master_key_test.go#L97) | func TestNewFileMasterKeyMissingFile(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:111](../../../pkg/encryption/master_key_test.go#L111) | func TestNewFileMasterKeyNotHexString(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:127](../../../pkg/encryption/master_key_test.go#L127) | func TestNewFileMasterKeyLengthMismatch(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go:143](../../../pkg/encryption/master_key_test.go#L143) | func TestNewFileMasterKey(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:73](../../../pkg/encryption/region_crypter_test.go#L73) | func TestNilRegion(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:83](../../../pkg/encryption/region_crypter_test.go#L83) | func TestEncryptRegionWithoutKeyManager(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:99](../../../pkg/encryption/region_crypter_test.go#L99) | func TestEncryptRegionWhileEncryptionDisabled(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:117](../../../pkg/encryption/region_crypter_test.go#L117) | func TestEncryptRegion(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:151](../../../pkg/encryption/region_crypter_test.go#L151) | func TestDecryptRegionNotEncrypted(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:168](../../../pkg/encryption/region_crypter_test.go#L168) | func TestDecryptRegionWithoutKeyManager(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:183](../../../pkg/encryption/region_crypter_test.go#L183) | func TestDecryptRegionWhileKeyMissing(t *testing.T) { | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go:203](../../../pkg/encryption/region_crypter_test.go#L203) | func TestDecryptRegion(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:177](../../../pkg/gc/metrics_test.go#L177) | func TestBarrierMetricsAgeBoundaries(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:226](../../../pkg/gc/metrics_test.go#L226) | func TestBarrierMetricsNameRefreshPreservesIdentityAndWarnings(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:252](../../../pkg/gc/metrics_test.go#L252) | func TestBarrierMetricsRenewalExpiryAndRecovery(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:296](../../../pkg/gc/metrics_test.go#L296) | func TestBarrierMetricsHealthySetAndGlobalIdentity(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:321](../../../pkg/gc/metrics_test.go#L321) | func TestBarrierMetricsRegistrationAndLeadership(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:357](../../../pkg/gc/metrics_test.go#L357) | func TestBarrierMetricsConcurrentLifecycleAndScrapes(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:388](../../../pkg/gc/metrics_test.go#L388) | func TestBarrierMetricsManagerFixedWarningAge(t *testing.T) { | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go:395](../../../pkg/gc/metrics_test.go#L395) | func TestBarrierMetricsScrapeUnlocksBeforeSending(t *testing.T) { | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/config.go:212](../../../pkg/mcs/resourcemanager/server/config.go#L212) | failpoint.Inject("enableDegradedModeAndTraceLog", func() { | 主题8；78/102边界 |
| [pkg/ratelimit/concurrency_limiter_test.go:30](../../../pkg/ratelimit/concurrency_limiter_test.go#L30) | func TestMain(m *testing.M) { | 90/91/92 |
| [pkg/ratelimit/concurrency_limiter_test.go:34](../../../pkg/ratelimit/concurrency_limiter_test.go#L34) | func TestConcurrencyLimiter(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/concurrency_limiter_test.go:66](../../../pkg/ratelimit/concurrency_limiter_test.go#L66) | func TestConcurrencyLimiter2(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/concurrency_limiter_test.go:106](../../../pkg/ratelimit/concurrency_limiter_test.go#L106) | func TestConcurrencyLimiterAcquire(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs:6](../../../pkg/ratelimit/concurrency_test.rs#L6) | fn concurrency_allow_release() { | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs:53](../../../pkg/ratelimit/concurrency_test.rs#L53) | fn concurrency_allow_release_parallel() { | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs:95](../../../pkg/ratelimit/concurrency_test.rs#L95) | async fn concurrency_tokens() { | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs:156](../../../pkg/ratelimit/concurrency_test.rs#L156) | async fn concurrency_tokens_parallel() { | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs:164](../../../pkg/ratelimit/concurrency_test.rs#L164) | handles.push(tokio::spawn(async move { | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs:185](../../../pkg/ratelimit/concurrency_test.rs#L185) | async fn concurrency_tokens_cancel_head() { | 90/91/92 |
| [pkg/ratelimit/controller_test.go:83](../../../pkg/ratelimit/controller_test.go#L83) | func TestControllerWithConcurrencyLimiter(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/controller_test.go:195](../../../pkg/ratelimit/controller_test.go#L195) | func TestBlockList(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/controller_test.go:216](../../../pkg/ratelimit/controller_test.go#L216) | func TestControllerWithQPSLimiter(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/controller_test.go:325](../../../pkg/ratelimit/controller_test.go#L325) | func TestControllerWithTwoLimiters(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/controller_test.rs:6](../../../pkg/ratelimit/controller_test.rs#L6) | async fn controller_labels_and_allowlist() { | 90/91/92 |
| [pkg/ratelimit/controller_test.rs:64](../../../pkg/ratelimit/controller_test.rs#L64) | async fn controller_labels_and_allowlist_parallel() { | 90/91/92 |
| [pkg/ratelimit/controller_test.rs:92](../../../pkg/ratelimit/controller_test.rs#L92) | async fn controller_qps_and_metrics() { | 90/91/92 |
| [pkg/ratelimit/controller_test.rs:132](../../../pkg/ratelimit/controller_test.rs#L132) | tokio::spawn(async move { c.run_metrics().await }) | 90/91/92 |
| [pkg/ratelimit/limiter_test.go:43](../../../pkg/ratelimit/limiter_test.go#L43) | func TestWithConcurrencyLimiter(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/limiter_test.go:105](../../../pkg/ratelimit/limiter_test.go#L105) | func TestWithQPSLimiter(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/limiter_test.go:178](../../../pkg/ratelimit/limiter_test.go#L178) | func TestWithTwoLimiters(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/limiter_test.rs:5](../../../pkg/ratelimit/limiter_test.rs#L5) | async fn limiter_config_updates() { | 90/91/92 |
| [pkg/ratelimit/limiter_test.rs:55](../../../pkg/ratelimit/limiter_test.rs#L55) | async fn combined_limiter_allow() { | 90/91/92 |
| [pkg/ratelimit/limiter_test.rs:88](../../../pkg/ratelimit/limiter_test.rs#L88) | async fn combined_limiter_allow_parallel() { | 90/91/92 |
| [pkg/ratelimit/limiter_test.rs:102](../../../pkg/ratelimit/limiter_test.rs#L102) | jobs.push(tokio::spawn(async move { l.allow().await })); | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:8](../../../pkg/ratelimit/rate_limiter_test.rs#L8) | async fn token_bucket_oracle_and_updates() { | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:34](../../../pkg/ratelimit/rate_limiter_test.rs#L34) | async fn token_bucket_wait_cancel_deadline_and_refund() { | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:48](../../../pkg/ratelimit/rate_limiter_test.rs#L48) | let task = tokio::spawn(async move { limiter.wait_n(2, &c, None).await }); | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:63](../../../pkg/ratelimit/rate_limiter_test.rs#L63) | async fn token_bucket_zero_infinite_and_concurrency() { | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:75](../../../pkg/ratelimit/rate_limiter_test.rs#L75) | jobs.push(tokio::spawn(async move { l.allow().await })); | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:85](../../../pkg/ratelimit/rate_limiter_test.rs#L85) | async fn token_bucket_wait_serializes_and_drop_refunds() { | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:90](../../../pkg/ratelimit/rate_limiter_test.rs#L90) | tokio::spawn(async move { limiter.wait_n(1, &CancellationToken::new(), None).await }); | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:93](../../../pkg/ratelimit/rate_limiter_test.rs#L93) | let update = tokio::spawn(async move { limiter.set_burst(2).await }); | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:106](../../../pkg/ratelimit/rate_limiter_test.rs#L106) | async fn token_bucket_fractional_refill_and_burst_growth() { | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs:124](../../../pkg/ratelimit/rate_limiter_test.rs#L124) | async fn token_bucket_available_does_not_consume_on_running_clock() { | 90/91/92 |
| [pkg/ratelimit/ratelimiter_test.go:24](../../../pkg/ratelimit/ratelimiter_test.go#L24) | func TestRateLimiter(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/runner.rs:153](../../../pkg/ratelimit/runner.rs#L153) | self.worker = Some(tokio::spawn(async move { | 90/91/92 |
| [pkg/ratelimit/runner_test.go:28](../../../pkg/ratelimit/runner_test.go#L28) | func TestConcurrentRunner(t *testing.T) { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:8](../../../pkg/ratelimit/runner_test.rs#L8) | async fn runner_lifecycle_dispatch_and_release() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:38](../../../pkg/ratelimit/runner_test.rs#L38) | async fn runner_lifecycle_completion_schedules_next() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:60](../../../pkg/ratelimit/runner_test.rs#L60) | async fn runner_lifecycle_panic_releases_token() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:79](../../../pkg/ratelimit/runner_test.rs#L79) | async fn runner_lifecycle_unlimited_and_parent_cancel() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:96](../../../pkg/ratelimit/runner_test.rs#L96) | fn runner_lifecycle_sync() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:109](../../../pkg/ratelimit/runner_test.rs#L109) | async fn runner_lifecycle_stop_during_acquire_and_restart() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:147](../../../pkg/ratelimit/runner_test.rs#L147) | async fn runner_dedup_and_queue_age_retained_capacity() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:214](../../../pkg/ratelimit/runner_test.rs#L214) | async fn runner_dedup_and_queue_storage_reuse_release_fifo() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:296](../../../pkg/ratelimit/runner_test.rs#L296) | async fn runner_dedup_and_queue_unlimited_dispatches_without_completion() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:325](../../../pkg/ratelimit/runner_test.rs#L325) | async fn runner_timeout_and_cancellation_age_boundary() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:380](../../../pkg/ratelimit/runner_test.rs#L380) | async fn runner_timeout_and_cancellation_burst_cleanup() { | 90/91/92 |
| [pkg/ratelimit/runner_test.rs:446](../../../pkg/ratelimit/runner_test.rs#L446) | async fn runner_timeout_and_cancellation_concurrent_burst_drains() { | 90/91/92 |
| [pkg/runtime/node.rs:183](../../../pkg/runtime/node.rs#L183) | let worker = tokio::spawn(async move { | 90/91/92 |
| [pkg/runtime/node_test.rs:11](../../../pkg/runtime/node_test.rs#L11) | async fn shared_runtime_network_lifecycle() { | 90/91/92 |
| [pkg/runtime/node_test.rs:128](../../../pkg/runtime/node_test.rs#L128) | async fn startup_failure_releases_term_and_listener() { | 90/91/92 |
| [pkg/runtime/node_test.rs:162](../../../pkg/runtime/node_test.rs#L162) | async fn lease_loss_stops_admission_and_reports_failure() { | 90/91/92 |
| [pkg/schedule/config/config_provider_test.rs:7](../../../pkg/schedule/config/config_provider_test.rs#L7) | fn label(key: &str, value: &str) -> StoreLabel { | 48 |
| [pkg/schedule/config/config_provider_test.rs:14](../../../pkg/schedule/config/config_provider_test.rs#L14) | fn config_provider_shared_generation_and_store_delegation() { | 48 |
| [pkg/schedule/config/config_provider_test.rs:46](../../../pkg/schedule/config/config_provider_test.rs#L46) | fn config_provider_reject_leader_exact_or_matching() { | 48 |
| [pkg/schedule/config/config_provider_test.rs:70](../../../pkg/schedule/config/config_provider_test.rs#L70) | fn config_provider_registry_and_scheduler_first_match() { | 48 |
| [pkg/schedule/config/config_provider_test.rs:100](../../../pkg/schedule/config/config_provider_test.rs#L100) | fn config_provider_flags_rates_and_integer_boundaries() { | 48 |
| [pkg/schedule/config/config_test.rs:3](../../../pkg/schedule/config/config_test.rs#L3) | fn configured() -> ScheduleConfig { | 48 |
| [pkg/schedule/config/config_test.rs:14](../../../pkg/schedule/config/config_test.rs#L14) | fn schedule_defaults_presence_reload_and_snapshot() { | 48 |
| [pkg/schedule/config/config_test.rs:39](../../../pkg/schedule/config/config_test.rs#L39) | fn schedule_all_validation_branches() { | 48 |
| [pkg/schedule/config/config_test.rs:99](../../../pkg/schedule/config/config_test.rs#L99) | fn schedule_five_deprecated_flag_truth_tables() { | 48 |
| [pkg/schedule/config/config_test.rs:141](../../../pkg/schedule/config/config_test.rs#L141) | fn schedule_store_migration_and_json_inheritance() { | 48 |
| [pkg/schedule/config/config_test.rs:165](../../../pkg/schedule/config/config_test.rs#L165) | fn replication_labels_and_isolation() { | 48 |
| [pkg/schedule/config/config_values_test.rs:4](../../../pkg/schedule/config/config_values_test.rs#L4) | fn fixtures() -> serde_json::Value { | 48 |
| [pkg/schedule/config/config_values_test.rs:12](../../../pkg/schedule/config/config_values_test.rs#L12) | fn config_values_json_document_boundary() { | 48 |
| [pkg/schedule/config/config_values_test.rs:23](../../../pkg/schedule/config/config_values_test.rs#L23) | fn config_values_json_null_numbers() { | 48 |
| [pkg/schedule/config/config_values_test.rs:29](../../../pkg/schedule/config/config_values_test.rs#L29) | fn config_values_go_snapshots_round_trip() { | 48 |
| [pkg/schedule/config/config_values_test.rs:56](../../../pkg/schedule/config/config_values_test.rs#L56) | fn config_values_go_toml_defaults_presence_and_errors() { | 48 |
| [pkg/schedule/config/config_values_test.rs:116](../../../pkg/schedule/config/config_values_test.rs#L116) | fn config_values_clone_owned_collections() { | 48 |
| [pkg/schedule/config/config_values_test.rs:154](../../../pkg/schedule/config/config_values_test.rs#L154) | fn config_values_label_keys_go_table() { | 48 |
| [pkg/schedule/config/config_values_test.rs:186](../../../pkg/schedule/config/config_values_test.rs#L186) | fn config_values_label_values_and_first_error() { | 48 |
| [pkg/schedule/config/store_config_test.go:26](../../../pkg/schedule/config/store_config_test.go#L26) | func TestMain(m *testing.M) { | 48 |
| [pkg/schedule/config/store_config_test.go:30](../../../pkg/schedule/config/store_config_test.go#L30) | func TestMergeCheck(t *testing.T) { | 48 |
| [pkg/schedule/config/store_config_test.go:84](../../../pkg/schedule/config/store_config_test.go#L84) | func TestStoreConfigEqualChecksReadPool(t *testing.T) { | 48 |
| [pkg/schedule/config/store_config_test.rs:5](../../../pkg/schedule/config/store_config_test.rs#L5) | fn store_config_merge_go_table_and_boundaries() { | 48 |
| [pkg/schedule/config/store_config_test.rs:34](../../../pkg/schedule/config/store_config_test.rs#L34) | fn store_config_units_adjust_and_bucket() { | 48 |
| [pkg/schedule/config/store_config_test.rs:69](../../../pkg/schedule/config/store_config_test.rs#L69) | fn store_config_clone_equal_readpool_and_engine() { | 48 |
| [pkg/schedule/config/store_config_test.rs:91](../../../pkg/schedule/config/store_config_test.rs#L91) | fn store_config_json_null_and_cache_exclusion() { | 48 |
| [pkg/schedule/config/util_test.go:25](../../../pkg/schedule/config/util_test.go#L25) | func TestValidateLabels(t *testing.T) { | 48 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:30](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L30) | fn is_bootstrapped(&self) -> bool { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:33](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L33) | async fn bucket_heartbeat( | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:40](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L40) | fn drop(&mut self) { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:59](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L59) | fn identity(&self) -> &crate::Identity { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:62](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L62) | fn bucket_heartbeat_service(&self) -> Option<&dyn BucketHeartbeatService> { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:65](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L65) | async fn healthy(&self) -> Result<(), String> { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:68](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L68) | async fn alloc_id(&self) -> Result<u64, String> { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:71](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L71) | async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:75](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L75) | fn request(version: u64) -> pdpb::ReportBucketsRequest { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:111](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L111) | async fn new() -> Self { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:170](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L170) | let server = tokio::spawn(async move { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:185](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L185) | async fn close(mut self) { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:194](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L194) | async fn stats(&self) -> BucketStats { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:203](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L203) | fn version(&self) -> u64 { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:215](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L215) | async fn exchange( | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:224](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L224) | fn eof(result: Result<pdpb::ReportBucketsResponse, tonic::Status>) { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:230](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L230) | async fn state_task_49_bucket_heartbeat_adapter_wire_versions_and_loads() { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:247](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L247) | async fn state_task_49_bucket_heartbeat_adapter_applied_old_versions_missing_removed() { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:292](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L292) | async fn state_task_49_bucket_heartbeat_adapter_defaults_errors_and_empty() { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:364](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L364) | async fn state_task_49_bucket_heartbeat_adapter_competing_streams() { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:386](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L386) | async fn wait_active(host: &Host, value: usize) { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:396](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L396) | async fn state_task_49_bucket_heartbeat_adapter_cancel_and_disconnect() { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:401](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L401) | let call = tokio::spawn(async move { exchange(&mut client, vec![request(1)]).await }); | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:423](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L423) | async fn state_task_49_bucket_heartbeat_adapter_open_stream_has_no_ack() { | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs:441](../../../pkg/server/bucket_heartbeat_adapter_test.rs#L441) | async fn state_task_49_bucket_heartbeat_adapter_same_stream_continues_unknown_store() { | 90/91/92 |
| [pkg/server/core_region_adapter_test.rs:5](../../../pkg/server/core_region_adapter_test.rs#L5) | fn core_task_61_metadata_roles() { | 90/91/92 |
| [pkg/server/core_region_adapter_test.rs:43](../../../pkg/server/core_region_adapter_test.rs#L43) | fn core_task_61_sizes() { | 90/91/92 |
| [pkg/server/core_region_adapter_test.rs:72](../../../pkg/server/core_region_adapter_test.rs#L72) | fn core_task_61_heartbeat_fields() { | 90/91/92 |
| [pkg/server/core_region_adapter_test.rs:142](../../../pkg/server/core_region_adapter_test.rs#L142) | fn core_task_61_missing_fields() { | 90/91/92 |
| [pkg/server/core_store_adapter_test.rs:6](../../../pkg/server/core_store_adapter_test.rs#L6) | fn core_task_60_defaults_and_presence() { | 90/91/92 |
| [pkg/server/core_store_adapter_test.rs:17](../../../pkg/server/core_store_adapter_test.rs#L17) | fn core_task_60_unknown_enums_and_signed_extremes() { | 90/91/92 |
| [pkg/server/core_store_adapter_test.rs:56](../../../pkg/server/core_store_adapter_test.rs#L56) | fn core_task_60_full_store() { | 90/91/92 |
| [pkg/server/core_store_adapter_test.rs:113](../../../pkg/server/core_store_adapter_test.rs#L113) | fn core_task_60_full_store_stats() { | 90/91/92 |
| [pkg/server/core_store_adapter_test.rs:357](../../../pkg/server/core_store_adapter_test.rs#L357) | fn core_task_60_nested_presence_order_and_unsigned_limits() { | 90/91/92 |
| [pkg/server/core_trace_test.rs:7](../../../pkg/server/core_trace_test.rs#L7) | fn fixture() -> serde_json::Value { | 90/91/92 |
| [pkg/server/core_trace_test.rs:11](../../../pkg/server/core_trace_test.rs#L11) | fn core_task_59_each_stage_and_clock_regression() { | 90/91/92 |
| [pkg/server/core_trace_test.rs:52](../../../pkg/server/core_trace_test.rs#L52) | fn core_task_59_save_cache_resets_and_recovery() { | 90/91/92 |
| [pkg/server/core_trace_test.rs:79](../../../pkg/server/core_trace_test.rs#L79) | fn core_task_59_registry_ownership() { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:22](../../../pkg/server/region_heartbeat_adapter_test.rs#L22) | fn is_bootstrapped(&self) -> bool { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:25](../../../pkg/server/region_heartbeat_adapter_test.rs#L25) | fn region_store_state(&self, id: u64) -> Option<NodeState> { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:32](../../../pkg/server/region_heartbeat_adapter_test.rs#L32) | async fn region_heartbeat(&self, region: RegionInfo) -> Result<(), RegionHeartbeatError> { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:36](../../../pkg/server/region_heartbeat_adapter_test.rs#L36) | fn drop(&mut self) { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:56](../../../pkg/server/region_heartbeat_adapter_test.rs#L56) | fn identity(&self) -> &crate::Identity { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:59](../../../pkg/server/region_heartbeat_adapter_test.rs#L59) | fn region_heartbeat_service(&self) -> Option<&dyn RegionHeartbeatService> { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:62](../../../pkg/server/region_heartbeat_adapter_test.rs#L62) | async fn healthy(&self) -> Result<(), String> { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:65](../../../pkg/server/region_heartbeat_adapter_test.rs#L65) | async fn alloc_id(&self) -> Result<u64, String> { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:68](../../../pkg/server/region_heartbeat_adapter_test.rs#L68) | async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:72](../../../pkg/server/region_heartbeat_adapter_test.rs#L72) | fn request() -> pdpb::RegionHeartbeatRequest { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:112](../../../pkg/server/region_heartbeat_adapter_test.rs#L112) | async fn setup() -> ( | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:138](../../../pkg/server/region_heartbeat_adapter_test.rs#L138) | let server = tokio::spawn(async move { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:148](../../../pkg/server/region_heartbeat_adapter_test.rs#L148) | async fn state_task_48_region_heartbeat_adapter_loopback() { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:207](../../../pkg/server/region_heartbeat_adapter_test.rs#L207) | async fn exchange( | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:225](../../../pkg/server/region_heartbeat_adapter_test.rs#L225) | async fn state_task_48_region_heartbeat_adapter_errors_and_close() { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:292](../../../pkg/server/region_heartbeat_adapter_test.rs#L292) | async fn state_task_48_region_heartbeat_adapter_defaults_and_epoch() { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:348](../../../pkg/server/region_heartbeat_adapter_test.rs#L348) | async fn state_task_48_region_heartbeat_adapter_cancel_inflight() { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:351](../../../pkg/server/region_heartbeat_adapter_test.rs#L351) | let call = tokio::spawn(async move { exchange(&mut client, vec![request()]).await }); | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:374](../../../pkg/server/region_heartbeat_adapter_test.rs#L374) | async fn state_task_48_region_heartbeat_adapter_disconnect() { | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs:377](../../../pkg/server/region_heartbeat_adapter_test.rs#L377) | let call = tokio::spawn(async move { exchange(&mut client, vec![request()]).await }); | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:13](../../../pkg/server/schedule_admin_balance_leader_test.rs#L13) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:50](../../../pkg/server/schedule_admin_balance_leader_test.rs#L50) | async fn task_137_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:96](../../../pkg/server/schedule_admin_balance_leader_test.rs#L96) | fn store(id: u64, count: i64) -> Arc<StoreInfo> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:106](../../../pkg/server/schedule_admin_balance_leader_test.rs#L106) | async fn task_137_updated_scheduling_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:339](../../../pkg/server/schedule_admin_balance_leader_test.rs#L339) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:345](../../../pkg/server/schedule_admin_balance_leader_test.rs#L345) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:355](../../../pkg/server/schedule_admin_balance_leader_test.rs#L355) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:363](../../../pkg/server/schedule_admin_balance_leader_test.rs#L363) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:371](../../../pkg/server/schedule_admin_balance_leader_test.rs#L371) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:378](../../../pkg/server/schedule_admin_balance_leader_test.rs#L378) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:381](../../../pkg/server/schedule_admin_balance_leader_test.rs#L381) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:384](../../../pkg/server/schedule_admin_balance_leader_test.rs#L384) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:387](../../../pkg/server/schedule_admin_balance_leader_test.rs#L387) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:390](../../../pkg/server/schedule_admin_balance_leader_test.rs#L390) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:396](../../../pkg/server/schedule_admin_balance_leader_test.rs#L396) | async fn task_137_persistence_failure() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs:470](../../../pkg/server/schedule_admin_balance_leader_test.rs#L470) | async fn task_137_body_read_failure() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:16](../../../pkg/server/schedule_admin_balance_range_test.rs#L16) | fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:23](../../../pkg/server/schedule_admin_balance_range_test.rs#L23) | async fn body(response: axum::response::Response) -> Vec<u8> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:30](../../../pkg/server/schedule_admin_balance_range_test.rs#L30) | async fn task_154_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:152](../../../pkg/server/schedule_admin_balance_range_test.rs#L152) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:158](../../../pkg/server/schedule_admin_balance_range_test.rs#L158) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:171](../../../pkg/server/schedule_admin_balance_range_test.rs#L171) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:179](../../../pkg/server/schedule_admin_balance_range_test.rs#L179) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:187](../../../pkg/server/schedule_admin_balance_range_test.rs#L187) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:194](../../../pkg/server/schedule_admin_balance_range_test.rs#L194) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:197](../../../pkg/server/schedule_admin_balance_range_test.rs#L197) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:200](../../../pkg/server/schedule_admin_balance_range_test.rs#L200) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:203](../../../pkg/server/schedule_admin_balance_range_test.rs#L203) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:206](../../../pkg/server/schedule_admin_balance_range_test.rs#L206) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:212](../../../pkg/server/schedule_admin_balance_range_test.rs#L212) | async fn task_154_job_lifecycle_and_list() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:282](../../../pkg/server/schedule_admin_balance_range_test.rs#L282) | async fn task_154_persistence_and_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs:408](../../../pkg/server/schedule_admin_balance_range_test.rs#L408) | async fn task_154_http_job_drives_real_schedule() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_region_test.rs:12](../../../pkg/server/schedule_admin_balance_region_test.rs#L12) | async fn task_138_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_region_test.rs:55](../../../pkg/server/schedule_admin_balance_region_test.rs#L55) | async fn task_138_body_io_failure() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_region_test.rs:89](../../../pkg/server/schedule_admin_balance_region_test.rs#L89) | async fn task_138_schedule_after_unsupported_update() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:13](../../../pkg/server/schedule_admin_balance_witness_test.rs#L13) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:50](../../../pkg/server/schedule_admin_balance_witness_test.rs#L50) | async fn task_139_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:114](../../../pkg/server/schedule_admin_balance_witness_test.rs#L114) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:120](../../../pkg/server/schedule_admin_balance_witness_test.rs#L120) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:130](../../../pkg/server/schedule_admin_balance_witness_test.rs#L130) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:138](../../../pkg/server/schedule_admin_balance_witness_test.rs#L138) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:146](../../../pkg/server/schedule_admin_balance_witness_test.rs#L146) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:153](../../../pkg/server/schedule_admin_balance_witness_test.rs#L153) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:156](../../../pkg/server/schedule_admin_balance_witness_test.rs#L156) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:159](../../../pkg/server/schedule_admin_balance_witness_test.rs#L159) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:162](../../../pkg/server/schedule_admin_balance_witness_test.rs#L162) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:165](../../../pkg/server/schedule_admin_balance_witness_test.rs#L165) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:171](../../../pkg/server/schedule_admin_balance_witness_test.rs#L171) | async fn task_139_persistence_failure() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:245](../../../pkg/server/schedule_admin_balance_witness_test.rs#L245) | async fn task_139_body_read_failure() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:278](../../../pkg/server/schedule_admin_balance_witness_test.rs#L278) | async fn task_139_updated_scheduling_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs:419](../../../pkg/server/schedule_admin_balance_witness_test.rs#L419) | async fn task_139_partial_decode_and_snapshot() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:13](../../../pkg/server/schedule_admin_evict_leader_test.rs#L13) | fn cluster() -> BasicCluster { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:25](../../../pkg/server/schedule_admin_evict_leader_test.rs#L25) | fn scheduler() -> EvictLeaderScheduler { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:28](../../../pkg/server/schedule_admin_evict_leader_test.rs#L28) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:64](../../../pkg/server/schedule_admin_evict_leader_test.rs#L64) | async fn task_140_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:139](../../../pkg/server/schedule_admin_evict_leader_test.rs#L139) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:145](../../../pkg/server/schedule_admin_evict_leader_test.rs#L145) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:155](../../../pkg/server/schedule_admin_evict_leader_test.rs#L155) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:163](../../../pkg/server/schedule_admin_evict_leader_test.rs#L163) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:171](../../../pkg/server/schedule_admin_evict_leader_test.rs#L171) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:178](../../../pkg/server/schedule_admin_evict_leader_test.rs#L178) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:181](../../../pkg/server/schedule_admin_evict_leader_test.rs#L181) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:184](../../../pkg/server/schedule_admin_evict_leader_test.rs#L184) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:187](../../../pkg/server/schedule_admin_evict_leader_test.rs#L187) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:190](../../../pkg/server/schedule_admin_evict_leader_test.rs#L190) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:196](../../../pkg/server/schedule_admin_evict_leader_test.rs#L196) | async fn task_140_transactions_and_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:321](../../../pkg/server/schedule_admin_evict_leader_test.rs#L321) | async fn task_140_updated_scheduling_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs:420](../../../pkg/server/schedule_admin_evict_leader_test.rs#L420) | async fn task_140_body_read_and_clone() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:15](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L15) | fn schedulers() -> (EvictDiskScheduler, EvictNetworkScheduler) { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:21](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L21) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:56](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L56) | async fn task_141_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:123](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L123) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:129](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L129) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:139](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L139) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:147](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L147) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:155](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L155) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:162](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L162) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:165](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L165) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:168](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L168) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:171](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L171) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:174](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L174) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:180](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L180) | async fn task_141_faults_and_body_errors() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:307](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L307) | fn cluster() -> astersql_pd_core::BasicCluster { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:318](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L318) | fn score(c: &astersql_pd_core::BasicCluster, id: u64, value: u64) { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:327](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L327) | async fn task_141_updated_scheduling() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:465](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L465) | fn network_score(c: &astersql_pd_core::BasicCluster, scores: &[(u64, u64)]) { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:474](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L474) | async fn task_141_network_updated_scheduling() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs:613](../../../pkg/server/schedule_admin_evict_slow_store_test.rs#L613) | async fn task_141_disabled_envelope() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:16](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L16) | fn scheduler() -> SlowTrendScheduler { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:19](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L19) | fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:27](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L27) | async fn task_153_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:112](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L112) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:118](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L118) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:128](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L128) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:136](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L136) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:144](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L144) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:151](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L151) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:154](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L154) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:157](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L157) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:160](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L160) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:163](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L163) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:169](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L169) | async fn task_153_persistence_and_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:289](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L289) | fn cluster() -> BasicCluster { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:307](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L307) | fn trend(c: &BasicCluster, id: u64, value: f64, cause: f64, result: f64) { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:320](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L320) | fn heartbeats(c: &BasicCluster, now: i64) { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:330](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L330) | async fn task_153_schedule_after_http_update() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs:543](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs#L543) | async fn task_153_captured_candidate_preserved() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:13](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L13) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:46](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L46) | async fn task_142_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:110](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L110) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:116](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L116) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:126](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L126) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:134](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L134) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:142](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L142) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:149](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L149) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:152](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L152) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:155](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L155) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:158](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L158) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:161](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L161) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:167](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L167) | async fn task_142_faults_and_body_errors() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:256](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L256) | async fn task_142_disabled_and_legacy_batch() { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:300](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L300) | fn cluster() -> BasicCluster { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:311](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L311) | fn stopping(c: &BasicCluster, id: u64, value: bool) { | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs:321](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs#L321) | async fn task_142_updated_scheduling() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:13](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L13) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:45](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L45) | async fn task_143_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:108](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L108) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:114](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L114) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:124](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L124) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:132](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L132) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:140](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L140) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:147](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L147) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:150](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L150) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:153](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L153) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:156](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L156) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:159](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L159) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:165](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L165) | async fn task_143_faults_and_body_errors() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs:263](../../../pkg/server/schedule_admin_grant_hot_region_test.rs#L263) | async fn task_143_updated_scheduling() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:13](../../../pkg/server/schedule_admin_grant_leader_test.rs#L13) | fn cluster() -> BasicCluster { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:25](../../../pkg/server/schedule_admin_grant_leader_test.rs#L25) | fn scheduler() -> GrantLeaderScheduler { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:28](../../../pkg/server/schedule_admin_grant_leader_test.rs#L28) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:75](../../../pkg/server/schedule_admin_grant_leader_test.rs#L75) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:81](../../../pkg/server/schedule_admin_grant_leader_test.rs#L81) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:91](../../../pkg/server/schedule_admin_grant_leader_test.rs#L91) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:99](../../../pkg/server/schedule_admin_grant_leader_test.rs#L99) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:107](../../../pkg/server/schedule_admin_grant_leader_test.rs#L107) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:114](../../../pkg/server/schedule_admin_grant_leader_test.rs#L114) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:117](../../../pkg/server/schedule_admin_grant_leader_test.rs#L117) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:120](../../../pkg/server/schedule_admin_grant_leader_test.rs#L120) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:123](../../../pkg/server/schedule_admin_grant_leader_test.rs#L123) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:126](../../../pkg/server/schedule_admin_grant_leader_test.rs#L126) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:132](../../../pkg/server/schedule_admin_grant_leader_test.rs#L132) | async fn task_145_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:196](../../../pkg/server/schedule_admin_grant_leader_test.rs#L196) | async fn task_145_transactions_and_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:336](../../../pkg/server/schedule_admin_grant_leader_test.rs#L336) | async fn task_145_updated_scheduling_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs:438](../../../pkg/server/schedule_admin_grant_leader_test.rs#L438) | async fn task_145_body_read_and_clone() { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:13](../../../pkg/server/schedule_admin_hot_region_test.rs#L13) | async fn call( | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:44](../../../pkg/server/schedule_admin_hot_region_test.rs#L44) | fn normalized(mut v: Value) -> Value { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:61](../../../pkg/server/schedule_admin_hot_region_test.rs#L61) | fn fixture() -> Value { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:68](../../../pkg/server/schedule_admin_hot_region_test.rs#L68) | async fn task_144_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:177](../../../pkg/server/schedule_admin_hot_region_test.rs#L177) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:183](../../../pkg/server/schedule_admin_hot_region_test.rs#L183) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:193](../../../pkg/server/schedule_admin_hot_region_test.rs#L193) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:201](../../../pkg/server/schedule_admin_hot_region_test.rs#L201) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:209](../../../pkg/server/schedule_admin_hot_region_test.rs#L209) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:216](../../../pkg/server/schedule_admin_hot_region_test.rs#L216) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:219](../../../pkg/server/schedule_admin_hot_region_test.rs#L219) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:222](../../../pkg/server/schedule_admin_hot_region_test.rs#L222) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:225](../../../pkg/server/schedule_admin_hot_region_test.rs#L225) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:228](../../../pkg/server/schedule_admin_hot_region_test.rs#L228) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:234](../../../pkg/server/schedule_admin_hot_region_test.rs#L234) | async fn task_144_faults_and_body_errors() { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:312](../../../pkg/server/schedule_admin_hot_region_test.rs#L312) | fn detail(id: u64, loads: Loads) -> StoreLoadDetail { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:339](../../../pkg/server/schedule_admin_hot_region_test.rs#L339) | fn peer(store_id: u64, region_id: u64, loads: Loads) -> HotPeerStat { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:347](../../../pkg/server/schedule_admin_hot_region_test.rs#L347) | fn region(id: u64, stores: &[u64], leader: u64) -> RegionInfo { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:368](../../../pkg/server/schedule_admin_hot_region_test.rs#L368) | fn stores() -> StoresInfo { | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs:380](../../../pkg/server/schedule_admin_hot_region_test.rs#L380) | async fn task_144_updated_scheduling() { | 90/91/92 |
| [pkg/server/schedule_admin_label_test.rs:12](../../../pkg/server/schedule_admin_label_test.rs#L12) | async fn assert_response(response: Response) { | 90/91/92 |
| [pkg/server/schedule_admin_label_test.rs:30](../../../pkg/server/schedule_admin_label_test.rs#L30) | async fn task_146_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_label_test.rs:80](../../../pkg/server/schedule_admin_label_test.rs#L80) | async fn task_146_unread_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_label_test.rs:105](../../../pkg/server/schedule_admin_label_test.rs#L105) | async fn task_146_schedule_after_unsupported_update() { | 90/91/92 |
| [pkg/server/schedule_admin_random_merge_test.rs:12](../../../pkg/server/schedule_admin_random_merge_test.rs#L12) | async fn assert_response(response: Response) { | 90/91/92 |
| [pkg/server/schedule_admin_random_merge_test.rs:30](../../../pkg/server/schedule_admin_random_merge_test.rs#L30) | async fn task_147_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_random_merge_test.rs:80](../../../pkg/server/schedule_admin_random_merge_test.rs#L80) | async fn task_147_unread_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_random_merge_test.rs:105](../../../pkg/server/schedule_admin_random_merge_test.rs#L105) | async fn task_147_schedule_after_unsupported_update() { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:16](../../../pkg/server/schedule_admin_scatter_range_test.rs#L16) | fn scheduler() -> ScatterRangeScheduler { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:19](../../../pkg/server/schedule_admin_scatter_range_test.rs#L19) | fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:27](../../../pkg/server/schedule_admin_scatter_range_test.rs#L27) | async fn task_148_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:112](../../../pkg/server/schedule_admin_scatter_range_test.rs#L112) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:118](../../../pkg/server/schedule_admin_scatter_range_test.rs#L118) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:128](../../../pkg/server/schedule_admin_scatter_range_test.rs#L128) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:136](../../../pkg/server/schedule_admin_scatter_range_test.rs#L136) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:144](../../../pkg/server/schedule_admin_scatter_range_test.rs#L144) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:151](../../../pkg/server/schedule_admin_scatter_range_test.rs#L151) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:154](../../../pkg/server/schedule_admin_scatter_range_test.rs#L154) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:157](../../../pkg/server/schedule_admin_scatter_range_test.rs#L157) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:160](../../../pkg/server/schedule_admin_scatter_range_test.rs#L160) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:163](../../../pkg/server/schedule_admin_scatter_range_test.rs#L163) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:168](../../../pkg/server/schedule_admin_scatter_range_test.rs#L168) | async fn task_148_persistence_and_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs:285](../../../pkg/server/schedule_admin_scatter_range_test.rs#L285) | async fn task_148_schedule_after_http_update() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:16](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L16) | fn scheduler() -> ShuffleHotScheduler { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:19](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L19) | fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:27](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L27) | async fn task_149_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:112](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L112) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:118](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L118) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:128](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L128) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:136](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L136) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:144](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L144) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:151](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L151) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:154](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L154) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:157](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L157) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:160](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L160) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:163](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L163) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:169](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L169) | async fn task_149_persistence_and_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs:275](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs#L275) | async fn task_149_schedule_after_http_update() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:11](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L11) | async fn assert_response(response: Response) { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:29](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L29) | async fn task_150_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:79](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L79) | async fn task_150_unread_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:120](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L120) | async fn task_150_schedule_after_unsupported_update() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:379](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L379) | async fn task_150_persistence_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:449](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L449) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:455](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L455) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:465](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L465) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:473](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L473) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:481](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L481) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:488](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L488) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:491](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L491) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:494](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L494) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:497](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L497) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs:500](../../../pkg/server/schedule_admin_shuffle_leader_test.rs#L500) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:14](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L14) | async fn task_151_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:127](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L127) | async fn task_151_schedule_after_update() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:570](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L570) | fn snapshot(state: &[Peer], leader: u64) -> RegionInfo { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:595](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L595) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:601](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L601) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:611](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L611) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:619](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L619) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:627](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L627) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:634](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L634) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:637](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L637) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:640](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L640) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:643](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L643) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:646](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L646) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:652](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L652) | async fn task_151_persistence_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs:727](../../../pkg/server/schedule_admin_shuffle_region_test.rs#L727) | async fn task_151_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:23](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L23) | async fn assert_response(response: Response) { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:41](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L41) | async fn task_152_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:112](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L112) | async fn task_152_unread_body_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:136](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L136) | fn region(case: &serde_json::Value) -> Arc<RegionInfo> { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:178](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L178) | async fn cluster(case: &serde_json::Value) -> BasicCluster { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:214](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L214) | async fn task_152_schedule_after_unsupported_update() { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:357](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L357) | async fn task_152_persistence_faults() { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:439](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L439) | async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:445](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L445) | async fn transact( | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:455](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L455) | async fn transact_branches( | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:463](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L463) | async fn transact_raw( | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:471](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L471) | async fn watch( | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:478](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L478) | async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:481](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L481) | async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:484](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L484) | async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:487](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L487) | async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> { | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs:490](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs#L490) | async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:12](../../../pkg/server/schedule_wire_test.rs#L12) | fn number(v: &Value, k: &str) -> u64 { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:15](../../../pkg/server/schedule_wire_test.rs#L15) | fn raw(v: &Value, k: &str) -> i32 { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:18](../../../pkg/server/schedule_wire_test.rs#L18) | fn flag(v: &Value, k: &str) -> bool { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:21](../../../pkg/server/schedule_wire_test.rs#L21) | fn bytes(v: &Value, k: &str) -> Vec<u8> { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:24](../../../pkg/server/schedule_wire_test.rs#L24) | fn optional<T>(v: &Value, k: &str, parse: fn(&Value) -> T) -> Option<T> { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:27](../../../pkg/server/schedule_wire_test.rs#L27) | fn list<T>(v: &Value, k: &str, parse: fn(&Value) -> T) -> Vec<T> { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:32](../../../pkg/server/schedule_wire_test.rs#L32) | fn peer(v: &Value) -> Peer { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:46](../../../pkg/server/schedule_wire_test.rs#L46) | fn epoch(v: &Value) -> RegionEpoch { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:52](../../../pkg/server/schedule_wire_test.rs#L52) | fn region(v: &Value) -> RegionMeta { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:69](../../../pkg/server/schedule_wire_test.rs#L69) | fn change(v: &Value) -> ChangePeer { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:75](../../../pkg/server/schedule_wire_test.rs#L75) | fn operation(v: &Value) -> Operation { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:108](../../../pkg/server/schedule_wire_test.rs#L108) | fn hex(bytes: &[u8]) -> String { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:112](../../../pkg/server/schedule_wire_test.rs#L112) | fn task_134_behavior_cases() { | 90/91/92 |
| [pkg/server/schedule_wire_test.rs:173](../../../pkg/server/schedule_wire_test.rs#L173) | fn task_134_headers_errors_and_decode_faults() { | 90/91/92 |
| [pkg/server/server_test.rs:11](../../../pkg/server/server_test.rs#L11) | async fn validation_and_open_stream_shutdown() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:6](../../../pkg/server/store_heartbeat_adapter_test.rs#L6) | fn state_task_47_store_heartbeat_adapter_state() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:43](../../../pkg/server/store_heartbeat_adapter_test.rs#L43) | fn new(hot: Option<HotStat>) -> Self { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:78](../../../pkg/server/store_heartbeat_adapter_test.rs#L78) | async fn store_heartbeat( | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:98](../../../pkg/server/store_heartbeat_adapter_test.rs#L98) | fn identity(&self) -> &crate::Identity { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:101](../../../pkg/server/store_heartbeat_adapter_test.rs#L101) | fn store_heartbeat_service(&self) -> Option<&dyn StoreHeartbeatService> { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:104](../../../pkg/server/store_heartbeat_adapter_test.rs#L104) | async fn healthy(&self) -> Result<(), String> { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:107](../../../pkg/server/store_heartbeat_adapter_test.rs#L107) | async fn alloc_id(&self) -> Result<u64, String> { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:110](../../../pkg/server/store_heartbeat_adapter_test.rs#L110) | async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:114](../../../pkg/server/store_heartbeat_adapter_test.rs#L114) | fn request(stats: Option<pdpb::StoreStats>) -> pdpb::StoreHeartbeatRequest { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:127](../../../pkg/server/store_heartbeat_adapter_test.rs#L127) | fn grpc(host: Arc<Host>) -> crate::grpc::Grpc { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:134](../../../pkg/server/store_heartbeat_adapter_test.rs#L134) | async fn state_task_47_store_heartbeat_adapter_wire_errors() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:187](../../../pkg/server/store_heartbeat_adapter_test.rs#L187) | async fn state_task_47_store_heartbeat_adapter_defaults() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:216](../../../pkg/server/store_heartbeat_adapter_test.rs#L216) | async fn state_task_47_store_heartbeat_adapter_opt_in_and_bootstrap() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:241](../../../pkg/server/store_heartbeat_adapter_test.rs#L241) | async fn state_task_47_store_heartbeat_adapter_multi_peer_network() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:280](../../../pkg/server/store_heartbeat_adapter_test.rs#L280) | let server = tokio::spawn(async move { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:385](../../../pkg/server/store_heartbeat_adapter_test.rs#L385) | async fn state_task_47_store_heartbeat_adapter_response_state_and_errors() { | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs:424](../../../pkg/server/store_heartbeat_adapter_test.rs#L424) | async fn state_task_47_store_heartbeat_adapter_presence_and_controller_boundary() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:5](../../../pkg/server/sync_codec_test.rs#L5) | fn fixtures() -> Vec<Vec<u8>> { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:20](../../../pkg/server/sync_codec_test.rs#L20) | fn state_task_50_sync_codec_go_wire() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:42](../../../pkg/server/sync_codec_test.rs#L42) | fn roundtrip(p: pdpb::SyncRegionResponse) -> pdpb::SyncRegionResponse { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:50](../../../pkg/server/sync_codec_test.rs#L50) | fn state_task_50_sync_codec_request_presence_and_all_member_fields() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:87](../../../pkg/server/sync_codec_test.rs#L87) | fn state_task_50_sync_codec_lossless_pinned_response_fields() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:168](../../../pkg/server/sync_codec_test.rs#L168) | fn state_task_50_sync_codec_send_matches_go_raw_wire() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:222](../../../pkg/server/sync_codec_test.rs#L222) | fn region_from(p: metapb::Region) -> RegionMeta { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:225](../../../pkg/server/sync_codec_test.rs#L225) | fn follower() -> ( | 90/91/92 |
| [pkg/server/sync_codec_test.rs:241](../../../pkg/server/sync_codec_test.rs#L241) | async fn state_task_50_sync_codec_mismatch_go_application() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:325](../../../pkg/server/sync_codec_test.rs#L325) | async fn state_task_50_sync_codec_error_header_and_keepalive() { | 90/91/92 |
| [pkg/server/sync_codec_test.rs:379](../../../pkg/server/sync_codec_test.rs#L379) | async fn state_task_50_sync_codec_snapshot_offsets_and_incremental() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:21](../../../pkg/server/sync_transport_test.rs#L21) | fn identity(&self) -> &crate::Identity { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:24](../../../pkg/server/sync_transport_test.rs#L24) | fn region_sync_service(&self) -> Option<&RegionSyncer> { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:27](../../../pkg/server/sync_transport_test.rs#L27) | async fn healthy(&self) -> Result<(), String> { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:30](../../../pkg/server/sync_transport_test.rs#L30) | async fn alloc_id(&self) -> Result<u64, String> { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:33](../../../pkg/server/sync_transport_test.rs#L33) | async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:37](../../../pkg/server/sync_transport_test.rs#L37) | fn region(version: u64) -> Arc<RegionInfo> { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:53](../../../pkg/server/sync_transport_test.rs#L53) | async fn setup() -> ( | 90/91/92 |
| [pkg/server/sync_transport_test.rs:80](../../../pkg/server/sync_transport_test.rs#L80) | let task = tokio::spawn(async move { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:89](../../../pkg/server/sync_transport_test.rs#L89) | async fn subscribe( | 90/91/92 |
| [pkg/server/sync_transport_test.rs:118](../../../pkg/server/sync_transport_test.rs#L118) | async fn next(stream: &mut tonic::Streaming<pdpb::SyncRegionResponse>) -> pdpb::SyncRegionResponse { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:125](../../../pkg/server/sync_transport_test.rs#L125) | async fn shutdown(host: &Host, cancel: CancellationToken, server: JoinHandle<()>) { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:135](../../../pkg/server/sync_transport_test.rs#L135) | async fn state_task_51_sync_transport_full_and_incremental() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:159](../../../pkg/server/sync_transport_test.rs#L159) | fn follower_state() -> SyncFollowerState { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:168](../../../pkg/server/sync_transport_test.rs#L168) | async fn apply(state: &mut SyncFollowerState, response: pdpb::SyncRegionResponse) { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:189](../../../pkg/server/sync_transport_test.rs#L189) | async fn names(host: &Host, expected: &[&str]) { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:202](../../../pkg/server/sync_transport_test.rs#L202) | async fn state_task_51_sync_transport_independent_slow_reader_and_final_state() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:245](../../../pkg/server/sync_transport_test.rs#L245) | async fn state_task_51_sync_transport_replacement_and_reconnect() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:284](../../../pkg/server/sync_transport_test.rs#L284) | async fn state_task_51_sync_transport_send_failure_cancellation_and_join() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:344](../../../pkg/server/sync_transport_test.rs#L344) | async fn state_task_51_sync_transport_follower_leader_reconnect() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:411](../../../pkg/server/sync_transport_test.rs#L411) | async fn state_task_51_sync_transport_snapshot_batches_and_publish_during_scan() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:475](../../../pkg/server/sync_transport_test.rs#L475) | async fn state_task_51_sync_transport_stop_before_first_request() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:478](../../../pkg/server/sync_transport_test.rs#L478) | let pending = tokio::spawn(async move { client.sync_regions(ReceiverStream::new(rx)).await }); | 90/91/92 |
| [pkg/server/sync_transport_test.rs:494](../../../pkg/server/sync_transport_test.rs#L494) | async fn state_task_51_sync_transport_etcd_follower_persistence() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:523](../../../pkg/server/sync_transport_test.rs#L523) | async fn sync_regions( | 90/91/92 |
| [pkg/server/sync_transport_test.rs:571](../../../pkg/server/sync_transport_test.rs#L571) | async fn state_task_51_sync_transport_follower_error_and_eof() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:580](../../../pkg/server/sync_transport_test.rs#L580) | let server = tokio::spawn(async move { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:618](../../../pkg/server/sync_transport_test.rs#L618) | async fn state_task_52_state_pipeline_real_region_syncer() { | 90/91/92 |
| [pkg/server/sync_transport_test.rs:669](../../../pkg/server/sync_transport_test.rs#L669) | let server = tokio::spawn(async move { | 90/91/92 |
| [pkg/utils/apiutil/apiutil_test.go:29](../../../pkg/utils/apiutil/apiutil_test.go#L29) | func TestMain(m *testing.M) { | 90/91/92 |
| [pkg/utils/apiutil/apiutil_test.go:33](../../../pkg/utils/apiutil/apiutil_test.go#L33) | func TestJsonRespondErrorOk(t *testing.T) { | 90/91/92 |
| [pkg/utils/apiutil/apiutil_test.go:51](../../../pkg/utils/apiutil/apiutil_test.go#L51) | func TestJsonRespondErrorBadInput(t *testing.T) { | 90/91/92 |
| [pkg/utils/apiutil/apiutil_test.go:76](../../../pkg/utils/apiutil/apiutil_test.go#L76) | func TestGetIPPortFromHTTPRequest(t *testing.T) { | 90/91/92 |
| [pkg/utils/apiutil/apiutil_test.go:213](../../../pkg/utils/apiutil/apiutil_test.go#L213) | func TestParseHexKeys(t *testing.T) { | 90/91/92 |
| [pkg/utils/apiutil/serverapi/middleware_test.go:30](../../../pkg/utils/apiutil/serverapi/middleware_test.go#L30) | func TestMain(m *testing.M) { | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware_test.go:36](../../../pkg/utils/apiutil/serverapi/middleware_test.go#L36) | func TestMatchMicroserviceRedirectNoPrimary(t *testing.T) { | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware_test.go:57](../../../pkg/utils/apiutil/serverapi/middleware_test.go#L57) | func TestMatchMicroserviceRedirectForbiddenHeader(t *testing.T) { | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware_test.go:80](../../../pkg/utils/apiutil/serverapi/middleware_test.go#L80) | func TestMatchMicroserviceRedirectKeyspaceDisabled(t *testing.T) { | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware_test.go:102](../../../pkg/utils/apiutil/serverapi/middleware_test.go#L102) | func TestMatchMicroserviceRedirectSchedulingNotIndependent(t *testing.T) { | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware_test.go:124](../../../pkg/utils/apiutil/serverapi/middleware_test.go#L124) | func TestMatchMicroserviceRedirectFilterBlocks(t *testing.T) { | 50/79/80/81 |
| [pkg/utils/configutil/config_adjust_test.rs:7](../../../pkg/utils/configutil/config_adjust_test.rs#L7) | fn config_adjust_string() { | 90/91/92 |
| [pkg/utils/configutil/config_adjust_test.rs:21](../../../pkg/utils/configutil/config_adjust_test.rs#L21) | fn config_adjust_integers() { | 90/91/92 |
| [pkg/utils/configutil/config_adjust_test.rs:60](../../../pkg/utils/configutil/config_adjust_test.rs#L60) | fn config_adjust_float64() { | 90/91/92 |
| [pkg/utils/configutil/config_adjust_test.rs:81](../../../pkg/utils/configutil/config_adjust_test.rs#L81) | fn config_adjust_duration() { | 90/91/92 |
| [pkg/utils/configutil/config_adjust_test.rs:92](../../../pkg/utils/configutil/config_adjust_test.rs#L92) | fn config_adjust_bool() { | 90/91/92 |
| [pkg/utils/configutil/config_metadata_test.rs:6](../../../pkg/utils/configutil/config_metadata_test.rs#L6) | fn config_metadata_explicit_zero_values_and_missing() { | 90/91/92 |
| [pkg/utils/configutil/config_metadata_test.rs:33](../../../pkg/utils/configutil/config_metadata_test.rs#L33) | fn config_metadata_nested_prefixes_and_missing_paths() { | 90/91/92 |
| [pkg/utils/configutil/config_metadata_test.rs:56](../../../pkg/utils/configutil/config_metadata_test.rs#L56) | fn config_metadata_child_does_not_mutate_parent_or_siblings() { | 90/91/92 |
| [pkg/utils/configutil/config_metadata_test.rs:82](../../../pkg/utils/configutil/config_metadata_test.rs#L82) | fn config_metadata_literal_segments_case_and_empty_keys() { | 90/91/92 |
| [pkg/utils/configutil/config_metadata_test.rs:103](../../../pkg/utils/configutil/config_metadata_test.rs#L103) | fn config_metadata_absent_and_empty_metadata() { | 90/91/92 |
| [pkg/utils/configutil/config_unknown_test.rs:6](../../../pkg/utils/configutil/config_unknown_test.rs#L6) | fn config_unknown_absent_and_empty_metadata() { | 90/91/92 |
| [pkg/utils/configutil/config_unknown_test.rs:19](../../../pkg/utils/configutil/config_unknown_test.rs#L19) | fn config_unknown_nested_paths_and_document_order() { | 90/91/92 |
| [pkg/utils/configutil/config_unknown_test.rs:35](../../../pkg/utils/configutil/config_unknown_test.rs#L35) | fn config_unknown_preserves_parser_display_verbatim() { | 90/91/92 |
| [pkg/utils/configutil/config_unknown_test.rs:48](../../../pkg/utils/configutil/config_unknown_test.rs#L48) | fn config_unknown_child_uses_global_metadata() { | 90/91/92 |
| [pkg/utils/configutil/config_unknown_test.rs:68](../../../pkg/utils/configutil/config_unknown_test.rs#L68) | fn config_unknown_warning_leaves_values_and_views_usable() { | 90/91/92 |
| [pkg/utils/configutil/configutil_test.go:24](../../../pkg/utils/configutil/configutil_test.go#L24) | func TestMain(m *testing.M) { | 90/91/92 |
| [pkg/utils/configutil/configutil_test.go:28](../../../pkg/utils/configutil/configutil_test.go#L28) | func TestPrintConfigCheckMsg(t *testing.T) { | 90/91/92 |
| [pkg/utils/grpcutil/cluster_test.go:36](../../../pkg/utils/grpcutil/cluster_test.go#L36) | func TestFollowerForwardAndHandleTestSuite(t *testing.T) { | 90/91/92 |
| [pkg/utils/grpcutil/config_test.rs:4](../../../pkg/utils/grpcutil/config_test.rs#L4) | fn tls_bytes_null_empty_and_base64() { | 90/91/92 |
| [pkg/utils/grpcutil/grpcutil_test.go:36](../../../pkg/utils/grpcutil/grpcutil_test.go#L36) | func TestMain(m *testing.M) { | 90/91/92 |
| [pkg/utils/grpcutil/grpcutil_test.go:75](../../../pkg/utils/grpcutil/grpcutil_test.go#L75) | func TestToClientTLSConfig(t *testing.T) { | 90/91/92 |
| [pkg/utils/grpcutil/grpcutil_test.go:116](../../../pkg/utils/grpcutil/grpcutil_test.go#L116) | func TestToServerTLSConfig(t *testing.T) { | 90/91/92 |
| [pkg/utils/metricutil/config_test.rs:4](../../../pkg/utils/metricutil/config_test.rs#L4) | fn metric_invalid_interval_and_job_defaults() { | 15/86/101 |
| [pkg/utils/metricutil/metricutil_test.go:28](../../../pkg/utils/metricutil/metricutil_test.go#L28) | func TestMain(m *testing.M) { | 15/86/101 |
| [pkg/utils/metricutil/metricutil_test.go:32](../../../pkg/utils/metricutil/metricutil_test.go#L32) | func TestCamelCaseToSnakeCase(t *testing.T) { | 15/86/101 |
| [pkg/utils/metricutil/metricutil_test.go:65](../../../pkg/utils/metricutil/metricutil_test.go#L65) | func TestCoverage(_ *testing.T) { | 15/86/101 |
| [pkg/utils/tsoutil/tso_dispatcher_test.go:123](../../../pkg/utils/tsoutil/tso_dispatcher_test.go#L123) | func TestMain(m *testing.M) { | 90/91/92 |
| [pkg/utils/tsoutil/tso_dispatcher_test.go:127](../../../pkg/utils/tsoutil/tso_dispatcher_test.go#L127) | func TestTSODispatcherTestSuite(t *testing.T) { | 90/91/92 |
| [server/api/middleware.go:72](../../../server/api/middleware.go#L72) | failpoint.Inject("addRequestInfoMiddleware", func() { | 50/79/80/81 |
| [server/api/router.go:393](../../../server/api/router.go#L393) | new(failpoint.HttpHandler).ServeHTTP(w, r) | 90/91/92 |
| [server/config/config_decode_test.rs:6](../../../server/config/config_decode_test.rs#L6) | fn config_decode_presence_before_defaults() { | 47/90 |
| [server/config/config_decode_test.rs:32](../../../server/config/config_decode_test.rs#L32) | fn config_decode_format_specific_names() { | 47/90 |
| [server/config/config_decode_test.rs:48](../../../server/config/config_decode_test.rs#L48) | fn config_decode_go_json_bytes() { | 47/90 |
| [server/config/config_decode_test.rs:86](../../../server/config/config_decode_test.rs#L86) | fn config_decode_type_errors_and_unknown_fields() { | 47/90 |
| [server/config/config_decode_test.rs:113](../../../server/config/config_decode_test.rs#L113) | fn config_decode_go_toml_input_to_json() { | 47/90 |
| [server/config/config_defaults_test.rs:7](../../../server/config/config_defaults_test.rs#L7) | fn adjust(config: &mut ConfigScalars, meta: &ConfigMetaData) { | 47/90 |
| [server/config/config_defaults_test.rs:18](../../../server/config/config_defaults_test.rs#L18) | fn config_defaults_missing_values() { | 47/90 |
| [server/config/config_defaults_test.rs:47](../../../server/config/config_defaults_test.rs#L47) | fn config_defaults_presence_zero_and_false() { | 47/90 |
| [server/config/config_defaults_test.rs:63](../../../server/config/config_defaults_test.rs#L63) | fn config_defaults_explicit_values_preserved() { | 47/90 |
| [server/config/config_defaults_test.rs:105](../../../server/config/config_defaults_test.rs#L105) | fn config_defaults_absent_metadata_overwrites_sizes() { | 47/90 |
| [server/config/config_defaults_test.rs:121](../../../server/config/config_defaults_test.rs#L121) | fn config_defaults_duration_boundaries() { | 47/90 |
| [server/config/config_defaults_test.rs:143](../../../server/config/config_defaults_test.rs#L143) | fn config_defaults_inherits_addresses_and_preserves_other_owners() { | 47/90 |
| [server/config/config_defaults_test.rs:163](../../../server/config/config_defaults_test.rs#L163) | fn config_defaults_decoded_durations_and_labels() { | 47/90 |
| [server/config/config_hot_update_test.rs:25](../../../server/config/config_hot_update_test.rs#L25) | fn new(inner: Arc<dyn MetaStore>) -> Arc<Self> { | 47/90 |
| [server/config/config_hot_update_test.rs:37](../../../server/config/config_hot_update_test.rs#L37) | async fn read(&self, r: Read) -> Result<Snapshot> { | 47/90 |
| [server/config/config_hot_update_test.rs:40](../../../server/config/config_hot_update_test.rs#L40) | async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> { | 47/90 |
| [server/config/config_hot_update_test.rs:50](../../../server/config/config_hot_update_test.rs#L50) | async fn transact_branches( | 47/90 |
| [server/config/config_hot_update_test.rs:58](../../../server/config/config_hot_update_test.rs#L58) | async fn transact_raw( | 47/90 |
| [server/config/config_hot_update_test.rs:66](../../../server/config/config_hot_update_test.rs#L66) | async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> { | 47/90 |
| [server/config/config_hot_update_test.rs:69](../../../server/config/config_hot_update_test.rs#L69) | async fn grant(&self, t: i64) -> Result<Lease> { | 47/90 |
| [server/config/config_hot_update_test.rs:72](../../../server/config/config_hot_update_test.rs#L72) | async fn keep_alive(&self, l: LeaseId) -> Result<Lease> { | 47/90 |
| [server/config/config_hot_update_test.rs:75](../../../server/config/config_hot_update_test.rs#L75) | async fn revoke(&self, l: LeaseId) -> Result<()> { | 47/90 |
| [server/config/config_hot_update_test.rs:78](../../../server/config/config_hot_update_test.rs#L78) | async fn ttl(&self, l: LeaseId) -> Result<i64> { | 47/90 |
| [server/config/config_hot_update_test.rs:81](../../../server/config/config_hot_update_test.rs#L81) | async fn compact(&self, r: Revision) -> Result<()> { | 47/90 |
| [server/config/config_hot_update_test.rs:85](../../../server/config/config_hot_update_test.rs#L85) | fn host(store: Arc<dyn MetaStore>) -> Arc<HotConfig> { | 47/90 |
| [server/config/config_hot_update_test.rs:92](../../../server/config/config_hot_update_test.rs#L92) | async fn stored(store: Arc<dyn MetaStore>) -> PersistedConfig { | 47/90 |
| [server/config/config_hot_update_test.rs:99](../../../server/config/config_hot_update_test.rs#L99) | async fn contract(store: Arc<dyn MetaStore>) { | 47/90 |
| [server/config/config_hot_update_test.rs:124](../../../server/config/config_hot_update_test.rs#L124) | let pending = tokio::spawn(async move { | 47/90 |
| [server/config/config_hot_update_test.rs:160](../../../server/config/config_hot_update_test.rs#L160) | async fn config_hot_update_memory_publication_and_rollback() { | 47/90 |
| [server/config/config_hot_update_test.rs:164](../../../server/config/config_hot_update_test.rs#L164) | async fn config_hot_update_etcd_publication_and_rollback() { | 47/90 |
| [server/config/config_hot_update_test.rs:169](../../../server/config/config_hot_update_test.rs#L169) | async fn concurrent_contract(store: Arc<dyn MetaStore>) { | 47/90 |
| [server/config/config_hot_update_test.rs:174](../../../server/config/config_hot_update_test.rs#L174) | tasks.push(tokio::spawn(async move { | 47/90 |
| [server/config/config_hot_update_test.rs:183](../../../server/config/config_hot_update_test.rs#L183) | tasks.push(tokio::spawn(async move { | 47/90 |
| [server/config/config_hot_update_test.rs:187](../../../server/config/config_hot_update_test.rs#L187) | tasks.push(tokio::spawn(async move { o.persist().await.unwrap() })); | 47/90 |
| [server/config/config_hot_update_test.rs:199](../../../server/config/config_hot_update_test.rs#L199) | async fn config_hot_update_memory_concurrent_read_modify_persist() { | 47/90 |
| [server/config/config_hot_update_test.rs:203](../../../server/config/config_hot_update_test.rs#L203) | async fn config_hot_update_etcd_concurrent_read_modify_persist() { | 47/90 |
| [server/config/config_hot_update_test.rs:209](../../../server/config/config_hot_update_test.rs#L209) | async fn config_hot_update_noop_error_and_clone_isolation() { | 47/90 |
| [server/config/config_hot_update_test.rs:236](../../../server/config/config_hot_update_test.rs#L236) | async fn config_hot_update_cancel_restores_before_next_writer() { | 47/90 |
| [server/config/config_hot_update_test.rs:244](../../../server/config/config_hot_update_test.rs#L244) | let pending = tokio::spawn(async move { | 47/90 |
| [server/config/config_hot_update_test.rs:267](../../../server/config/config_hot_update_test.rs#L267) | async fn config_hot_update_queued_writer_and_persist_wait_for_rollback() { | 47/90 |
| [server/config/config_hot_update_test.rs:275](../../../server/config/config_hot_update_test.rs#L275) | let first = tokio::spawn(async move { | 47/90 |
| [server/config/config_persist_test.rs:9](../../../server/config/config_persist_test.rs#L9) | fn context() -> ConfigDefaultsContext<'static> { | 47/90 |
| [server/config/config_persist_test.rs:15](../../../server/config/config_persist_test.rs#L15) | fn configured() -> PersistedConfig { | 47/90 |
| [server/config/config_persist_test.rs:28](../../../server/config/config_persist_test.rs#L28) | async fn reload_contract(store: Arc<dyn MetaStore>) { | 47/90 |
| [server/config/config_persist_test.rs:87](../../../server/config/config_persist_test.rs#L87) | async fn config_persist_memory_reload_upgrade() { | 47/90 |
| [server/config/config_persist_test.rs:95](../../../server/config/config_persist_test.rs#L95) | async fn roundtrip(store: Arc<dyn MetaStore>) { | 47/90 |
| [server/config/config_persist_test.rs:142](../../../server/config/config_persist_test.rs#L142) | async fn config_persist_memory_roundtrip() { | 47/90 |
| [server/config/config_persist_test.rs:146](../../../server/config/config_persist_test.rs#L146) | async fn config_persist_etcd_reload_and_roundtrip() { | 47/90 |
| [server/config/config_persist_test.rs:152](../../../server/config/config_persist_test.rs#L152) | async fn config_persist_etcd_restart_and_backend_errors() { | 47/90 |
| [server/config/config_persist_test.rs:190](../../../server/config/config_persist_test.rs#L190) | async fn config_persist_legacy_limits_null_and_flags() { | 47/90 |
| [server/config/config_persist_test.rs:242](../../../server/config/config_persist_test.rs#L242) | async fn config_persist_go_snapshot_differential() { | 47/90 |
| [server/config/config_persist_test.rs:281](../../../server/config/config_persist_test.rs#L281) | async fn config_persist_rejects_malformed_migration_fields() { | 47/90 |
| [server/config/config_test.go:41](../../../server/config/config_test.go#L41) | func TestMain(m *testing.M) { | 47/90 |
| [server/config/config_test.go:45](../../../server/config/config_test.go#L45) | func TestSecurity(t *testing.T) { | 82–85 |
| [server/config/config_test.go:51](../../../server/config/config_test.go#L51) | func TestTLS(t *testing.T) { | 82–85 |
| [server/config/config_test.go:59](../../../server/config/config_test.go#L59) | func TestBadFormatJoinAddr(t *testing.T) { | 47/90 |
| [server/config/config_test.go:68](../../../server/config/config_test.go#L68) | func TestJoinAddr(t *testing.T) { | 47/90 |
| [server/config/config_test.go:111](../../../server/config/config_test.go#L111) | func TestReloadConfig(t *testing.T) { | 47/90 |
| [server/config/config_test.go:131](../../../server/config/config_test.go#L131) | func TestReloadDefaultStoreLimit(t *testing.T) { | 47/90 |
| [server/config/config_test.go:185](../../../server/config/config_test.go#L185) | func TestDefaultStoreLimitAdjust(t *testing.T) { | 47/90 |
| [server/config/config_test.go:299](../../../server/config/config_test.go#L299) | func TestStoreLimitPartialJSONUpdates(t *testing.T) { | 47/90 |
| [server/config/config_test.go:379](../../../server/config/config_test.go#L379) | func TestReloadLegacyStoreBalanceRate(t *testing.T) { | 47/90 |
| [server/config/config_test.go:417](../../../server/config/config_test.go#L417) | func TestReloadUpgrade(t *testing.T) { | 47/90 |
| [server/config/config_test.go:440](../../../server/config/config_test.go#L440) | func TestReloadUpgrade2(t *testing.T) { | 47/90 |
| [server/config/config_test.go:461](../../../server/config/config_test.go#L461) | func TestValidation(t *testing.T) { | 47/90 |
| [server/config/config_test.go:527](../../../server/config/config_test.go#L527) | func TestAdjust(t *testing.T) { | 47/90 |
| [server/config/config_test.go:662](../../../server/config/config_test.go#L662) | func TestMigrateFlags(t *testing.T) { | 47/90 |
| [server/config/config_test.go:700](../../../server/config/config_test.go#L700) | func TestLegacyDisableRawKVRegionSplitConfigDoesNotPanic(t *testing.T) { | 47/90 |
| [server/config/config_test.go:717](../../../server/config/config_test.go#L717) | func TestPDServerConfig(t *testing.T) { | 47/90 |
| [server/config/config_test.go:783](../../../server/config/config_test.go#L783) | func TestGCConcurrencyConfig(t *testing.T) { | 47/90 |
| [server/config/config_test.go:810](../../../server/config/config_test.go#L810) | func TestGCConcurrencyConfigReload(t *testing.T) { | 47/90 |
| [server/config/config_test.go:835](../../../server/config/config_test.go#L835) | func TestGCConcurrencyConfigReloadRejectsInvalid(t *testing.T) { | 47/90 |
| [server/config/config_test.go:853](../../../server/config/config_test.go#L853) | func TestDashboardConfig(t *testing.T) { | 47/90 |
| [server/config/config_test.go:892](../../../server/config/config_test.go#L892) | func TestReplicationMode(t *testing.T) { | 49；主题8领域 |
| [server/config/config_test.go:927](../../../server/config/config_test.go#L927) | func TestHotHistoryRegionConfig(t *testing.T) { | 47/90 |
| [server/config/config_test.go:949](../../../server/config/config_test.go#L949) | func TestMaxStorePreparingTime(t *testing.T) { | 47/90 |
| [server/config/config_test.go:981](../../../server/config/config_test.go#L981) | func TestConfigClone(t *testing.T) { | 47/90 |
| [server/config/config_test.go:1019](../../../server/config/config_test.go#L1019) | func TestRateLimitClone(t *testing.T) { | 47/90 |
| [server/config/config_test.go:1044](../../../server/config/config_test.go#L1044) | func TestAdjustMetaServiceGroups(t *testing.T) { | 47/90 |
| [server/config/config_tso_test.rs:6](../../../server/config/config_tso_test.rs#L6) | fn config_tso_zero_and_negative_defaults() { | 47/90 |
| [server/config/config_tso_test.rs:30](../../../server/config/config_tso_test.rs#L30) | fn config_tso_physical_interval_clamps() { | 47/90 |
| [server/config/config_tso_test.rs:61](../../../server/config/config_tso_test.rs#L61) | fn config_tso_proxy_count_preserves_negative_unlimited() { | 47/90 |
| [server/config/config_tso_test.rs:79](../../../server/config/config_tso_test.rs#L79) | fn config_tso_positive_save_and_timeout_preserved() { | 47/90 |
| [server/config/config_tso_test.rs:100](../../../server/config/config_tso_test.rs#L100) | fn config_tso_decoded_values_normalize() { | 47/90 |
| [server/config/config_urls_test.rs:5](../../../server/config/config_urls_test.rs#L5) | fn config_urls_scheme_matches_go_table() { | 47/90 |
| [server/config/config_urls_test.rs:36](../../../server/config/config_urls_test.rs#L36) | fn config_urls_join_matches_go_cases() { | 47/90 |
| [server/config/config_urls_test.rs:61](../../../server/config/config_urls_test.rs#L61) | fn config_urls_list_preserves_parse_not_request_semantics() { | 47/90 |
| [server/config/config_urls_test.rs:98](../../../server/config/config_urls_test.rs#L98) | fn config_urls_request_and_join_boundaries() { | 47/90 |
| [server/config/dashboard_config_test.rs:7](../../../server/config/dashboard_config_test.rs#L7) | fn dashboard_config_presence_and_host_defaults() { | 47/90 |
| [server/config/dashboard_config_test.rs:36](../../../server/config/dashboard_config_test.rs#L36) | fn dashboard_config_custom_values_and_toml_roundtrip() { | 47/90 |
| [server/config/dashboard_config_test.rs:75](../../../server/config/dashboard_config_test.rs#L75) | fn dashboard_config_nested_metadata_and_missing_toml() { | 47/90 |
| [server/config/dashboard_config_test.rs:88](../../../server/config/dashboard_config_test.rs#L88) | fn dashboard_config_existing_go_json_bytes() { | 47/90 |
| [server/config/dashboard_config_test.rs:113](../../../server/config/dashboard_config_test.rs#L113) | fn dashboard_config_scalar_types_and_null() { | 47/90 |
| [server/config/deprecated_config_test.rs:8](../../../server/config/deprecated_config_test.rs#L8) | fn deprecated_config_log_mapping_and_warning_order() { | 47/90 |
| [server/config/deprecated_config_test.rs:77](../../../server/config/deprecated_config_test.rs#L77) | fn deprecated_config_learner_presence_including_false() { | 47/90 |
| [server/config/deprecated_config_test.rs:94](../../../server/config/deprecated_config_test.rs#L94) | fn deprecated_config_trace_presence_always_conflicts() { | 47/90 |
| [server/config/deprecated_config_test.rs:121](../../../server/config/deprecated_config_test.rs#L121) | fn deprecated_config_raw_kv_split_is_ignored() { | 47/90 |
| [server/config/deprecated_config_test.rs:144](../../../server/config/deprecated_config_test.rs#L144) | fn deprecated_config_existing_go_bytes_after_ignored_legacy_key() { | 47/90 |
| [server/config/keyspace_config_test.rs:9](../../../server/config/keyspace_config_test.rs#L9) | fn groups(entries: &[(&str, &str)]) -> BTreeMap<String, String> { | 47/90 |
| [server/config/keyspace_config_test.rs:17](../../../server/config/keyspace_config_test.rs#L17) | fn keyspace_config_defaults_and_presence() { | 47/90 |
| [server/config/keyspace_config_test.rs:61](../../../server/config/keyspace_config_test.rs#L61) | fn keyspace_config_split_validation_boundaries() { | 47/90 |
| [server/config/keyspace_config_test.rs:102](../../../server/config/keyspace_config_test.rs#L102) | fn keyspace_config_group_ids_and_atomic_errors() { | 47/90 |
| [server/config/keyspace_config_test.rs:136](../../../server/config/keyspace_config_test.rs#L136) | fn keyspace_config_group_mapping_and_adjust_failure() { | 47/90 |
| [server/config/keyspace_config_test.rs:173](../../../server/config/keyspace_config_test.rs#L173) | fn keyspace_config_clone_and_accessors() { | 47/90 |
| [server/config/keyspace_config_test.rs:203](../../../server/config/keyspace_config_test.rs#L203) | fn keyspace_config_existing_go_json_bytes() { | 47/90 |
| [server/config/microservice_config_test.rs:12](../../../server/config/microservice_config_test.rs#L12) | fn flags(config: &MicroserviceConfig) -> [bool; 3] { | 47/90 |
| [server/config/microservice_config_test.rs:21](../../../server/config/microservice_config_test.rs#L21) | fn microservice_config_presence_all_combinations() { | 47/90 |
| [server/config/microservice_config_test.rs:55](../../../server/config/microservice_config_test.rs#L55) | fn microservice_config_toml_presence_and_types() { | 47/90 |
| [server/config/microservice_config_test.rs:88](../../../server/config/microservice_config_test.rs#L88) | fn microservice_config_nested_metadata() { | 47/90 |
| [server/config/microservice_config_test.rs:103](../../../server/config/microservice_config_test.rs#L103) | fn microservice_config_clone_and_getters() { | 47/90 |
| [server/config/microservice_config_test.rs:127](../../../server/config/microservice_config_test.rs#L127) | fn microservice_config_existing_go_json_bytes() { | 47/90 |
| [server/config/microservice_config_test.rs:152](../../../server/config/microservice_config_test.rs#L152) | fn microservice_config_json_roundtrip_and_types() { | 47/90 |
| [server/config/middleware_config_test.rs:3](../../../server/config/middleware_config_test.rs#L3) | fn dimension() -> MiddlewareDimensionConfig { | 50/79/80/81 |
| [server/config/middleware_config_test.rs:11](../../../server/config/middleware_config_test.rs#L11) | fn middleware_config_constructor_go_bytes() { | 50/79/80/81 |
| [server/config/middleware_config_test.rs:32](../../../server/config/middleware_config_test.rs#L32) | fn middleware_config_zero_and_nil_clone() { | 50/79/80/81 |
| [server/config/middleware_config_test.rs:43](../../../server/config/middleware_config_test.rs#L43) | fn middleware_config_clone_map_ownership() { | 50/79/80/81 |
| [server/config/middleware_config_test.rs:93](../../../server/config/middleware_config_test.rs#L93) | fn middleware_config_dimension_and_decode() { | 50/79/80/81 |
| [server/config/middleware_config_test.rs:125](../../../server/config/middleware_config_test.rs#L125) | fn middleware_config_populated_go_bytes() { | 50/79/80/81 |
| [server/config/middleware_config_test.rs:195](../../../server/config/middleware_config_test.rs#L195) | fn middleware_config_qps_number_encoding() { | 50/79/80/81 |
| [server/config/nested_config_test.rs:6](../../../server/config/nested_config_test.rs#L6) | fn adjust(c: &mut Config, meta: &ConfigMetaData) -> Result<(), String> { | 47/90 |
| [server/config/nested_config_test.rs:18](../../../server/config/nested_config_test.rs#L18) | fn nested_config_defaults() { | 47/90 |
| [server/config/nested_config_test.rs:29](../../../server/config/nested_config_test.rs#L29) | fn nested_config_error_precedence() { | 47/90 |
| [server/config/nested_config_test.rs:41](../../../server/config/nested_config_test.rs#L41) | fn nested_config_go_captured_values_and_errors() { | 47/90 |
| [server/config/nested_config_test.rs:84](../../../server/config/nested_config_test.rs#L84) | fn nested_config_go_full_json_decode() { | 47/90 |
| [server/config/nested_config_test.rs:114](../../../server/config/nested_config_test.rs#L114) | fn nested_config_go_raw_nested_bytes() { | 47/90 |
| [server/config/nested_config_test.rs:150](../../../server/config/nested_config_test.rs#L150) | fn nested_config_order_reload_presence_and_no_side_effects() { | 47/90 |
| [server/config/nested_config_test.rs:196](../../../server/config/nested_config_test.rs#L196) | fn nested_config_json_toml_types_and_float_boundaries() { | 47/90 |
| [server/config/nested_config_test.rs:215](../../../server/config/nested_config_test.rs#L215) | fn nested_config_paths_semver_and_warnings() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:7](../../../server/config/pd_server_defaults_test.rs#L7) | fn all_defined() -> ConfigMetaData { | 47/90 |
| [server/config/pd_server_defaults_test.rs:13](../../../server/config/pd_server_defaults_test.rs#L13) | fn pd_server_defaults_go_bytes() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:45](../../../server/config/pd_server_defaults_test.rs#L45) | fn pd_server_defaults_explicit_zero() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:69](../../../server/config/pd_server_defaults_test.rs#L69) | fn pd_server_defaults_preserve_and_replace() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:110](../../../server/config/pd_server_defaults_test.rs#L110) | fn pd_server_defaults_bounds() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:156](../../../server/config/pd_server_defaults_test.rs#L156) | fn pd_server_defaults_string_encoding() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:175](../../../server/config/pd_server_defaults_test.rs#L175) | fn pd_server_defaults_toml_presence_and_types() { | 47/90 |
| [server/config/pd_server_defaults_test.rs:228](../../../server/config/pd_server_defaults_test.rs#L228) | fn pd_server_defaults_float_boundaries_and_nonfinite_encoding() { | 47/90 |
| [server/config/pd_server_gc_test.rs:9](../../../server/config/pd_server_gc_test.rs#L9) | fn defaults() -> PDServerConfig { | 47/90 |
| [server/config/pd_server_gc_test.rs:16](../../../server/config/pd_server_gc_test.rs#L16) | fn pd_server_gc_defaults_and_valid_combinations() { | 47/90 |
| [server/config/pd_server_gc_test.rs:36](../../../server/config/pd_server_gc_test.rs#L36) | fn pd_server_gc_explicit_boundaries_and_errors() { | 47/90 |
| [server/config/pd_server_gc_test.rs:59](../../../server/config/pd_server_gc_test.rs#L59) | fn pd_server_gc_first_error_and_no_implicit_adjustment() { | 47/90 |
| [server/config/pd_server_gc_test.rs:75](../../../server/config/pd_server_gc_test.rs#L75) | fn pd_server_gc_reload_candidate_values() { | 47/90 |
| [server/config/pd_server_validate_test.rs:7](../../../server/config/pd_server_validate_test.rs#L7) | fn valid() -> PDServerConfig { | 47/90 |
| [server/config/pd_server_validate_test.rs:13](../../../server/config/pd_server_validate_test.rs#L13) | fn error(config: &PDServerConfig) -> String { | 47/90 |
| [server/config/pd_server_validate_test.rs:18](../../../server/config/pd_server_validate_test.rs#L18) | fn pd_server_validate_dashboard() { | 47/90 |
| [server/config/pd_server_validate_test.rs:51](../../../server/config/pd_server_validate_test.rs#L51) | fn pd_server_validate_key_and_flow() { | 47/90 |
| [server/config/pd_server_validate_test.rs:84](../../../server/config/pd_server_validate_test.rs#L84) | fn pd_server_validate_float_boundaries() { | 47/90 |
| [server/config/pd_server_validate_test.rs:132](../../../server/config/pd_server_validate_test.rs#L132) | fn float_field(config: &mut PDServerConfig, index: usize) -> &mut f64 { | 47/90 |
| [server/config/pd_server_validate_test.rs:141](../../../server/config/pd_server_validate_test.rs#L141) | fn pd_server_validate_error_order() { | 47/90 |
| [server/config/pd_server_validate_test.rs:189](../../../server/config/pd_server_validate_test.rs#L189) | fn pd_server_validate_decode_adjust_and_unchecked_fields() { | 47/90 |
| [server/config/persist_options.go:880](../../../server/config/persist_options.go#L880) | failpoint.Inject("persistFail", func() { | 47/90 |
| [server/config/persist_options.go:881](../../../server/config/persist_options.go#L881) | failpoint.Return(errors.New("fail to persist")) | 47/90 |
| [server/config/replication_config_test.rs:8](../../../server/config/replication_config_test.rs#L8) | fn replication_config_normalize_and_preserve_mode() { | 49；主题8领域 |
| [server/config/replication_config_test.rs:46](../../../server/config/replication_config_test.rs#L46) | fn replication_config_defaults_and_presence() { | 49；主题8领域 |
| [server/config/replication_config_test.rs:87](../../../server/config/replication_config_test.rs#L87) | fn replication_config_toml_dr_values() { | 49；主题8领域 |
| [server/config/replication_config_test.rs:143](../../../server/config/replication_config_test.rs#L143) | fn replication_config_clone_independent() { | 49；主题8领域 |
| [server/config/replication_config_test.rs:164](../../../server/config/replication_config_test.rs#L164) | fn replication_config_existing_go_json_bytes() { | 49；主题8领域 |
| [server/config/replication_config_test.rs:189](../../../server/config/replication_config_test.rs#L189) | fn replication_config_codec_types_and_roundtrip() { | 49；主题8领域 |
| [server/config/service_middleware_persist_options.go:96](../../../server/config/service_middleware_persist_options.go#L96) | failpoint.Inject("persistServiceMiddlewareFail", func() { | 50/79/80/81 |
| [server/config/util_test.go:23](../../../server/config/util_test.go#L23) | func TestValidateURLWithScheme(t *testing.T) { | 47/90 |
| [server/grpc_service.go:105](../../../server/grpc_service.go#L105) | failpoint.Inject("tsoProxyFailToSendToClient", func() { | 90/91/92 |
| [server/grpc_service.go:107](../../../server/grpc_service.go#L107) | failpoint.Return() | 90/91/92 |
| [server/grpc_service.go:129](../../../server/grpc_service.go#L129) | failpoint.Inject("tsoProxyRecvFromClientTimeout", func(val failpoint.Value) { | 90/91/92 |
| [server/grpc_service.go:230](../../../server/grpc_service.go#L230) | failpoint.Inject("customTimeout", func() { | 90/91/92 |
| [server/grpc_service.go:724](../../../server/grpc_service.go#L724) | failpoint.Inject("handleAllocIDNonBatch", func() { | 90/91/92 |
| [server/grpc_service.go:1119](../../../server/grpc_service.go#L1119) | failpoint.Inject("grpcClientClosed", func() { | 90/91/92 |
| [server/grpc_service.go:1136](../../../server/grpc_service.go#L1136) | failpoint.Inject("grpcClientClosed", func() { | 90/91/92 |
| [server/grpc_service.go:1324](../../../server/grpc_service.go#L1324) | failpoint.Inject("grpcClientClosed", func() { | 90/91/92 |
| [server/grpc_service.go:1515](../../../server/grpc_service.go#L1515) | failpoint.Inject("rateLimit", func() { | 90/91/92 |
| [server/grpc_service.go:1516](../../../server/grpc_service.go#L1516) | failpoint.Return(nil, errs.ErrGRPCRateLimitExceeded(errs.ErrRateLimitExceeded)) | 90/91/92 |
| [server/grpc_service.go:1537](../../../server/grpc_service.go#L1537) | failpoint.Inject("delayProcess", nil) | 90/91/92 |
| [server/grpc_service.go:1653](../../../server/grpc_service.go#L1653) | failpoint.Inject("queryRegionMetError", func() { | 90/91/92 |
| [server/grpc_service.go:1654](../../../server/grpc_service.go#L1654) | failpoint.Return(errs.ErrNotBootstrapped.FastGenByArgs()) | 90/91/92 |
| [server/grpc_service.go:1659](../../../server/grpc_service.go#L1659) | failpoint.Inject("queryRegionFollowerCacheMiss", func() { | 90/91/92 |
| [server/server.go:474](../../../server/server.go#L474) | failpoint.Inject("memberNil", func() { | 90/91/92 |
| [server/server.go:601](../../../server/server.go#L601) | failpoint.InjectCall("delayStartServer") | 90/91/92 |
| [server/server.go:709](../../../server/server.go#L709) | failpoint.Inject("delayStartServerLoop", func() { | 90/91/92 |
| [server/server.go:882](../../../server/server.go#L882) | failpoint.Inject("raftclusterIsBusy", func() {}) | 90/91/92 |
| [server/server.go:1932](../../../server/server.go#L1932) | func memberFailpointEnabled(val failpoint.Value, memberID uint64) bool { | 90/91/92 |
| [server/server.go:1961](../../../server/server.go#L1961) | failpoint.Inject("leaderLoopCheckAgain", func(val failpoint.Value) { | 90/91/92 |
| [server/server.go:1998](../../../server/server.go#L1998) | failpoint.Inject("timeoutWaitPDLeader", func(val failpoint.Value) { | 90/91/92 |
| [server/server.go:2128](../../../server/server.go#L2128) | failpoint.Inject("rebaseErr", func(val failpoint.Value) { | 90/91/92 |
| [server/server.go:2130](../../../server/server.go#L2130) | failpoint.Return() | 90/91/92 |
| [server/server.go:2149](../../../server/server.go#L2149) | failpoint.Inject("blockReadyToServe", func(val failpoint.Value) { | 90/91/92 |
| [server/server.go:2183](../../../server/server.go#L2183) | failpoint.Inject("exitCampaignLeader", func(val failpoint.Value) { | 90/91/92 |
| [server/server.go:2186](../../../server/server.go#L2186) | failpoint.Return() | 90/91/92 |

## 来源集合 SHA-256

集合按仓库相对路径去重并排序；哈希原始字节。集合摘要为以下 `(path,sha256)` 数组的 Python `json.dumps`（默认空格、ensure_ascii=True）UTF-8 SHA-256。包括相邻测试、调用方、注册处及只读边界文件；不含生成本文。

文件数：267；集合 SHA-256：`ce6e64c8cf56aced73e1bb88d6c4eaf14796cfa411d813d9ede578ace3e4843e`。

| 来源 | SHA-256 | owner |
| --- | --- | --- |
| [.github/copilot-instructions.md](../../../.github/copilot-instructions.md) | 21f26123b1c90b854735cb46f3ecc41d02c5be7cfa6c89f8840d96624a78e52b | 90/91/92 |
| [.plans/2026-10-22-pd-rust-渐进迁移主题/10-实现生产元数据后端与发布切换.md](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/10-实现生产元数据后端与发布切换.md) | ea95c6bc10428b4303ce1eb5a34a4a23aed22946546b008dc576d193d974cb76 | 90/91/92 |
| [.plans/2026-10-22-pd-rust-渐进迁移主题/7-补齐PD服务端与API.md](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/7-补齐PD服务端与API.md) | f01ee4f06c5608e439e774de63fc11828774f049edfbceb224674105e7c27f75 | 90/91/92 |
| [.plans/2026-10-22-pd-rust-渐进迁移主题/8-迁移控制面服务与MCS.md](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/8-迁移控制面服务与MCS.md) | ea04b21c53239ce1962f6e04952bc061420f7a7b1b281472facb8e62ec9e5128 | 90/91/92 |
| [.plans/2026-10-22-pd-rust-渐进迁移主题/9-迁移运维客户端与工具.md](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/9-迁移运维客户端与工具.md) | 06be9a4a1831612eae8371af533bbde00e238f7e09b7f6614de258a405f25cf0 | 90/91/92 |
| [AGENTS.md](../../../AGENTS.md) | a7deeb5123e3af7d0a9ed2cbba1b679dc9174b25d426f0788430871c09461200 | 90/91/92 |
| [client/resource_group/controller/config.go](../../../client/resource_group/controller/config.go) | 3f339a7c178e25137103338c0dcc4d38df9c8699d88629d4fec1e81d8cf4f608 | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller.go](../../../client/resource_group/controller/global_controller.go) | 9709bd33d0ed6a8078551ce099ee24575f9d0cf6e032724f97fc9dd1d520acd9 | 主题8；78/102边界 |
| [client/resource_group/controller/global_controller_test.go](../../../client/resource_group/controller/global_controller_test.go) | 4cf947ea6a0dce9365d688e66862e6f3eb1ece3809f324ab75b8a676f2fbf5af | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller.go](../../../client/resource_group/controller/group_controller.go) | e751ce097735d4446f77e0c2fdc8c40724ec4b5ac703fd7f27026f5be264f855 | 主题8；78/102边界 |
| [client/resource_group/controller/group_controller_test.go](../../../client/resource_group/controller/group_controller_test.go) | 99e1418f596cc7a0b58bf84961eb0ce01846b1a052ab620f45dede7fe9565aaa | 主题8；78/102边界 |
| [client/resource_group/controller/limiter.go](../../../client/resource_group/controller/limiter.go) | 3e08e99b23b8070e14b21f6c6117701f3604fdea1da66144269452137e24bc0e | 主题8；78/102边界 |
| [client/resource_group/controller/limiter_test.go](../../../client/resource_group/controller/limiter_test.go) | c0d5ab4d7c46817f9bcad3040f6d57ff086a9ce41cff32d94752ecec153b9f5e | 主题8；78/102边界 |
| [client/resource_group/controller/metrics/metrics.go](../../../client/resource_group/controller/metrics/metrics.go) | f45e77d16ca904ac6dd34eb5e91cdf7d966f57d6d6c0efe141441b2e6e7053dd | 主题8；78/102边界 |
| [client/resource_group/controller/model.go](../../../client/resource_group/controller/model.go) | 5b2a2619dc4f88413ee88100499683fb3914e1d4d9f8dbec2c861ac1b3227b02 | 主题8；78/102边界 |
| [client/resource_group/controller/model_test.go](../../../client/resource_group/controller/model_test.go) | 96654f42f8408957d528f2976cd60a972608da0d1f3d9c19d38b6b5e0f67359d | 主题8；78/102边界 |
| [client/resource_group/controller/request_source_metrics_test.go](../../../client/resource_group/controller/request_source_metrics_test.go) | 50a307663028d51d1f8c8f0e0e2efeef2f8eb98f250a4ab4026b69df2edc23cb | 主题8；78/102边界 |
| [client/resource_group/controller/testutil.go](../../../client/resource_group/controller/testutil.go) | ec6229e1dcb1b593d1c64584d871579665329bd1eff66a68dc4896893d59666c | 主题8；78/102边界 |
| [client/resource_group/controller/util.go](../../../client/resource_group/controller/util.go) | 75514dae232a750e35f66da4c05ba4431395fc1f587c4ec545a4074433c83844 | 主题8；78/102边界 |
| [client/resource_group/controller/util_test.go](../../../client/resource_group/controller/util_test.go) | c8f15acc826f0988c7b53179b5c046a22b80ec525ed83629150586991879895a | 主题8；78/102边界 |
| [cmd/pd-server/Cargo.toml](../../../cmd/pd-server/Cargo.toml) | 255faa82274f55b315a742a9439c00084182ee14df12050c3d71c9b8befcb891 | 90/91/92 |
| [cmd/pd-server/bin_main.rs](../../../cmd/pd-server/bin_main.rs) | 73a0908402dbee37f9745dc0b44bd61380defb176e5c1f8e660d2d12a94aa92c | 90/91/92 |
| [cmd/pd-server/lib.rs](../../../cmd/pd-server/lib.rs) | ac786710db8a0e1eb5dd4071f355344f2bea1acf66c8a569d6403031677fdb30 | 90/91/92 |
| [cmd/pd-server/main.go](../../../cmd/pd-server/main.go) | 00091e020c70f2a206e3dbe34fff924e0eb17da0a9a700121ba1b2d73318565d | 90/91/92 |
| [cmd/pd-server/main.rs](../../../cmd/pd-server/main.rs) | 44e254aafeb76f5de31d874234792499ae49ebaddb1ca3d6e292ed0ef77ba970 | 90/91/92 |
| [cmd/pd-server/main_test.rs](../../../cmd/pd-server/main_test.rs) | 7cfffce64d3f51f76db02942d26d3db670ae0f76e9041ba53d84afcb9b7c5a89 | 90/91/92 |
| [docs/rust/architecture.md](../../../docs/rust/architecture.md) | 3cb14bc9fcc037aaad34cf91ba7ade7343ae6a55e3325d2bbe2e8778e7ff8bef | 90/91/92 |
| [docs/rust/dependencies.md](../../../docs/rust/dependencies.md) | 2e88d1d7d265b9e9281063d09f89e1a561d045c81015fb5c3d64692403e22343 | 90/91/92 |
| [docs/rust/porting-guide.md](../../../docs/rust/porting-guide.md) | 8bfe9b1102e35a783402c7347e1e42ebf68581fa586736cf783cf32967c8e807 | 90/91/92 |
| [docs/rust/server-api/source-inventory.md](../../../docs/rust/server-api/source-inventory.md) | 82d331ebc1d45392f49c64dad2d631062d3efaa505e23aba643bea270b1a6315 | 90/91/92 |
| [docs/rust/server-api/task-001.md](../../../docs/rust/server-api/task-001.md) | 0758279511970cb3bc6a961715faa6a47d5704cf2e6a3b359b4baba7458993ce | 90/91/92 |
| [docs/rust/server-api/task-002.md](../../../docs/rust/server-api/task-002.md) | 79bd21cf5b0e3033fe13cc44099bca8df8154ab7b8397ea34276cc0dcae32c55 | 90/91/92 |
| [docs/rust/server-api/wire-inventory.md](../../../docs/rust/server-api/wire-inventory.md) | 4559438dd1be416382ced66c11d56d7154852c26c7586b1385216ab93c457971 | 90/91/92 |
| [pkg/audit/audit.go](../../../pkg/audit/audit.go) | 088f70fe9fe18b0ef5d99ad41160fe399b7d0b1d3330eefcfc300f174f59960d | 主题9；81/86/92/102接线 |
| [pkg/audit/audit_test.go](../../../pkg/audit/audit_test.go) | e28bb19d7759a66a2e4d2067b9fed6aef3d629ee10353ad69c177261452dc24b | 主题9；81/86/92/102接线 |
| [pkg/basicserver/basic_server.go](../../../pkg/basicserver/basic_server.go) | 52b69ee57a97d19f4f3e10593a78d024b642b1786b373bf1d7ffecf20797fae5 | 90/91/92 |
| [pkg/basicserver/metrics.go](../../../pkg/basicserver/metrics.go) | 31c02659be8ef323598358d00f7bc931fe3f41a40216ec012385ce5262681be0 | 15/86/101 |
| [pkg/core/metrics.go](../../../pkg/core/metrics.go) | 13fd85dba12904604493abeb2e34b32f7208f6b08ea393eec99368142fc63a7a | 15/86/101 |
| [pkg/election/metrics.go](../../../pkg/election/metrics.go) | 55181f401b3318c24fef73505d5164ffb399b733a3d494c4b21739e19e255d21 | 15/86/101 |
| [pkg/encryption/Cargo.toml](../../../pkg/encryption/Cargo.toml) | cfff1ccabe8724edaeb682ff62a09de1bd91529919ce8e1f24743702f073fb3d | 主题8；78/102边界 |
| [pkg/encryption/config.go](../../../pkg/encryption/config.go) | 2fdb709c297ea2d642cecef18f6f468f55c6b4168d6a40f634125da25f50d285 | 主题8；78/102边界 |
| [pkg/encryption/config_test.go](../../../pkg/encryption/config_test.go) | 6c64bc35bfd9096d33f8f4ea921f3f719aeec04760018a4bb25fa48b532a0f2c | 主题8；78/102边界 |
| [pkg/encryption/config_test.rs](../../../pkg/encryption/config_test.rs) | ee9a1ea6962682c5de9f0a392c3e5250b94c240ce2c974a2341e0b8b7d584f55 | 主题8；78/102边界 |
| [pkg/encryption/crypter.go](../../../pkg/encryption/crypter.go) | 10e8fa35a566e07dc2d8a254317c928db5db8674e779b444b01eccb82b8e55ae | 主题8；78/102边界 |
| [pkg/encryption/crypter_test.go](../../../pkg/encryption/crypter_test.go) | f388a57ddcb9cc1d7c101dfd735e229459139f9868e6c3ebe307823c8f4dd144 | 主题8；78/102边界 |
| [pkg/encryption/key_manager.go](../../../pkg/encryption/key_manager.go) | ff593c7ce739d3aba781482c19ae1fa2db29854e5367f6c208d920b0b80327ff | 主题8；78/102边界 |
| [pkg/encryption/key_manager_test.go](../../../pkg/encryption/key_manager_test.go) | fe403493afdbe44ebeb5b0febd8496c4d82a3d3b099cd0637dbf2e5cff06a31a | 主题8；78/102边界 |
| [pkg/encryption/kms.go](../../../pkg/encryption/kms.go) | 13536bce04649ad9e86e5fd62dd22963f21aa0461c8e1f9695a9934c0a7b413b | 主题8；78/102边界 |
| [pkg/encryption/lib.rs](../../../pkg/encryption/lib.rs) | c3771fc31cdac89b0552f8b2470e911b70bdc45a8531b4ba4a6cc05478034b08 | 主题8；78/102边界 |
| [pkg/encryption/master_key.go](../../../pkg/encryption/master_key.go) | 019a27adedad5362a9ff327bc9776f8949b26ab650d9f84ab47014903492906d | 主题8；78/102边界 |
| [pkg/encryption/master_key_test.go](../../../pkg/encryption/master_key_test.go) | b38ac65e5687b8ca99d826d1c1569e77f10e5280b416d93c894ba6ba0b376110 | 主题8；78/102边界 |
| [pkg/encryption/region_crypter.go](../../../pkg/encryption/region_crypter.go) | 13a951714bee9599439393d2dc06f5eb1f2b32f702cafa3d40c82b94bd6ad434 | 主题8；78/102边界 |
| [pkg/encryption/region_crypter_test.go](../../../pkg/encryption/region_crypter_test.go) | 4cbc317df437ac41081e1fdc632bf2e3626f029fc1546b0089c9c1cd3b3ac602 | 主题8；78/102边界 |
| [pkg/gc/metrics.go](../../../pkg/gc/metrics.go) | dbaea187d438ffab4fe2fedc7ed16d9f7932e224c11c3c070f0156076d1819e3 | 主题8；78/102边界 |
| [pkg/gc/metrics_test.go](../../../pkg/gc/metrics_test.go) | 3274e8c15e54cdd9d666ec59839f74c0d2003d7837e03955f42a9f0b9ebed2cf | 主题8；78/102边界 |
| [pkg/id/metrics.go](../../../pkg/id/metrics.go) | 613224400397f0de556f37895880861d610afd605507f2403dbd34fa10d8065b | 15/86/101 |
| [pkg/keyspace/metrics.go](../../../pkg/keyspace/metrics.go) | 6c0cc1dd99ff039eba0f3e5c964d362525592bfc22a68abfb465272f7679b7d0 | 主题8；78/102边界 |
| [pkg/maintenance/metrics.go](../../../pkg/maintenance/metrics.go) | 3ce211205604e30fe13a8e40c00ea4c34d4f1e713a2f51a16d9738147876685c | 15/86/101 |
| [pkg/mcs/resourcemanager/server/config.go](../../../pkg/mcs/resourcemanager/server/config.go) | 92b5591431db7e1c391ffbf805004ba80de9f6b357d7da54c1617468bf0e8134 | 主题8；78/102边界 |
| [pkg/mcs/resourcemanager/server/metrics.go](../../../pkg/mcs/resourcemanager/server/metrics.go) | b9f91be0015bb2da457fcf616fe3adf778cf3804ccd834b1bd02e044bef3e426 | 主题8；78/102边界 |
| [pkg/mcs/router/server/metrics.go](../../../pkg/mcs/router/server/metrics.go) | d7333f1d031746e64095523fddfa2fd52f91296aa565676746df5818bd65b31f | 主题8；78/102边界 |
| [pkg/mcs/scheduling/server/metrics.go](../../../pkg/mcs/scheduling/server/metrics.go) | 54bc4b343e0405e78aed11bb04b963d676f96406453d3ce5c9f5e9ef4678fa01 | 主题8；78/102边界 |
| [pkg/mcs/tso/server/metrics.go](../../../pkg/mcs/tso/server/metrics.go) | d179a8a67157775cf7bb91decc790d533577edf63db2a8007b9d0c88d8733546 | 主题8；78/102边界 |
| [pkg/member/metrics.go](../../../pkg/member/metrics.go) | 2b21d972aa67b340ecd0809f8eebc2c4b211e7b0cfd6363e0112e7ee7b4539c6 | 15/86/101 |
| [pkg/progress/metrics.go](../../../pkg/progress/metrics.go) | 64639f41a9bfe3b634821604c2647d88c30b9f0b73f364d5a17e2da7eac3df81 | 15/86/101 |
| [pkg/ratelimit/Cargo.toml](../../../pkg/ratelimit/Cargo.toml) | 5e50bdc7f0467773f56a6b7c0390defc8923f7cbe0ba7afaf15f12bbdbab59db | 90/91/92 |
| [pkg/ratelimit/concurrency.rs](../../../pkg/ratelimit/concurrency.rs) | 22ad205a3dbc6d7726337363758e65260e3aeecb6c80d0bace4786b7620362b2 | 90/91/92 |
| [pkg/ratelimit/concurrency_limiter.go](../../../pkg/ratelimit/concurrency_limiter.go) | 0f9400a3d1d32611e57ce39d808998be7e47e6e4846c536c69706a2497490849 | 90/91/92 |
| [pkg/ratelimit/concurrency_limiter_test.go](../../../pkg/ratelimit/concurrency_limiter_test.go) | 1f9dcb9ec538255859e36b6f39d18ec5bbcba4c4efe317ff35a243fe1601467e | 90/91/92 |
| [pkg/ratelimit/concurrency_test.rs](../../../pkg/ratelimit/concurrency_test.rs) | d4d5d6bef8124ec71af01692c4ba99156ce379bafe91761cd03482f0e7e7c5e0 | 90/91/92 |
| [pkg/ratelimit/controller.go](../../../pkg/ratelimit/controller.go) | 13fcc7094d67fa0452a5d66d6a265ba73daf69668cbf51f56a5d6614e2c04856 | 90/91/92 |
| [pkg/ratelimit/controller.rs](../../../pkg/ratelimit/controller.rs) | d13e6e1884bb615682960189607a7f64717a7008a0fb879e181062b82c5e88ed | 90/91/92 |
| [pkg/ratelimit/controller_test.go](../../../pkg/ratelimit/controller_test.go) | 1bc671df5fb5891a98d03a3a14939174cf766e5dd8b5b03b4a03140e801b0a41 | 90/91/92 |
| [pkg/ratelimit/controller_test.rs](../../../pkg/ratelimit/controller_test.rs) | b59d4639abc5384dfb87dd91058ca5a2ab358d1ca37d429ce737a877c6832cab | 90/91/92 |
| [pkg/ratelimit/lib.rs](../../../pkg/ratelimit/lib.rs) | 8bb83edea325afe0c73682ff7965c0d16aba361d47c284601561bc53ef03dade | 90/91/92 |
| [pkg/ratelimit/limiter.go](../../../pkg/ratelimit/limiter.go) | 00f776a44c9574caedf725aed04974c7911f26a31d1173055397ef2ce26cadab | 90/91/92 |
| [pkg/ratelimit/limiter.rs](../../../pkg/ratelimit/limiter.rs) | 0fdf255f12be9c360fdd367aa72ef93e15a90bd717007b969bd510333f4e68e8 | 90/91/92 |
| [pkg/ratelimit/limiter_test.go](../../../pkg/ratelimit/limiter_test.go) | 6e07a8571411adcbb095771b59b850655fecf13836237bac80940474793bc267 | 90/91/92 |
| [pkg/ratelimit/limiter_test.rs](../../../pkg/ratelimit/limiter_test.rs) | a731eff4ba47eb494f905de02344566126a31dfb699b20b8bee721310077155d | 90/91/92 |
| [pkg/ratelimit/metrics.go](../../../pkg/ratelimit/metrics.go) | b640bd11663bd0945f22d9ab1388129ef9e37ae88c9b4a75c250ad408df3d7f9 | 15/86/101 |
| [pkg/ratelimit/option.go](../../../pkg/ratelimit/option.go) | ed62c63a616185d0a90e3424471cdc49828ec094822203090bf685ca3392656d | 90/91/92 |
| [pkg/ratelimit/rate_limiter.rs](../../../pkg/ratelimit/rate_limiter.rs) | 6e757938d1730e4892691c7fbdb9c3ee6af008dc99b5d546d3a88bec2ce7d21e | 90/91/92 |
| [pkg/ratelimit/rate_limiter_test.rs](../../../pkg/ratelimit/rate_limiter_test.rs) | 4d7e88ed045eb43e9a6729e105df3cd56a07dea36eb9303cb9e503d8f96b58f6 | 90/91/92 |
| [pkg/ratelimit/ratelimiter.go](../../../pkg/ratelimit/ratelimiter.go) | 0ae575cc095e8791b3be7e0b2e5e74716253242465ba28d556eaa388dc32a877 | 90/91/92 |
| [pkg/ratelimit/ratelimiter_test.go](../../../pkg/ratelimit/ratelimiter_test.go) | 25d6996f724aef9be8d3273432aafa735b02ac0fd2447e1132d41a4d56413dc5 | 90/91/92 |
| [pkg/ratelimit/runner.go](../../../pkg/ratelimit/runner.go) | ef4a8a1c6d84228bfb9432bba5a5f1e754f78af15c40219d4c1ed5e5c9bcf190 | 90/91/92 |
| [pkg/ratelimit/runner.rs](../../../pkg/ratelimit/runner.rs) | 65b440b5fd9917c5485191f95f1c2e7c3f3bbb95b99f7c2f957f480ebdfdff68 | 90/91/92 |
| [pkg/ratelimit/runner_benchmark_test.go](../../../pkg/ratelimit/runner_benchmark_test.go) | daf04ed231dea218addf7b900b4a279716cc142188053f55883b18d47dc7ffe3 | 90/91/92 |
| [pkg/ratelimit/runner_test.go](../../../pkg/ratelimit/runner_test.go) | 6f54a1027fcea9d4292467c6c76d0bdb7f4d48e5cb7578d8f129c0b0f5452102 | 90/91/92 |
| [pkg/ratelimit/runner_test.rs](../../../pkg/ratelimit/runner_test.rs) | 3ab35c3501689471f9d95e122d5e9c867523ed203da1213f6c413d53a78ad50e | 90/91/92 |
| [pkg/replication/metrics.go](../../../pkg/replication/metrics.go) | b9745d620fde80b6275830d7ab53b721d2dc12c9f014146679a45265ff5034bd | 主题8；78/102边界 |
| [pkg/runtime/Cargo.toml](../../../pkg/runtime/Cargo.toml) | 0c1654eadd5b2d0be4dff887b0a31b7497d90420e74edda54f8992954c7c3f4f | 90/91/92 |
| [pkg/runtime/lib.rs](../../../pkg/runtime/lib.rs) | 6f7c75b008cb1b11a00ab2cf10be1ba196e11e463a3f4f84d5a44a5600f136e9 | 90/91/92 |
| [pkg/runtime/node.rs](../../../pkg/runtime/node.rs) | aaaf21d8c9010cecae983bf7ef44015a3a624856bf67874003c0fa94b772774c | 90/91/92 |
| [pkg/runtime/node_test.rs](../../../pkg/runtime/node_test.rs) | 98cfe2e0830816367ea9c85adfa38f07b940963c87069274ad1d77925d820513 | 90/91/92 |
| [pkg/schedule/affinity/metrics.go](../../../pkg/schedule/affinity/metrics.go) | 3adf1df35e7841e489b313d55d919ea83c36127f37164229cde9be1781898f1b | 15/86/101 |
| [pkg/schedule/checker/metrics.go](../../../pkg/schedule/checker/metrics.go) | 43494888f446b18223eb46c47c9f27fb0b4f75ba63c4d4fbf559c6d33e454cb5 | 15/86/101 |
| [pkg/schedule/config/Cargo.toml](../../../pkg/schedule/config/Cargo.toml) | 87613df49684f3b4c689db4517010a207156e69bde6815d47860f3ecba15f9b8 | 48 |
| [pkg/schedule/config/config.go](../../../pkg/schedule/config/config.go) | 33c0cce0ec0c3feb1b789a0366584433ad104786ecbab29edd2224d413398557 | 48 |
| [pkg/schedule/config/config_provider.go](../../../pkg/schedule/config/config_provider.go) | c9b89f57cd68ef3eeca260a0cb53883cfc9c32ad3ea375fa81438adaade280e5 | 48 |
| [pkg/schedule/config/config_provider_test.rs](../../../pkg/schedule/config/config_provider_test.rs) | cf9d21daeaa9a37f652066935ad5e6b8f9eb32a3552eba5e8379da5603d7dfe8 | 48 |
| [pkg/schedule/config/config_test.rs](../../../pkg/schedule/config/config_test.rs) | 1517300f8d7d58e14a0b641a21e3fac33073a6aa1ddc66fd5f1157ee7486fcec | 48 |
| [pkg/schedule/config/config_values_test.rs](../../../pkg/schedule/config/config_values_test.rs) | 8a23f39e96a4d7c8ad9605c72f9635f078c89729398216fb1fb1503cd4d8cda9 | 48 |
| [pkg/schedule/config/lib.rs](../../../pkg/schedule/config/lib.rs) | c65938bfb0c9623234aaf4855a5cf022f6cb3914d61e65444d89007731424e2e | 48 |
| [pkg/schedule/config/provider.rs](../../../pkg/schedule/config/provider.rs) | 60044585b818c3ddcb993f1d400d8924fa2df1ec6a983c9320a90b43cff0da56 | 48 |
| [pkg/schedule/config/store_config.go](../../../pkg/schedule/config/store_config.go) | 81060941f0cee6dbd46d052ff1f730cd236d04211155f2c28a43f7fb884596e9 | 48 |
| [pkg/schedule/config/store_config.rs](../../../pkg/schedule/config/store_config.rs) | d8e21cac1bb2fb18c958179d22cfee254a5d93e1f153a52986f88fb49b945c08 | 48 |
| [pkg/schedule/config/store_config_test.go](../../../pkg/schedule/config/store_config_test.go) | f4114a41ede0c192a243b38e0ac4acff357ab7458c8f050068d269925ee79dc4 | 48 |
| [pkg/schedule/config/store_config_test.rs](../../../pkg/schedule/config/store_config_test.rs) | 09bb4135390dd09d5edee4632e1b08bfbcdce2acfd5c2372cccc8b87e5d74dfa | 48 |
| [pkg/schedule/config/util.go](../../../pkg/schedule/config/util.go) | 12aa3025ba33fd0752d527e24a5c07d48e962caa7bff95325be4fabcd453f838 | 48 |
| [pkg/schedule/config/util_test.go](../../../pkg/schedule/config/util_test.go) | 158f0467ed31cbc97a0ba353fab94fba88fdcf01f610dbd79b83555251bfd1b2 | 48 |
| [pkg/schedule/config/value_contract.rs](../../../pkg/schedule/config/value_contract.rs) | 0239a53a8170203c329a6e61abedabe4b027cdaa51eb512dc1b368250a9fedf7 | 48 |
| [pkg/schedule/filter/metrics.go](../../../pkg/schedule/filter/metrics.go) | 2b9fcd6a5491be91ec05e88d3515f798eabfdbe8dc8b8fdc4d7e73bbd9073788 | 15/86/101 |
| [pkg/schedule/hbstream/metric.go](../../../pkg/schedule/hbstream/metric.go) | ff4b8689ae09cac77e99f529681d0624f94400b564d6b08329c83a782517267d | 15/86/101 |
| [pkg/schedule/labeler/metrics.go](../../../pkg/schedule/labeler/metrics.go) | 86ee9bbf981a6eac4a17f36e3cd0e3711d4e928d2c6a9b42525d8f65f5c43f4b | 15/86/101 |
| [pkg/schedule/metrics.go](../../../pkg/schedule/metrics.go) | a5d4ae0dbc395de732e1d307f0ccecb54319561ccb8d77ee4684fcf23e5c5791 | 15/86/101 |
| [pkg/schedule/operator/metrics.go](../../../pkg/schedule/operator/metrics.go) | 59d651a15bbc27f6f8911291afa78e42f4e89e84c33da6bb3e78a22f874edf8a | 15/86/101 |
| [pkg/schedule/scatter/metrics.go](../../../pkg/schedule/scatter/metrics.go) | f43418291a7bd618e02b5e01d206772d6252225e6dbb0d07cff4ab651762a37d | 15/86/101 |
| [pkg/schedule/schedulers/metrics.go](../../../pkg/schedule/schedulers/metrics.go) | add2f1dacf1fbd2f8fadd516ae4ab04895aea7783dc951b53deab1581165869e | 15/86/101 |
| [pkg/server/Cargo.toml](../../../pkg/server/Cargo.toml) | c84d7de24016c11f26da6cf0d2a16e701c878045dfc67923827bbc6f20849fa2 | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter.rs](../../../pkg/server/bucket_heartbeat_adapter.rs) | bc90b608e30374cd413b37545ad7bd01044838ec662643f274f55ab27d098ece | 90/91/92 |
| [pkg/server/bucket_heartbeat_adapter_test.rs](../../../pkg/server/bucket_heartbeat_adapter_test.rs) | a48b737721749b4410756b1c8290821a105ac1bfe02e0cf69c9d75e8b7724d69 | 90/91/92 |
| [pkg/server/core_region_adapter.rs](../../../pkg/server/core_region_adapter.rs) | a38004d49dfad69b02c81d90e6ed98ec166b6e455f464eaa9a05dc3f9becb2a2 | 90/91/92 |
| [pkg/server/core_region_adapter_test.rs](../../../pkg/server/core_region_adapter_test.rs) | ddf35f5cf6f65741858c55a8584fa1cc87ab47174a51996135d77699ff18ebf7 | 90/91/92 |
| [pkg/server/core_store_adapter.rs](../../../pkg/server/core_store_adapter.rs) | 226822ece326cb81dec448e9297e466c1b538b1a0c342929a6732e3e1ee015b6 | 90/91/92 |
| [pkg/server/core_store_adapter_test.rs](../../../pkg/server/core_store_adapter_test.rs) | cf7a5538d755e239452229a399003b3bf59f18bf35fadb93ac45e8ae8155ec2a | 90/91/92 |
| [pkg/server/core_trace.rs](../../../pkg/server/core_trace.rs) | 5f16f6064816bcf7895ce7a8cfc588d69c9c3121cfd79e12d93041d03c5aaf47 | 90/91/92 |
| [pkg/server/core_trace_test.rs](../../../pkg/server/core_trace_test.rs) | 88c81bbb3d7a7909290cdb9e91326253f038e80ea34249aa847df1ecfbf1ada5 | 90/91/92 |
| [pkg/server/examples/bucket_heartbeat_oracle.rs](../../../pkg/server/examples/bucket_heartbeat_oracle.rs) | 4593bb434c2490c715dbb45d453d08f91d9561435122def4b93b75ba5569454e | 90/91/92 |
| [pkg/server/examples/region_heartbeat_oracle.rs](../../../pkg/server/examples/region_heartbeat_oracle.rs) | d084ae065a8b2048a7a5491af248028f8d57c1f42dd05131808501150c01190c | 90/91/92 |
| [pkg/server/examples/store_heartbeat_oracle.rs](../../../pkg/server/examples/store_heartbeat_oracle.rs) | 3ab50cccdf9e9c2baab8fcc2d3ca7101d6786f3e4171ec7a6c5d15c0178aad37 | 90/91/92 |
| [pkg/server/examples/sync_codec_oracle.rs](../../../pkg/server/examples/sync_codec_oracle.rs) | 96db3f6370dfed3584e2fae59054d604ee8cd501c0060748565339518ae3285e | 90/91/92 |
| [pkg/server/grpc.rs](../../../pkg/server/grpc.rs) | 1505f261fe1b13f3e1db3e0b26d3370f2768304f05d9bd2dd1df054baf0349be | 90/91/92 |
| [pkg/server/http.rs](../../../pkg/server/http.rs) | fb4a46fbfd0bbf03d4e16bb5e37240f006e03c66057b1b6acb2039b6ab87f4f0 | 90/91/92 |
| [pkg/server/lib.rs](../../../pkg/server/lib.rs) | 81fbddb1ab8ba08c28ef1396628315d3f72bf303fa5c02ddf65901248c906a65 | 90/91/92 |
| [pkg/server/region_heartbeat_adapter.rs](../../../pkg/server/region_heartbeat_adapter.rs) | 2638486b1c1d8e787baf3d91a1570ad894a17095a3214dd8fe758ff129943cff | 90/91/92 |
| [pkg/server/region_heartbeat_adapter_test.rs](../../../pkg/server/region_heartbeat_adapter_test.rs) | b4838859365b6ab94a86ca2388b8ca554b2619a94600b8fa258d1623e9ed6b58 | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader.rs](../../../pkg/server/schedule_admin_balance_leader.rs) | 97f04873c66f6a6e76a9f35367eb361f084d59f2db21df194e671b50065b72a1 | 90/91/92 |
| [pkg/server/schedule_admin_balance_leader_test.rs](../../../pkg/server/schedule_admin_balance_leader_test.rs) | d2d0a18a2998117605eea2bcfedece77af62bb1b063350d1b4ae7d4e53dded47 | 90/91/92 |
| [pkg/server/schedule_admin_balance_range.rs](../../../pkg/server/schedule_admin_balance_range.rs) | 0983508a73b89a919c7e8fcd1f30e31338122c9032670300c5573cac684c083d | 90/91/92 |
| [pkg/server/schedule_admin_balance_range_test.rs](../../../pkg/server/schedule_admin_balance_range_test.rs) | 496e238a951593e2ae6260544b31acdad43627c758135660ba59fb9171aecfda | 90/91/92 |
| [pkg/server/schedule_admin_balance_region.rs](../../../pkg/server/schedule_admin_balance_region.rs) | 77d6951edc3c7a8ec486e2cda1b5295f38b9b8b594b5231807b72ad74f3e79a2 | 90/91/92 |
| [pkg/server/schedule_admin_balance_region_test.rs](../../../pkg/server/schedule_admin_balance_region_test.rs) | e478ccf69ce0effdc5e332fa4ded74610627b5d556ba38943f9ce7feeaa21e09 | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness.rs](../../../pkg/server/schedule_admin_balance_witness.rs) | 78ef201e39cc7dab8da748219af42a609ef760e32063b6717021e54b9c46caa1 | 90/91/92 |
| [pkg/server/schedule_admin_balance_witness_test.rs](../../../pkg/server/schedule_admin_balance_witness_test.rs) | ccd9694afbef84aeff8f4dd09c8c25155a80a17c4fc896fea16b576f1f925136 | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader.rs](../../../pkg/server/schedule_admin_evict_leader.rs) | bce4297042b781fc9042c4a0098ecff1e0c1f860a1bf708f4964ad15f41ff881 | 90/91/92 |
| [pkg/server/schedule_admin_evict_leader_test.rs](../../../pkg/server/schedule_admin_evict_leader_test.rs) | 4032a4e38b4a94dab9d8f1238984b1fc9561200150dd2179c58a9ceee77e7ad2 | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store.rs](../../../pkg/server/schedule_admin_evict_slow_store.rs) | 7208f790fad199e74a1a02ecb69d4ab6122ac7917977e2725e7cd1ff5be8191b | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_store_test.rs](../../../pkg/server/schedule_admin_evict_slow_store_test.rs) | f8623650b0a188bb6861fb7f07249231d3654893dde4dae5d85de4e3428421d2 | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend.rs](../../../pkg/server/schedule_admin_evict_slow_trend.rs) | 7df8bbb80292dfab757f6dd45b517a2e64b76936411b461fc0891f51a3ffa6bd | 90/91/92 |
| [pkg/server/schedule_admin_evict_slow_trend_test.rs](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs) | 3bc52bbb9e3bcd9c49eddec7a81fd8d8800f4df8288f6c18434c461b6ecf1c34 | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store.rs](../../../pkg/server/schedule_admin_evict_stopping_store.rs) | 4a6cabab8bfcea01d4ab88464ca0f65491a29e14c9a531f80d828f5eb66475ca | 90/91/92 |
| [pkg/server/schedule_admin_evict_stopping_store_test.rs](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs) | 7397f09336f5599a8586b48acdb45413ed1b742cf90f2e3c720c31328f0dbb03 | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region.rs](../../../pkg/server/schedule_admin_grant_hot_region.rs) | 0b54ac12b221366685d8db47c4e1f27346359219eca2550f274ffda197c9f485 | 90/91/92 |
| [pkg/server/schedule_admin_grant_hot_region_test.rs](../../../pkg/server/schedule_admin_grant_hot_region_test.rs) | 90617574a262a2a34e9f42c14f23eab294a8f1a0fbb9408002c04ab4bc7ae753 | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader.rs](../../../pkg/server/schedule_admin_grant_leader.rs) | f1967b9a9bad1f20e8209debe17a71485a47b3eef51d24d25bf20e75699f2c26 | 90/91/92 |
| [pkg/server/schedule_admin_grant_leader_test.rs](../../../pkg/server/schedule_admin_grant_leader_test.rs) | 9defb8e4dbbfd0f6b4d6fcf4e8c0984ae6b8fba740b1a85aded8d5384393e88a | 90/91/92 |
| [pkg/server/schedule_admin_hot_region.rs](../../../pkg/server/schedule_admin_hot_region.rs) | 3507173745338790da728951756880a5d2146d2c9d161b944d3bfe760217e087 | 90/91/92 |
| [pkg/server/schedule_admin_hot_region_test.rs](../../../pkg/server/schedule_admin_hot_region_test.rs) | 02b9ea9dcf6ce49a276637a19bdbeec63a429a78f2cdf3cefcfacde20be91108 | 90/91/92 |
| [pkg/server/schedule_admin_label.rs](../../../pkg/server/schedule_admin_label.rs) | 2e28d6a52a30f79bd5899478acd2153a45cbf69b906fc28278f2a3f6198da8cc | 90/91/92 |
| [pkg/server/schedule_admin_label_test.rs](../../../pkg/server/schedule_admin_label_test.rs) | 20ca4a4170e1530b1c11cf95e1667f5ac0281a147077ad51237cb02916dbc829 | 90/91/92 |
| [pkg/server/schedule_admin_random_merge.rs](../../../pkg/server/schedule_admin_random_merge.rs) | 0efd659bceba0e828c1ff14081644418ed8b0a14464de9c549f98f691663549d | 90/91/92 |
| [pkg/server/schedule_admin_random_merge_test.rs](../../../pkg/server/schedule_admin_random_merge_test.rs) | 2f35b585b81a3d325d7e161a30bd81c1d94229f22b5c05847d3cf0e6b08df08f | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range.rs](../../../pkg/server/schedule_admin_scatter_range.rs) | fdb95ffee664c13c3bfa53f53263aa6de9e369f802ad996ff2509592b3dc2946 | 90/91/92 |
| [pkg/server/schedule_admin_scatter_range_test.rs](../../../pkg/server/schedule_admin_scatter_range_test.rs) | 6e9ef37f534db527bed3bf8b6502fbc9a869b5ab0d6aab32d5f54041ec2729e7 | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region.rs](../../../pkg/server/schedule_admin_shuffle_hot_region.rs) | b9eed12620d7a50662c293273387226199ac3099f6d6bee7e2aab88521ad95f5 | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_hot_region_test.rs](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs) | 85663a4bcbf8dceab6d2c6417a33de84277e30f3fd1f0a53d4660be94d954694 | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader.rs](../../../pkg/server/schedule_admin_shuffle_leader.rs) | a7aad72e6e31709baaa091c2a608dfc17cfd31b2342dce48fcc62e20e4e6bfbb | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_leader_test.rs](../../../pkg/server/schedule_admin_shuffle_leader_test.rs) | 2099656caf021127f870cb1087ee3346dcff38a65bb8ab9aad6280a9e2da86ae | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region.rs](../../../pkg/server/schedule_admin_shuffle_region.rs) | a14629880620fe93c77cd4a59c44dd49b888f3507a9c8209c0c0f3a7e457a3e7 | 90/91/92 |
| [pkg/server/schedule_admin_shuffle_region_test.rs](../../../pkg/server/schedule_admin_shuffle_region_test.rs) | 8775e84439f5ee4b67b8dbbebf6c5823b64fa7b490dab364810275536335b3d3 | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader.rs](../../../pkg/server/schedule_admin_transfer_witness_leader.rs) | a21192731bb1450f24d15d62748c0accfeeb1a8e4f48a38701180882283db63c | 90/91/92 |
| [pkg/server/schedule_admin_transfer_witness_leader_test.rs](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs) | 3e4867fbd8a3fdfd5b36d4ebd0c9fe04d83fef493ad13382709294f24c7887a8 | 90/91/92 |
| [pkg/server/schedule_wire.rs](../../../pkg/server/schedule_wire.rs) | e4f386ea8947dfcfbb56ae0152c066395b9b7e6e29fe628169896de3578eb3a8 | 90/91/92 |
| [pkg/server/schedule_wire_test.rs](../../../pkg/server/schedule_wire_test.rs) | 0ada4192ff1fd4368706c33bf29267763f4abd7d668fe808e071868053a64411 | 90/91/92 |
| [pkg/server/server_test.rs](../../../pkg/server/server_test.rs) | 025e3b581ef436ebdfbabf551ad0a0ea4b5b87518cf72c798ee64900b0a10c08 | 90/91/92 |
| [pkg/server/store_heartbeat_adapter.rs](../../../pkg/server/store_heartbeat_adapter.rs) | 6f4a3beb03958aab571b806028b14964c9347459a5c68ae070c0f0bf2e00bd2e | 90/91/92 |
| [pkg/server/store_heartbeat_adapter_test.rs](../../../pkg/server/store_heartbeat_adapter_test.rs) | bc640dd36e955cd64065005719ff8ad5b883007ea601a8312709d95e47dd37eb | 90/91/92 |
| [pkg/server/sync_codec.rs](../../../pkg/server/sync_codec.rs) | b06ebdc360b1cd6ea58857dc999e5769679b7e0d23d964c413a14cd53287ba9f | 90/91/92 |
| [pkg/server/sync_codec_test.rs](../../../pkg/server/sync_codec_test.rs) | f4c6e660d6c48cdc1a99014ea5321e8334c20d3a69d8e155acc7900ae4f35b11 | 90/91/92 |
| [pkg/server/sync_transport.rs](../../../pkg/server/sync_transport.rs) | 23ffb4f5f18061dd926f22c52fa811ccbedbe8ebbfc8a6a5c3dcfa09e6249789 | 90/91/92 |
| [pkg/server/sync_transport_test.rs](../../../pkg/server/sync_transport_test.rs) | d33c9c7dffb79061384cd73e7206380b1f9fc6f02fd0150b1a3293db40cf44d7 | 90/91/92 |
| [pkg/statistics/buckets/metric.go](../../../pkg/statistics/buckets/metric.go) | 5c912821c3c65252df59ad222c35af974cd3b49555897b774902804b3f9911f9 | 15/86/101 |
| [pkg/statistics/metrics.go](../../../pkg/statistics/metrics.go) | 90722435c505b7ea9478fafa0ed0c5e60fd85c26461a0c11b1a10cb6a6e26aa0 | 15/86/101 |
| [pkg/storage/kv/metrics.go](../../../pkg/storage/kv/metrics.go) | eb5c263366aca6ab102aebd42a492c522a2ffcd2eb26ebaf324759e3cb9c3136 | 15/86/101 |
| [pkg/syncer/metrics.go](../../../pkg/syncer/metrics.go) | fd836c2fe7d8c48f266bf3f1de08ea1f232460160392592c07874bab0895f235 | 15/86/101 |
| [pkg/tso/metrics.go](../../../pkg/tso/metrics.go) | 3d72684d82ea06e3e324cefe597482fa8af6b31a1654fd188946c88483dee425 | 15/86/101 |
| [pkg/utils/apiutil/apiutil.go](../../../pkg/utils/apiutil/apiutil.go) | edda400120b9f6cc3def157e8aefc74e4572b7971d01b521f41b6bee49e6bfe6 | 90/91/92 |
| [pkg/utils/apiutil/apiutil_test.go](../../../pkg/utils/apiutil/apiutil_test.go) | f7e8636ea8527283afeea992a3da3acdc5de6830cad54902a6f069be8d7cf1cd | 90/91/92 |
| [pkg/utils/apiutil/multiservicesapi/middleware.go](../../../pkg/utils/apiutil/multiservicesapi/middleware.go) | 487a734a337a24d7aa9c362a7f291955df503f2253e39b1b75447840fee140a7 | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware.go](../../../pkg/utils/apiutil/serverapi/middleware.go) | 1ade7b1de2ce0bd5d46768bf100a2ef371215cc4aead1e463b9436cd42a92a0c | 50/79/80/81 |
| [pkg/utils/apiutil/serverapi/middleware_test.go](../../../pkg/utils/apiutil/serverapi/middleware_test.go) | 94749a7549dafa18783a234d7b6a8be42aec030c1654bbe440cd1eb303bb88d8 | 50/79/80/81 |
| [pkg/utils/configutil/Cargo.toml](../../../pkg/utils/configutil/Cargo.toml) | 936c30a65917b0aff087dc474102d6a9e649ad75b3e51240a051191767c17e0c | 90/91/92 |
| [pkg/utils/configutil/config_adjust.rs](../../../pkg/utils/configutil/config_adjust.rs) | 2419431a194b5aa63a22f9bc61209fcaef4c9c59cdfbc5878b3babef3a2c928b | 90/91/92 |
| [pkg/utils/configutil/config_adjust_test.rs](../../../pkg/utils/configutil/config_adjust_test.rs) | c629240f51d0ad50a6ee28dd3fd9dc3c17013cf6dad650ed063f5663be7ce54c | 90/91/92 |
| [pkg/utils/configutil/config_metadata.rs](../../../pkg/utils/configutil/config_metadata.rs) | f7a1c424f21a0fce24e640f0e91e46ff90bc8ccf21de9d08fc248c8f65d8286a | 90/91/92 |
| [pkg/utils/configutil/config_metadata_test.rs](../../../pkg/utils/configutil/config_metadata_test.rs) | 29382b49df031df439f8a156b7a6a6fab814b59d2bb220f93d72a6db9c9e864b | 90/91/92 |
| [pkg/utils/configutil/config_unknown.rs](../../../pkg/utils/configutil/config_unknown.rs) | 8ecacbceb03f0a26731e13fd699470b892d366df0cc7557fb42f5fd8e2b45470 | 90/91/92 |
| [pkg/utils/configutil/config_unknown_test.rs](../../../pkg/utils/configutil/config_unknown_test.rs) | 3d32735d1c937000e258ec455e5154730914b97f66da753ad811f80f857cc94e | 90/91/92 |
| [pkg/utils/configutil/configutil.go](../../../pkg/utils/configutil/configutil.go) | fde41872763e4cf00f48401a2e78d446d38b351db31068177df7bd60e5c5e6ee | 90/91/92 |
| [pkg/utils/configutil/configutil_test.go](../../../pkg/utils/configutil/configutil_test.go) | 45b17ef6f89e6ec1b9fb90b4ebec62f3b5283935a468be7f17a26f726935d121 | 90/91/92 |
| [pkg/utils/configutil/lib.rs](../../../pkg/utils/configutil/lib.rs) | 6a0fb615dd0de7c8eaf243df8c356c8e3621b086c1ca7b71794e19351ddf7b94 | 90/91/92 |
| [pkg/utils/configutil/security_config.rs](../../../pkg/utils/configutil/security_config.rs) | 7753fb15d9d8468a0b9b11957efc8399040bd9216ccc167e15105f8536b1c3a4 | 90/91/92 |
| [pkg/utils/etcdutil/metrics.go](../../../pkg/utils/etcdutil/metrics.go) | 81db37b69d533235317299124ddeff6fffd2221b20d1329279c87b9bc29c2ed4 | 15/86/101 |
| [pkg/utils/grpcutil/Cargo.toml](../../../pkg/utils/grpcutil/Cargo.toml) | 3d2d617db34ba14713fe43dd261e0f2fa93cf4531e55d35f275ebda6d53cde29 | 90/91/92 |
| [pkg/utils/grpcutil/cluster.go](../../../pkg/utils/grpcutil/cluster.go) | 7a265f1b78c2551517f1c8aa7ab085eeb0338c2f2d6425d11baf3283495dc6cb | 90/91/92 |
| [pkg/utils/grpcutil/cluster_test.go](../../../pkg/utils/grpcutil/cluster_test.go) | 2b878a5e8e0e5b7853ef725bf19585ec8edd524fdb4d12d4f7351889696b1762 | 90/91/92 |
| [pkg/utils/grpcutil/config_test.rs](../../../pkg/utils/grpcutil/config_test.rs) | c0e906b73bedd1574e77df7e082e77557d5d20a028402e71a67da76c19dffd23 | 90/91/92 |
| [pkg/utils/grpcutil/grpcutil.go](../../../pkg/utils/grpcutil/grpcutil.go) | e8523a4888c1fd2a0ee2d8cc6e1b914b25736eee359afa4d6f49e998b422613b | 90/91/92 |
| [pkg/utils/grpcutil/grpcutil_test.go](../../../pkg/utils/grpcutil/grpcutil_test.go) | ce715fe91f301c4a102cad1f287f3a5a9f18fccf8024691d554177e60b19e014 | 90/91/92 |
| [pkg/utils/grpcutil/lib.rs](../../../pkg/utils/grpcutil/lib.rs) | c1617455420244d0aec29f8fb07626c89f76ff806e4a253e0b5b85b8f640070b | 90/91/92 |
| [pkg/utils/grpcutil/metrics.go](../../../pkg/utils/grpcutil/metrics.go) | a52919d1f9dbaa5dcc506f3b37dc78b4519d8099f888a35dfd71f8cecc7bf2a4 | 15/86/101 |
| [pkg/utils/metricutil/Cargo.toml](../../../pkg/utils/metricutil/Cargo.toml) | 6bb1b284c8dbbc267ac7933f159664874915945bf78e93ae0a2cc22b963e88a2 | 15/86/101 |
| [pkg/utils/metricutil/config_test.rs](../../../pkg/utils/metricutil/config_test.rs) | 2011e411b1dc4a236d95ae8262f151cbb2f7e5c1e029166f1f67954e42639df8 | 15/86/101 |
| [pkg/utils/metricutil/lib.rs](../../../pkg/utils/metricutil/lib.rs) | d87d1560da5b18917f00aa224cd8462847505db304ee7df5ea2afbde9185a0c2 | 15/86/101 |
| [pkg/utils/metricutil/metricutil.go](../../../pkg/utils/metricutil/metricutil.go) | 8000f3e27167f7b36f6c6b33a38bc241264ffb7e57ac49f8267715cd56fd2e8e | 15/86/101 |
| [pkg/utils/metricutil/metricutil_test.go](../../../pkg/utils/metricutil/metricutil_test.go) | 0acd50a31056ebd4f67dda4debf5ce1308c3bc743a1fa8d528161820974e4f49 | 15/86/101 |
| [pkg/utils/tsoutil/tso_dispatcher_test.go](../../../pkg/utils/tsoutil/tso_dispatcher_test.go) | 39532c69e35d3244230ad959d463d4bea3d28736b215a275ccbb30957459b070 | 90/91/92 |
| [server/api/middleware.go](../../../server/api/middleware.go) | b33e56c1f1540e3ce02a1d6552f7707b446b0f169b271fa004cdded1865dca92 | 50/79/80/81 |
| [server/api/router.go](../../../server/api/router.go) | 153c7396117af4561c33e9dfe0529e1a0ef45ba5304fa9e41600cd7e60eb2b79 | 90/91/92 |
| [server/apiv2/router.go](../../../server/apiv2/router.go) | c6661e0e949be8eb7fdf4823991f4456902c3b02129317352e522b2daf242837 | 90/91/92 |
| [server/cluster/metrics.go](../../../server/cluster/metrics.go) | d05081e40e6708ad169f5e6424dc53a9842806099f5426f50b4adbee30502183 | 15/86/101 |
| [server/config/Cargo.toml](../../../server/config/Cargo.toml) | 55cddfb8f5351267918fc10eb63d7f6093843b4302f1852efc57214b2d3f59b5 | 47/90 |
| [server/config/config.go](../../../server/config/config.go) | fe6565d6d88cebbec8171adea49bba688762be8b32b2884ed4220d3c4d6448f8 | 47/90 |
| [server/config/config_decode.rs](../../../server/config/config_decode.rs) | 91d8cac060a2e65e504f3a8f3b80bbacabcd3caab3a7edfe062809d1c907d8da | 47/90 |
| [server/config/config_decode_test.rs](../../../server/config/config_decode_test.rs) | 46ddce324b8380720f765600ed3754a082f39d4abdd546afc9ef78be4122fe29 | 47/90 |
| [server/config/config_defaults.rs](../../../server/config/config_defaults.rs) | a8e30e649259de316ca53da1b97b1a06cee3d429e07bc75d14fde03ae7ba2f93 | 47/90 |
| [server/config/config_defaults_test.rs](../../../server/config/config_defaults_test.rs) | ce443f51864c7ba3dcb42bed46e932c56b3a736024cecb13e5b2d3c0879515cb | 47/90 |
| [server/config/config_hot_update_test.rs](../../../server/config/config_hot_update_test.rs) | f473cd1fd38be5062551cb9788f8b74cb2b3fba3b18bd6c00c85bd5867408f92 | 47/90 |
| [server/config/config_persist_test.rs](../../../server/config/config_persist_test.rs) | e99b23a88c8881f5f5bd5477777d91cf1f4cad7ad27adfd3e3fbec3ae0a501ad | 47/90 |
| [server/config/config_test.go](../../../server/config/config_test.go) | 705c57ffdbd953a5e9c274a1fe7cc31db8b6ebdbf31e4493f798242a245a847f | 47/90 |
| [server/config/config_tso.rs](../../../server/config/config_tso.rs) | 013046025d613351814435d28d95800aa2bd849f3e556f3f56e4c00c519163c9 | 47/90 |
| [server/config/config_tso_test.rs](../../../server/config/config_tso_test.rs) | 57e8b5775341e646b4dcc3251158708e86a3f74b77de7c017df28da74138b657 | 47/90 |
| [server/config/config_urls.rs](../../../server/config/config_urls.rs) | 5a7fcd732d746400b5f264142f86196c08276bcdb586e2d0cff8a2bd825cf2c5 | 47/90 |
| [server/config/config_urls_test.rs](../../../server/config/config_urls_test.rs) | b486d35a5a12d721f7d76c62ab3fd5f23a5c92cf1deb8b7f6ef8f326f208149c | 47/90 |
| [server/config/dashboard_config.rs](../../../server/config/dashboard_config.rs) | 6adbfa24c9755ab7048121c4b5c8e243cc55686a44c48f5c23c1620a1e6adead | 47/90 |
| [server/config/dashboard_config_test.rs](../../../server/config/dashboard_config_test.rs) | 77b9fd3bf776aad6211cad039ca908c471554a2ef6893ca91249998f7d58c6bf | 47/90 |
| [server/config/deprecated_config.rs](../../../server/config/deprecated_config.rs) | 49e19a9153f7782b6f49f4aa4fd4bd5bbc9593649f124f9ebe7e93fa36239822 | 47/90 |
| [server/config/deprecated_config_test.rs](../../../server/config/deprecated_config_test.rs) | 39fdcc43a7a244b81338e3963c28adae25f91dce02445d1887e537243122571a | 47/90 |
| [server/config/fixtures/nested_config_go.json](../../../server/config/fixtures/nested_config_go.json) | c93d600dc32de9a62a2496ba8dce4a949270941cd1529b5417a45969a48ba7b8 | 47/90 |
| [server/config/keyspace_config.rs](../../../server/config/keyspace_config.rs) | ffa2e214b1a1814f68b827c0de122f851636b7158ec2a0aff79d62f6dee5fc7d | 47/90 |
| [server/config/keyspace_config_test.rs](../../../server/config/keyspace_config_test.rs) | 3686fbae59ca1477412640527a7835042f9e0e0bae6c8b2cedeacbb03811d3b3 | 47/90 |
| [server/config/lib.rs](../../../server/config/lib.rs) | 6f16860bb1833f5bc701781119444b203d45100214681e69978060f1d4404b65 | 47/90 |
| [server/config/metrics.go](../../../server/config/metrics.go) | 709d594be4d7b2233cf4804e2080c234b3f546ba34898692e2f5ca3b78586d37 | 47/90 |
| [server/config/microservice_config.rs](../../../server/config/microservice_config.rs) | 63cfe4c91556e3ac0c7589030047f93915869594b3a85748ae1980623bbedef4 | 47/90 |
| [server/config/microservice_config_test.rs](../../../server/config/microservice_config_test.rs) | 7e44f823217a26d58f483703ec7dfc18a7519d547b939f180f04248d3102dd1d | 47/90 |
| [server/config/middleware_config.rs](../../../server/config/middleware_config.rs) | f502b00bb6a68e5413f160e1555f0b45b36630f6bb8dfdff7b29a7c34aaf2300 | 50/79/80/81 |
| [server/config/middleware_config_test.rs](../../../server/config/middleware_config_test.rs) | b457ce0e53b81715eb086185c4647aab16dd297968176b4c4976530e4131d3bd | 50/79/80/81 |
| [server/config/nested_config.rs](../../../server/config/nested_config.rs) | 17d6adb2c7fc4e357239c49006b50e4235d350d7cdba7ebafbd37e9aa3231694 | 47/90 |
| [server/config/nested_config_test.rs](../../../server/config/nested_config_test.rs) | 5a23694872985c4b0e477ac14de389482a52568ca6a31b04e6bf0f34d1c9bb8d | 47/90 |
| [server/config/pd_server_defaults.rs](../../../server/config/pd_server_defaults.rs) | caf63a77229b60ebd706efed96927f2d12259c54c2a012babb501b7ec4760ed6 | 47/90 |
| [server/config/pd_server_defaults_test.rs](../../../server/config/pd_server_defaults_test.rs) | e9ddb6a64ca0d0eae09936701690485fb28846b5a21ae61fa2eeede23e55372c | 47/90 |
| [server/config/pd_server_gc.rs](../../../server/config/pd_server_gc.rs) | f9bf88052d7129b736076f13eeeccbc662d15539097240243a6483386eda2a40 | 47/90 |
| [server/config/pd_server_gc_test.rs](../../../server/config/pd_server_gc_test.rs) | f36d1bea8c7a0edc76dcfae50793cd2fb8bd82009569dc665f750d78800d0ce3 | 47/90 |
| [server/config/pd_server_validate.rs](../../../server/config/pd_server_validate.rs) | 50438eeeef827f0715e29aaac0e3c8c6b5f0424cc3758f8ab8e8988162c94f6b | 47/90 |
| [server/config/pd_server_validate_test.rs](../../../server/config/pd_server_validate_test.rs) | fb6583c62c3818ebaf32782364a32ac4bf93c35b621e5e4bee2c8a46e79239ff | 47/90 |
| [server/config/persist_options.go](../../../server/config/persist_options.go) | d49284b2eee7f9a509f32598ed69fe47bdccda674cb4940d80c2ac7ad10b62ff | 47/90 |
| [server/config/persist_schedule.rs](../../../server/config/persist_schedule.rs) | 9df9cb22edfcf76193109fe1ef4a298006ae747b53e5821e221770fc65eaee0d | 48 |
| [server/config/replication_config.rs](../../../server/config/replication_config.rs) | a829afc2562b3d0f37afb62d1e7b1adbca67fa46229d32a9c241eb13a4f7f00f | 47/90 |
| [server/config/replication_config_test.rs](../../../server/config/replication_config_test.rs) | e1962abcb5875ca6c20de346fea3ca0d2fb22d198ca9d6fbf54846f3adc863f3 | 47/90 |
| [server/config/service_middleware_config.go](../../../server/config/service_middleware_config.go) | bf5fcd9ef17ac7810e84b2f2447c20178f6ff6c12dc7537e91a6f83d2d2399b1 | 50/79/80/81 |
| [server/config/service_middleware_persist_options.go](../../../server/config/service_middleware_persist_options.go) | f14e3648c04aba2723ba0c10f53a60811fc9e65b7b84d094fa4f954e15ea8f5e | 50/79/80/81 |
| [server/config/util.go](../../../server/config/util.go) | 503276291dee3b7811b0dad72b7a1b95e3a8cc76a79fd3b208ec952072c139fc | 47/90 |
| [server/config/util_test.go](../../../server/config/util_test.go) | c9d87606ed8d565288206e89058375e227da65036810bfa53d67373b91b039d0 | 47/90 |
| [server/grpc_service.go](../../../server/grpc_service.go) | 16fe2a147fe3d34966ca08c59c429871bf56242879839b56ded50af34e8dc088 | 90/91/92 |
| [server/metrics.go](../../../server/metrics.go) | 76eadd41bc68d62b19605ca4e2578e176311aebb2f8d72d50e39b26a5c62917f | 15/86/101 |
| [server/server.go](../../../server/server.go) | 829eb592771c69f68f0c5efb8b6a88565828c3b0d5430d01c8ec60948fb2ad80 | 90/91/92 |
