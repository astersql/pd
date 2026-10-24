# 批次 9 包级只读盘点

基准：`db1b56605b43b973afbd1eef8d03a7e7ba25e0bf`。目标 Go 包与基准无差异。

## config.go

```go
	DefaultMaxReplicas         = 3
	DefaultSplitMergeInterval      = time.Hour
	RegionSizeToKeysRatio = 10000
	DefaultStoreLimit = StoreLimit{AddPeer: 15, RemovePeer: 15, TransferLeaderIn: storelimit.Unlimited}
	DefaultTiFlashStoreLimit = StoreLimit{AddPeer: 30, RemovePeer: 30, TransferLeaderIn: storelimit.Unlimited}
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
	EnableTiKVSplitRegion = "schedule.enable-tikv-split-region"
	DefaultGCInterval = 5 * time.Second
	DefaultTTL        = 5 * time.Minute
type StoreLimit struct {
func (sl *StoreLimit) SetDefaultStoreLimit(typ storelimit.Type, ratePerMin float64) {
func (sl *StoreLimit) GetDefaultStoreLimit(typ storelimit.Type) float64 {
type ScheduleConfig struct {
func (c *ScheduleConfig) Clone() *ScheduleConfig {
func (c *ScheduleConfig) Adjust(meta *configutil.ConfigMetaData, reloading bool) error {
func (c *ScheduleConfig) GetMaxMergeRegionKeys() uint64 {
func (c *ScheduleConfig) GetMaxAffinityMergeRegionSize() uint64 {
func (c *ScheduleConfig) UnmarshalJSON(data []byte) error {
func (c *ScheduleConfig) MigrateDeprecatedFlags() {
func (c *ScheduleConfig) MigrateDeprecatedFlagsFromJSON(data []byte) error {
func (c *ScheduleConfig) Validate() error {
func (c *ScheduleConfig) Deprecated() error {
type StoreLimitConfig struct {
func (c StoreLimitConfig) SetLimit(typ storelimit.Type, ratePerMin float64) StoreLimitConfig {
func DefaultStoreLimitConfig() StoreLimitConfig {
func (c *ScheduleConfig) GetDefaultStoreLimit() StoreLimitConfig {
type SchedulerConfigs []SchedulerConfig
type SchedulerConfig struct {
var DefaultSchedulers = defaultSchedulersInit()
func IsDefaultScheduler(typ string) bool {
type ReplicationConfig struct {
func (c *ReplicationConfig) Clone() *ReplicationConfig {
func (c *ReplicationConfig) Validate() error {
func (c *ReplicationConfig) Adjust(meta *configutil.ConfigMetaData) error {
```

## config_provider.go

```go
const RejectLeader = "reject-leader"
func RegisterScheduler(typ types.CheckerSchedulerType) {
type SchedulerConfigProvider interface {
	SetSchedulingAllowanceStatus(bool, string)
	GetStoresLimit() map[uint64]StoreLimitConfig
	IsSchedulerDisabled(types.CheckerSchedulerType) bool
	AddSchedulerCfg(types.CheckerSchedulerType, []string)
	RemoveSchedulerCfg(types.CheckerSchedulerType)
	Persist(endpoint.ConfigStorage) error
	GetRegionScheduleLimit() uint64
	GetLeaderScheduleLimit() uint64
	GetHotRegionScheduleLimit() uint64
	GetWitnessScheduleLimit() uint64
	GetHotRegionCacheHitsThreshold() int
	GetMaxMovableHotPeerSize() int64
	IsTraceRegionFlow() bool
	GetTolerantSizeRatio() float64
	GetLeaderSchedulePolicy() constant.SchedulePolicy
	IsDebugMetricsEnabled() bool
	IsDiagnosticAllowed() bool
	GetSlowStoreEvictingAffectedStoreRatioThreshold() float64
	GetScheduleConfig() *ScheduleConfig
	SetScheduleConfig(*ScheduleConfig)
	SetSchedulers(SchedulerConfigs)
type CheckerConfigProvider interface {
	GetSwitchWitnessInterval() time.Duration
	IsRemoveExtraReplicaEnabled() bool
	IsRemoveDownReplicaEnabled() bool
	IsReplaceOfflineReplicaEnabled() bool
	IsMakeUpReplicaEnabled() bool
	IsLocationReplacementEnabled() bool
	GetIsolationLevel() string
	GetSplitMergeInterval() time.Duration
	GetPatrolRegionInterval() time.Duration
	GetPatrolRegionWorkerCount() int
	GetMaxMergeRegionSize() uint64
	GetMaxMergeRegionKeys() uint64
	GetReplicaScheduleLimit() uint64
	GetRegionScheduleLimit() uint64
type SharedConfigProvider interface {
	GetMaxReplicas() int
	IsPlacementRulesEnabled() bool
	GetAffinityScheduleLimit() uint64
	GetSplitScatterScheduleLimit() uint64
	GetMaxAffinityMergeRegionSize() uint64
	GetMaxSnapshotCount() uint64
	GetMaxPendingPeerCount() uint64
	GetLowSpaceRatio() float64
	GetHighSpaceRatio() float64
	GetMaxStoreDownTime() time.Duration
	GetLocationLabels() []string
	CheckLabelProperty(string, []*metapb.StoreLabel) bool
	GetClusterVersion() *semver.Version
	IsUseJointConsensus() bool
	GetKeyType() constant.KeyType
	IsCrossTableMergeEnabled() bool
	IsOneWayMergeEnabled() bool
	GetMergeScheduleLimit() uint64
	GetRegionScoreFormulaVersion() string
	GetSchedulerMaxWaitingOperator() uint64
	GetStoreLimitByType(uint64, storelimit.Type) float64
	IsWitnessAllowed() bool
	IsPlacementRulesCacheEnabled() bool
	SetHaltScheduling(bool, string)
	GetHotRegionCacheHitsThreshold() int
	SetPlacementRuleEnabled(bool)
	SetPlacementRulesCacheEnabled(bool)
	SetEnableWitness(bool)
type ConfProvider interface {
	SetPlacementRuleEnabled(bool)
	SetSplitMergeInterval(time.Duration)
	SetMaxReplicas(int)
	SetAllStoresLimit(storelimit.Type, float64)
type StoreConfigProvider interface {
	GetRegionMaxSize() uint64
	GetRegionMaxKeys() uint64
	GetRegionSplitSize() uint64
	GetRegionSplitKeys() uint64
	CheckRegionSize(uint64, uint64) error
	CheckRegionKeys(uint64, uint64) error
	IsEnableRegionBucket() bool
	IsRaftKV2() bool
	GetUnifiedReadPoolMaxThreadCount() uint64
```

