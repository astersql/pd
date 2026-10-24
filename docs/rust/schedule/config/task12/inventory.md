# 批次 12 包级盘点补充

基准 `db1b56605b43b973afbd1eef8d03a7e7ba25e0bf`；目标 Go 文件相对基准无差异。
基础调度包的完整生产文件、公开声明、依赖/反向依赖、测试、failpoint、指标和
任务映射复用 [批次 9 盘点](../task9/inventory.md)。Config 的完整字段、默认值、
外部准入和公开接口复用 [服务配置契约盘点](../../../theme3-服务配置契约.md)。

server/config 生产 Go 文件如下；本次扩展已有 canonical Rust crate，不新增平行包。

- `server/config/config.go`
- `server/config/metrics.go`
- `server/config/persist_options.go`
- `server/config/service_middleware_config.go`
- `server/config/service_middleware_persist_options.go`
- `server/config/util.go`

公开类型与函数/方法声明（只读索引，方法体按任务审查）：

```go
server/config/util.go:27:func ValidateURLWithScheme(rawURL string) error {
server/config/service_middleware_persist_options.go:28:type ServiceMiddlewarePersistOptions struct {
server/config/service_middleware_persist_options.go:35:func NewServiceMiddlewarePersistOptions(cfg *ServiceMiddlewareConfig) *ServiceMiddlewarePersistOptions {
server/config/service_middleware_persist_options.go:44:func (o *ServiceMiddlewarePersistOptions) GetAuditConfig() *AuditConfig {
server/config/service_middleware_persist_options.go:49:func (o *ServiceMiddlewarePersistOptions) SetAuditConfig(cfg *AuditConfig) {
server/config/service_middleware_persist_options.go:54:func (o *ServiceMiddlewarePersistOptions) IsAuditEnabled() bool {
server/config/service_middleware_persist_options.go:59:func (o *ServiceMiddlewarePersistOptions) GetRateLimitConfig() *RateLimitConfig {
server/config/service_middleware_persist_options.go:64:func (o *ServiceMiddlewarePersistOptions) SetRateLimitConfig(cfg *RateLimitConfig) {
server/config/service_middleware_persist_options.go:69:func (o *ServiceMiddlewarePersistOptions) IsRateLimitEnabled() bool {
server/config/service_middleware_persist_options.go:74:func (o *ServiceMiddlewarePersistOptions) GetGRPCRateLimitConfig() *GRPCRateLimitConfig {
server/config/service_middleware_persist_options.go:79:func (o *ServiceMiddlewarePersistOptions) SetGRPCRateLimitConfig(cfg *GRPCRateLimitConfig) {
server/config/service_middleware_persist_options.go:84:func (o *ServiceMiddlewarePersistOptions) IsGRPCRateLimitEnabled() bool {
server/config/service_middleware_persist_options.go:89:func (o *ServiceMiddlewarePersistOptions) Persist(storage endpoint.ServiceMiddlewareStorage) error {
server/config/service_middleware_persist_options.go:103:func (o *ServiceMiddlewarePersistOptions) Reload(storage endpoint.ServiceMiddlewareStorage) error {
server/config/service_middleware_config.go:26:type ServiceMiddlewareConfig struct {
server/config/service_middleware_config.go:33:func NewServiceMiddlewareConfig() *ServiceMiddlewareConfig {
server/config/service_middleware_config.go:54:func (c *ServiceMiddlewareConfig) Clone() *ServiceMiddlewareConfig {
server/config/service_middleware_config.go:60:type AuditConfig struct {
server/config/service_middleware_config.go:66:func (c *AuditConfig) Clone() *AuditConfig {
server/config/service_middleware_config.go:72:type RateLimitConfig struct {
server/config/service_middleware_config.go:80:func (c *RateLimitConfig) Clone() *RateLimitConfig {
server/config/service_middleware_config.go:91:type GRPCRateLimitConfig struct {
server/config/service_middleware_config.go:99:func (c *GRPCRateLimitConfig) Clone() *GRPCRateLimitConfig {
server/config/persist_options.go:49:type PersistOptions struct {
server/config/persist_options.go:66:func NewPersistOptions(cfg *Config) *PersistOptions {
server/config/persist_options.go:84:func (o *PersistOptions) GetScheduleConfig() *sc.ScheduleConfig {
server/config/persist_options.go:89:func (o *PersistOptions) SetScheduleConfig(cfg *sc.ScheduleConfig) {
server/config/persist_options.go:109:func (o *PersistOptions) UpdateScheduleConfig(
server/config/persist_options.go:131:func (o *PersistOptions) SetSchedulers(schedulers sc.SchedulerConfigs) {
server/config/persist_options.go:138:func (o *PersistOptions) GetReplicationConfig() *sc.ReplicationConfig {
server/config/persist_options.go:143:func (o *PersistOptions) SetReplicationConfig(cfg *sc.ReplicationConfig) {
server/config/persist_options.go:148:func (o *PersistOptions) GetPDServerConfig() *PDServerConfig {
server/config/persist_options.go:153:func (o *PersistOptions) SetPDServerConfig(cfg *PDServerConfig) {
server/config/persist_options.go:158:func (o *PersistOptions) GetReplicationModeConfig() *ReplicationModeConfig {
server/config/persist_options.go:163:func (o *PersistOptions) SetReplicationModeConfig(cfg *ReplicationModeConfig) {
server/config/persist_options.go:168:func (o *PersistOptions) GetLabelPropertyConfig() LabelPropertyConfig {
server/config/persist_options.go:173:func (o *PersistOptions) SetLabelPropertyConfig(cfg LabelPropertyConfig) {
server/config/persist_options.go:178:func (o *PersistOptions) GetKeyspaceConfig() *KeyspaceConfig {
server/config/persist_options.go:183:func (o *PersistOptions) SetKeyspaceConfig(cfg *KeyspaceConfig) {
server/config/persist_options.go:188:func (o *PersistOptions) CASKeyspaceConfig(oldCfg, newCfg *KeyspaceConfig) bool {
server/config/persist_options.go:193:func (o *PersistOptions) GetMicroserviceConfig() *MicroserviceConfig {
server/config/persist_options.go:198:func (o *PersistOptions) SetMicroserviceConfig(cfg *MicroserviceConfig) {
server/config/persist_options.go:203:func (o *PersistOptions) GetStoreConfig() *sc.StoreConfig {
server/config/persist_options.go:208:func (o *PersistOptions) SetStoreConfig(cfg *sc.StoreConfig) {
server/config/persist_options.go:213:func (o *PersistOptions) GetClusterVersion() *semver.Version {
server/config/persist_options.go:218:func (o *PersistOptions) SetClusterVersion(v *semver.Version) {
server/config/persist_options.go:223:func (o *PersistOptions) CASClusterVersion(old, new *semver.Version) bool {
server/config/persist_options.go:228:func (o *PersistOptions) GetLocationLabels() []string {
server/config/persist_options.go:233:func (o *PersistOptions) SetLocationLabels(labels []string) {
server/config/persist_options.go:240:func (o *PersistOptions) GetIsolationLevel() string {
server/config/persist_options.go:245:func (o *PersistOptions) IsPlacementRulesEnabled() bool {
server/config/persist_options.go:250:func (o *PersistOptions) GetAffinityScheduleLimit() uint64 {
server/config/persist_options.go:255:func (o *PersistOptions) GetSplitScatterScheduleLimit() uint64 {
server/config/persist_options.go:260:func (o *PersistOptions) SetAffinityScheduleLimit(limit uint64) {
server/config/persist_options.go:267:func (o *PersistOptions) SetSplitScatterScheduleLimit(limit uint64) {
server/config/persist_options.go:274:func (o *PersistOptions) SetPlacementRuleEnabled(enabled bool) {
server/config/persist_options.go:281:func (o *PersistOptions) IsPlacementRulesCacheEnabled() bool {
server/config/persist_options.go:286:func (o *PersistOptions) SetPlacementRulesCacheEnabled(enabled bool) {
server/config/persist_options.go:293:func (o *PersistOptions) GetStrictlyMatchLabel() bool {
server/config/persist_options.go:298:func (o *PersistOptions) GetMaxReplicas() int {
server/config/persist_options.go:303:func (o *PersistOptions) SetMaxReplicas(replicas int) {
server/config/persist_options.go:326:func IsSupportedTTLConfig(key string) bool {
server/config/persist_options.go:336:func (o *PersistOptions) GetMaxSnapshotCount() uint64 {
server/config/persist_options.go:341:func (o *PersistOptions) GetMaxPendingPeerCount() uint64 {
server/config/persist_options.go:346:func (o *PersistOptions) GetMaxMergeRegionSize() uint64 {
server/config/persist_options.go:351:func (o *PersistOptions) GetMaxAffinityMergeRegionSize() uint64 {
server/config/persist_options.go:357:func (o *PersistOptions) GetMaxMergeRegionKeys() uint64 {
server/config/persist_options.go:370:func (o *PersistOptions) GetSplitMergeInterval() time.Duration {
server/config/persist_options.go:375:func (o *PersistOptions) SetSplitMergeInterval(splitMergeInterval time.Duration) {
server/config/persist_options.go:382:func (o *PersistOptions) GetSwitchWitnessInterval() time.Duration {
server/config/persist_options.go:387:func (o *PersistOptions) IsDiagnosticAllowed() bool {
server/config/persist_options.go:392:func (o *PersistOptions) SetEnableDiagnostic(enable bool) {
server/config/persist_options.go:399:func (o *PersistOptions) IsWitnessAllowed() bool {
server/config/persist_options.go:404:func (o *PersistOptions) SetEnableWitness(enable bool) {
server/config/persist_options.go:411:func (o *PersistOptions) SetMaxStoreDownTime(time time.Duration) {
server/config/persist_options.go:418:func (o *PersistOptions) SetMaxMergeRegionSize(maxMergeRegionSize uint64) {
server/config/persist_options.go:425:func (o *PersistOptions) SetMaxAffinityMergeRegionSize(maxAffinityMergeRegionSize uint64) {
server/config/persist_options.go:432:func (o *PersistOptions) SetMaxMergeRegionKeys(maxMergeRegionKeys uint64) {
server/config/persist_options.go:439:func (o *PersistOptions) SetStoreLimit(storeID uint64, typ storelimit.Type, ratePerMin float64) {
server/config/persist_options.go:450:func (o *PersistOptions) SetAllStoresLimit(typ storelimit.Type, ratePerMin float64) {
server/config/persist_options.go:461:func (o *PersistOptions) IsOneWayMergeEnabled() bool {
server/config/persist_options.go:466:func (o *PersistOptions) IsCrossTableMergeEnabled() bool {
server/config/persist_options.go:471:func (o *PersistOptions) GetPatrolRegionInterval() time.Duration {
server/config/persist_options.go:476:func (o *PersistOptions) GetMaxStoreDownTime() time.Duration {
server/config/persist_options.go:481:func (o *PersistOptions) GetMaxStorePreparingTime() time.Duration {
server/config/persist_options.go:486:func (o *PersistOptions) GetLeaderScheduleLimit() uint64 {
server/config/persist_options.go:491:func (o *PersistOptions) GetRegionScheduleLimit() uint64 {
server/config/persist_options.go:496:func (o *PersistOptions) GetWitnessScheduleLimit() uint64 {
server/config/persist_options.go:501:func (o *PersistOptions) GetReplicaScheduleLimit() uint64 {
server/config/persist_options.go:506:func (o *PersistOptions) GetMergeScheduleLimit() uint64 {
server/config/persist_options.go:511:func (o *PersistOptions) GetHotRegionScheduleLimit() uint64 {
server/config/persist_options.go:516:func (o *PersistOptions) GetStoreLimit(storeID uint64) (returnSC sc.StoreLimitConfig) {
server/config/persist_options.go:532:func (o *PersistOptions) GetStoreLimitByType(storeID uint64, typ storelimit.Type) (returned float64) {
server/config/persist_options.go:546:func (o *PersistOptions) PeekStoreLimitByType(storeID uint64, typ storelimit.Type) float64 {
server/config/persist_options.go:572:func (o *PersistOptions) GetAllStoresLimit() map[uint64]sc.StoreLimitConfig {
server/config/persist_options.go:577:func (o *PersistOptions) GetStoreLimitVersion() string {
server/config/persist_options.go:582:func (o *PersistOptions) GetTolerantSizeRatio() float64 {
server/config/persist_options.go:587:func (o *PersistOptions) GetLowSpaceRatio() float64 {
server/config/persist_options.go:592:func (o *PersistOptions) GetSlowStoreEvictingAffectedStoreRatioThreshold() float64 {
server/config/persist_options.go:597:func (o *PersistOptions) GetHighSpaceRatio() float64 {
server/config/persist_options.go:602:func (o *PersistOptions) GetRegionScoreFormulaVersion() string {
server/config/persist_options.go:607:func (o *PersistOptions) GetSchedulerMaxWaitingOperator() uint64 {
server/config/persist_options.go:612:func (o *PersistOptions) GetLeaderSchedulePolicy() constant.SchedulePolicy {
server/config/persist_options.go:617:func (o *PersistOptions) GetKeyType() constant.KeyType {
server/config/persist_options.go:622:func (o *PersistOptions) GetMaxResetTSGap() time.Duration {
server/config/persist_options.go:627:func (o *PersistOptions) GetDashboardAddress() string {
server/config/persist_options.go:632:func (o *PersistOptions) IsUseRegionStorage() bool {
server/config/persist_options.go:637:func (o *PersistOptions) GetServerMemoryLimit() float64 {
server/config/persist_options.go:642:func (o *PersistOptions) GetServerMemoryLimitGCTrigger() float64 {
server/config/persist_options.go:647:func (o *PersistOptions) GetEnableGOGCTuner() bool {
server/config/persist_options.go:652:func (o *PersistOptions) GetGCTunerThreshold() float64 {
server/config/persist_options.go:657:func (o *PersistOptions) IsRemoveDownReplicaEnabled() bool {
server/config/persist_options.go:662:func (o *PersistOptions) IsReplaceOfflineReplicaEnabled() bool {
server/config/persist_options.go:667:func (o *PersistOptions) IsMakeUpReplicaEnabled() bool {
server/config/persist_options.go:672:func (o *PersistOptions) IsRemoveExtraReplicaEnabled() bool {
server/config/persist_options.go:677:func (o *PersistOptions) IsLocationReplacementEnabled() bool {
server/config/persist_options.go:682:func (o *PersistOptions) IsTikvRegionSplitEnabled() bool {
server/config/persist_options.go:687:func (o *PersistOptions) GetMaxMovableHotPeerSize() int64 {
server/config/persist_options.go:692:func (o *PersistOptions) IsDebugMetricsEnabled() bool {
server/config/persist_options.go:697:func (o *PersistOptions) IsUseJointConsensus() bool {
server/config/persist_options.go:702:func (o *PersistOptions) SetEnableUseJointConsensus(enable bool) {
server/config/persist_options.go:710:func (o *PersistOptions) IsTraceRegionFlow() bool {
server/config/persist_options.go:715:func (o *PersistOptions) GetHotRegionCacheHitsThreshold() int {
server/config/persist_options.go:720:func (o *PersistOptions) GetPatrolRegionWorkerCount() int {
server/config/persist_options.go:725:func (o *PersistOptions) GetStoresLimit() map[uint64]sc.StoreLimitConfig {
server/config/persist_options.go:730:func (o *PersistOptions) GetSchedulers() sc.SchedulerConfigs {
server/config/persist_options.go:735:func (o *PersistOptions) IsSchedulerDisabled(tp types.CheckerSchedulerType) bool {
server/config/persist_options.go:747:func (o *PersistOptions) GetHotRegionsWriteInterval() time.Duration {
server/config/persist_options.go:752:func (o *PersistOptions) GetHotRegionsReservedDays() uint64 {
server/config/persist_options.go:757:func (o *PersistOptions) AddSchedulerCfg(tp types.CheckerSchedulerType, args []string) {
server/config/persist_options.go:779:func (o *PersistOptions) RemoveSchedulerCfg(tp types.CheckerSchedulerType) {
server/config/persist_options.go:797:func (o *PersistOptions) SetLabelProperty(typ, labelKey, labelValue string) {
server/config/persist_options.go:809:func (o *PersistOptions) DeleteLabelProperty(typ, labelKey, labelValue string) {
server/config/persist_options.go:834:func (c *persistedConfig) UnmarshalJSON(data []byte) error {
server/config/persist_options.go:852:func (o *PersistOptions) SwitchRaftV2(storage endpoint.ConfigStorage) error {
server/config/persist_options.go:860:func (o *PersistOptions) Persist(storage endpoint.ConfigStorage) error {
server/config/persist_options.go:887:func (o *PersistOptions) Reload(storage endpoint.ConfigStorage) error {
server/config/persist_options.go:931:func (o *PersistOptions) CheckLabelProperty(typ string, labels []*metapb.StoreLabel) bool {
server/config/persist_options.go:944:func (o *PersistOptions) GetMinResolvedTSPersistenceInterval() time.Duration {
server/config/persist_options.go:949:func (o *PersistOptions) SetTTLData(parCtx context.Context, client *clientv3.Client, key string, value string, ttl time.Duration) error {
server/config/persist_options.go:1021:func (o *PersistOptions) GetTTLData(key string) (string, bool) {
server/config/persist_options.go:1032:func (o *PersistOptions) LoadTTLFromEtcd(ctx context.Context, client *clientv3.Client) error {
server/config/persist_options.go:1056:func (*PersistOptions) SetSchedulingAllowanceStatus(halt bool, source string) {
server/config/persist_options.go:1067:func (o *PersistOptions) SetHaltScheduling(halt bool, source string) {
server/config/persist_options.go:1075:func (o *PersistOptions) IsSchedulingHalted() bool {
server/config/persist_options.go:1083:func (o *PersistOptions) GetRegionMaxSize() uint64 {
server/config/persist_options.go:1088:func (o *PersistOptions) GetRegionMaxKeys() uint64 {
server/config/persist_options.go:1093:func (o *PersistOptions) GetRegionSplitSize() uint64 {
server/config/persist_options.go:1098:func (o *PersistOptions) GetRegionSplitKeys() uint64 {
server/config/persist_options.go:1103:func (o *PersistOptions) CheckRegionSize(size, mergeSize uint64) error {
server/config/persist_options.go:1108:func (o *PersistOptions) CheckRegionKeys(keys, mergeKeys uint64) error {
server/config/persist_options.go:1113:func (o *PersistOptions) IsEnableRegionBucket() bool {
server/config/persist_options.go:1118:func (o *PersistOptions) IsRaftKV2() bool {
server/config/persist_options.go:1123:func (o *PersistOptions) GetUnifiedReadPoolMaxThreadCount() uint64 {
server/config/persist_options.go:1129:func (o *PersistOptions) SetRegionBucketEnabled(enabled bool) {
server/config/persist_options.go:1136:func (o *PersistOptions) GetRegionBucketSize() uint64 {
server/config/config.go:50:type Config struct {
server/config/config.go:172:func NewConfig() *Config {
server/config/config.go:280:func (c *Config) Parse(flagSet *pflag.FlagSet) error {
server/config/config.go:334:func (c *Config) Validate() error {
server/config/config.go:358:func (c *Config) Adjust(meta *toml.MetaData, reloading bool) error {
server/config/config.go:494:func (c *Config) Clone() *Config {
server/config/config.go:499:func (c *Config) String() string {
server/config/config.go:509:type PDServerConfig struct {
server/config/config.go:615:func (c *PDServerConfig) Clone() *PDServerConfig {
server/config/config.go:623:func (c *PDServerConfig) Validate() error {
server/config/config.go:666:type StoreLabel struct {
server/config/config.go:673:type LabelPropertyConfig map[string][]StoreLabel
server/config/config.go:676:func (c LabelPropertyConfig) Clone() LabelPropertyConfig {
server/config/config.go:687:func (c *Config) GetLease() int64 {
server/config/config.go:693:func (c *Config) GetMaxConcurrentTSOProxyStreamings() int {
server/config/config.go:698:func (c *Config) GetTSOProxyRecvFromClientTimeout() time.Duration {
server/config/config.go:703:func (c *Config) GetTSOUpdatePhysicalInterval() time.Duration {
server/config/config.go:708:func (c *Config) GetTSOSaveInterval() time.Duration {
server/config/config.go:713:func (c *Config) GetTLSConfig() *grpcutil.TLSConfig {
server/config/config.go:718:func (c *Config) GenEmbedEtcdConfig() (*embed.Config, error) {
server/config/config.go:780:type DashboardConfig struct {
server/config/config.go:793:func (c *DashboardConfig) ToTiDBTLSConfig() (*tls.Config, error) {
server/config/config.go:817:type ReplicationModeConfig struct {
server/config/config.go:823:func (c *ReplicationModeConfig) Clone() *ReplicationModeConfig {
server/config/config.go:837:func NormalizeReplicationMode(m string) string {
server/config/config.go:846:type DRAutoSyncReplicationConfig struct {
server/config/config.go:864:type MicroserviceConfig struct {
server/config/config.go:886:func (c *MicroserviceConfig) Clone() *MicroserviceConfig {
server/config/config.go:892:func (c *MicroserviceConfig) IsSchedulingFallbackEnabled() bool {
server/config/config.go:897:func (c *MicroserviceConfig) IsTSODynamicSwitchingEnabled() bool {
server/config/config.go:902:func (c *MicroserviceConfig) IsResourceManagerFallbackEnabled() bool {
server/config/config.go:907:type KeyspaceConfig struct {
server/config/config.go:922:func (c *KeyspaceConfig) Validate() error {
server/config/config.go:951:func IsValidMetaServiceGroupID(id string) bool {
server/config/config.go:956:func AdjustMetaServiceGroups(metaGroups map[string]string) error {
server/config/config.go:987:func (c *KeyspaceConfig) Clone() *KeyspaceConfig {
server/config/config.go:1000:func (c *KeyspaceConfig) GetPreAlloc() []string {
server/config/config.go:1007:func (c *KeyspaceConfig) ToWaitRegionSplit() bool {
server/config/config.go:1012:func (c *KeyspaceConfig) GetWaitRegionSplitTimeout() time.Duration {
server/config/config.go:1017:func (c *KeyspaceConfig) GetCheckRegionSplitInterval() time.Duration {
server/config/config.go:1022:func (c *KeyspaceConfig) GetMetaServiceGroups() map[string]string {
server/config/config.go:1031:func (c *KeyspaceConfig) SetMetaServiceGroups(metaServiceGroups map[string]string) {
```