## store_config.go

```go
	RaftstoreV2 = "raft-kv2"
type StoreConfig struct {
type Storage struct {
type ReadPool struct {
type UnifiedReadPool struct {
type Coprocessor struct {
func (c *StoreConfig) Adjust() {
func (c *StoreConfig) Equal(other *StoreConfig) bool {
func (c *StoreConfig) String() string {
func (c *StoreConfig) GetRegionMaxSize() uint64 {
func (c *StoreConfig) GetRegionSplitSize() uint64 {
func (c *StoreConfig) GetRegionSplitKeys() uint64 {
func (c *StoreConfig) GetRegionMaxKeys() uint64 {
func (c *StoreConfig) IsEnableRegionBucket() bool {
func (c *StoreConfig) IsRaftKV2() bool {
func (c *StoreConfig) GetUnifiedReadPoolMaxThreadCount() uint64 {
func (c *StoreConfig) SetRegionBucketEnabled(enabled bool) {
func (c *StoreConfig) GetRegionBucketSize() uint64 {
func (c *StoreConfig) CheckRegionSize(size, mergeSize uint64) error {
func (c *StoreConfig) CheckRegionKeys(keys, mergeKeys uint64) error {
func (c *StoreConfig) Clone() *StoreConfig {
```

## util.go

```go
func ValidateLabels(labels []*metapb.StoreLabel) error {
func ValidateLabelKey(key string) error {
```

## 任务映射与调用边界

批次 9：ScheduleConfig、ReplicationConfig、StoreLimitConfig、SchedulerConfig/SchedulerConfigs 的值、序列化、默认、校验、克隆及 util 标签语法。
批次 10：StoreConfig/Storage/ReadPool/UnifiedReadPool/Coprocessor 与合并阈值。批次 11：provider 接口与 scheduler 注册集合。批次 12/13：server/config 持久化、重载与发布回退编排。

Go 依赖：encoding/json、math、time、regexp；pingcap/errors、kvproto/metapb；core/storelimit、schedule/types、configutil、syncutil、typeutil、kerneltype、storage/endpoint。
反向调用包含 server/config、server/api、server/cluster、mcs/scheduling、schedule/{placement,checker,filter,schedulers}、statistics、unsaferecovery、mock 与 pd-ctl/simulator。这些不是 Rust 反向依赖许可。
Go 测试：store_config_test.go（TestMain/TestMergeCheck/TestStoreConfigEqualChecksReadPool）、util_test.go（TestValidateLabels）；TestMain 调用 WaitForEtcdConnections/goleak，本任务未运行。
包内无 failpoint、指标或后台任务；TTLConfigPrefix=/config/ttl 是常量持久 key 前缀，实际持久化不在本任务。Go 的 schedulerMap 与默认 StoreLimit 是既有进程状态，Rust 不引入对应全局状态。
Rust 原有 config_test.rs 的五项测试保留；新增独立 config_values_test.rs 七项。