本任务只处理 persistedConfig/Persist/Reload/adjustScheduleCfg；Provider 已由批次 11
验收；UpdateScheduleConfig、SwitchRaftV2、TTL、发布/回退归后续批次 13；middleware
持久化另有 service_middleware_persist_options.go，不纳入普通配置 key。

依赖为 schedule/config、configutil、typeutil 和已有嵌套 Config 值 crate；本次编排
增加 endpoint/keypath，测试增加 storage/kv/Tokio。反向调用主要来自 server.go、
cluster、API、调度服务和 tests；生产 Rust 暂提供宿主调用 API，不接入服务启动。
相邻测试为 config_test.go、util_test.go；本任务对照 TestReloadConfig、
TestReloadDefaultStoreLimit、TestReloadLegacyStoreBalanceRate、TestReloadUpgrade/2、
TestGCConcurrencyConfigReload/RejectsInvalid。本次独立 Rust 文件 config_persist_test.rs。

persist_options.go 唯一 failpoint 为 persistFail，在实际 SaveConfig 之前返回错误；
本次不启停 Go failpoint，以真实停止 etcd 的 I/O 边界及编码失败验证错误传播。
指标在 metrics.go 中为调度 allowance gauge vector（kind），不在 Persist/Reload
路径注册或更新；本任务不新增指标。普通 config key 为 `/pd/{cluster_id}/config`，
TTL 独立 `/config/ttl`，middleware key 独立；后两者不在本任务写入。
Persist/Reload 自身无后台任务；TTL 的 context/cache/watch 生命周期归后续宿主。
本次 Rust 无后台任务，无全局状态，无私有 runtime，配置字段公开类型没有 proto。
