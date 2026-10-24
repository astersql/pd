# Placement 首次包审查

基准 `db1b56605b43b973afbd1eef8d03a7e7ba25e0bf` 至依赖提交 `ff8e4f28a`，本 Go 包无变化。只有根 AGENTS.md。

## config.go → 23 补丁/限制

```go
type ruleConfig struct
func newRuleConfig() *ruleConfig
func (c *ruleConfig) adjust()
func ruleMayRestrictEngine(rule *Rule, values ...string) bool
func (c *ruleConfig) getRule(key [2]string) *Rule
func (c *ruleConfig) iterateRules(f func(*Rule))
func (c *ruleConfig) setRule(r *Rule)
func (c *ruleConfig) setGroup(g *RuleGroup)
func (c *ruleConfig) getGroup(id string) *RuleGroup
func (c *ruleConfig) beginPatch() *RuleConfigPatch
type RuleConfigPatch struct
func (p *RuleConfigPatch) SetRule(r *Rule)
func (p *RuleConfigPatch) DeleteRule(group, id string)
func (p *RuleConfigPatch) getGroup(id string) *RuleGroup
func (p *RuleConfigPatch) SetGroup(g *RuleGroup)
func (p *RuleConfigPatch) DeleteGroup(id string)
func (p *RuleConfigPatch) iterateRules(f func(*Rule))
func (p *RuleConfigPatch) adjust()
func (p *RuleConfigPatch) trim()
func (p *RuleConfigPatch) commit()
func jsonEquals(a, b any) bool
```

直接 imports：`bytes`, `encoding/json`, `time`, `github.com/tikv/pd/pkg/core`。

## fit.go → 26 单规则角色；27 多规则搜索；28 隔离替换

```go
const replicaBaseScore = 100
type RegionFit struct
func (f *RegionFit) Replace(srcStoreID uint64, dstStore *core.StoreInfo) bool
func (f *RegionFit) getRuleFitByStoreID(storeID uint64) *RuleFit
func (f *RegionFit) IsSatisfied() bool
func (f *RegionFit) ExtraCount() int
func (f *RegionFit) GetRuleFit(peerID uint64) *RuleFit
func (f *RegionFit) GetRegionStores() []*core.StoreInfo
func (f *RegionFit) GetRules() []*Rule
type RuleFit struct
func (f *RuleFit) IsSatisfied() bool
func (f *RuleFit) contain(storeID uint64) bool
func compareRuleFit(a, b *RuleFit) int
type StoreSet interface
func fitRegion(stores []*core.StoreInfo, region *core.RegionInfo, rules []*Rule, supportWitness bool) *RegionFit
type fitWorker struct
func newFitPeer(stores []*core.StoreInfo, region *core.RegionInfo, fitPeers []*metapb.Peer) []*fitPeer
func newFitWorker(stores []*core.StoreInfo, region *core.RegionInfo, rules []*Rule, supportWitness bool) *fitWorker
func (w *fitWorker) run()
func (w *fitWorker) fitRule(index int) bool
func (w *fitWorker) fixRuleWithCandidates(candidates []*fitPeer, index int, count int) bool
func pickPeersFromBinaryInt(candidates []*fitPeer, binaryNumber uint) []*fitPeer
func unSelectPeers(selected []*fitPeer)
func (w *fitWorker) compareBest(selected []*fitPeer, index int) bool
func (w *fitWorker) updateOrphanPeers(index int)
func newRuleFit(rule *Rule, peers []*fitPeer, supportWitness bool) *RuleFit
type fitPeer struct
func (p *fitPeer) matchRoleStrict(role PeerRoleType) bool
func isolationStoreScore(srcStoreID uint64, dstStore *core.StoreInfo, stores []*core.StoreInfo, labels []string) float64
func isolationScore(peers []*fitPeer, labels []string) float64
func needIsolation(rules []*Rule) bool
func stateScore(region *core.RegionInfo, peerID uint64) int
func witnessScore(peers []*fitPeer, fitWitness bool) int
```

直接 imports：`math`, `math/bits`, `sort`, `github.com/pingcap/kvproto/pkg/metapb`, `github.com/tikv/pd/pkg/core`。

## label_constraint.go → 21 约束

```go
type LabelConstraintOp string
const (
func validateOp(op LabelConstraintOp) bool
type LabelConstraint struct
func (c *LabelConstraint) MatchStore(store *core.StoreInfo) bool
func (c *LabelConstraint) matchValue(value string) bool
var legacyExclusiveLabels = []string
func isExclusiveLabel(key string) bool
func MatchLabelConstraints(store *core.StoreInfo, constraints []LabelConstraint) bool
```

直接 imports：`strings`, `github.com/tikv/pd/pkg/core`, `github.com/tikv/pd/pkg/slice`。

## region_rule_cache.go → 29 缓存失效

```go
const (
type RegionRuleFitCacheManager struct
func NewRegionRuleFitCacheManager() *RegionRuleFitCacheManager
func (manager *RegionRuleFitCacheManager) RemoveStoreCache(storeID uint64)
func (manager *RegionRuleFitCacheManager) Invalid(regionID uint64)
func (manager *RegionRuleFitCacheManager) removeRegionCacheLocked(regionID uint64)
func (manager *RegionRuleFitCacheManager) CheckAndGetCache(region *core.RegionInfo,
func (manager *RegionRuleFitCacheManager) SetCache(storeSet StoreSet, region *core.RegionInfo, fit *RegionFit)
func allStoresLive(storeSet StoreSet, stores []*core.StoreInfo) bool
type regionRuleFitCache struct
func (cache *regionRuleFitCache) IsUnchanged(region *core.RegionInfo, rules []*Rule, stores []*core.StoreInfo) bool
func (cache *regionRuleFitCache) isRegionUnchanged(region *core.RegionInfo) bool
func rulesEqual(ruleCaches []ruleCache, rules []*Rule) bool
func storesEqual(a []*storeCache, b []*core.StoreInfo) bool
func (manager *RegionRuleFitCacheManager) toRegionRuleFitCache(storeSet StoreSet, region *core.RegionInfo, fit *RegionFit) (*regionRuleFitCache, bool)
type ruleCache struct
func (r ruleCache) ruleEqual(rule *Rule) bool
func toRuleCacheList(rules []*Rule) (c []ruleCache)
type storeCache struct
func (s storeCache) storeEqual(store *core.StoreInfo) bool
func (manager *RegionRuleFitCacheManager) toStoreCacheList(storeSet StoreSet, stores []*core.StoreInfo) (c []*storeCache, cacheable bool)
func labelEqual(label1 map[string]string, label2 []*metapb.StoreLabel) bool
type regionCache struct
func (r regionCache) epochEqual(region *core.RegionInfo) bool
func toRegionCache(r *core.RegionInfo) regionCache
func ValidateStores(stores []*core.StoreInfo) bool
func ValidateRegion(region *core.RegionInfo) bool
func ValidateFit(fit *RegionFit) bool
```

直接 imports：`github.com/pingcap/kvproto/pkg/metapb`, `github.com/tikv/pd/pkg/core`, `github.com/tikv/pd/pkg/slice`, `github.com/tikv/pd/pkg/utils/syncutil`。

## rule.go → 21 值；22 排序覆盖

```go
type PeerRoleType string
const (
func validateRole(s PeerRoleType) bool
func (s PeerRoleType) MetaPeerRole() metapb.PeerRole
type Rule struct
func NewRuleFromJSON(data []byte) (*Rule, error)
func (r *Rule) String() string
func (r *Rule) Clone() *Rule
func (r *Rule) Key() [2]string
func (r *Rule) StoreKey() string
func (r *Rule) groupIndex() int
type RuleGroup struct
func NewRuleGroupFromJSON(data []byte) (*RuleGroup, error)
func (g *RuleGroup) isDefault() bool
func (g *RuleGroup) String() string
func (g *RuleGroup) Clone() *RuleGroup
func compareRule(a, b *Rule) int
func sortRules(rules []*Rule)
func prepareRulesForApply(rules []*Rule) []*Rule
type GroupBundle struct
func (g GroupBundle) String() string
```

直接 imports：`encoding/hex`, `encoding/json`, `slices`, `sort`, `github.com/pingcap/kvproto/pkg/metapb`。

## rule_list.go → 22 列表

```go
func checkApplyRules(rules []*Rule) error
type rangeRules struct
type ruleList struct
type ruleContainer interface
func buildRuleList(rules ruleContainer) (ruleList, error)
func (rl ruleList) getRulesByKey(key []byte) []*Rule
func (rl ruleList) getRulesForApplyRange(start, end []byte) []*Rule
```

直接 imports：`encoding/hex`, `fmt`, `strings`, `github.com/pingcap/errors`, `github.com/tikv/pd/pkg/errs`, `github.com/tikv/pd/pkg/schedule/rangelist`。

## rule_manager.go → 24 加载单条；25 批量事务；30 查询集成

```go
const (
type RuleManager struct
func NewRuleManager(ctx context.Context, storage endpoint.RuleStorage, storeSetInformer core.StoreSetInformer, conf config.SharedConfigProvider) *RuleManager
func (m *RuleManager) Initialize(maxReplica int, locationLabels []string, isolationLevel string, skipLoadRules bool) error
func (m *RuleManager) loadRules() error
func (m *RuleManager) loadGroups() error
func (m *RuleManager) AdjustRule(r *Rule, groupID string) (err error)
func (m *RuleManager) GetRule(group, id string) *Rule
func (m *RuleManager) GetRuleLocked(group, id string) *Rule
func (m *RuleManager) SetRule(rule *Rule) error
func (m *RuleManager) DeleteRule(group, id string) error
func (m *RuleManager) GetSplitKeys(start, end []byte) [][]byte
func (m *RuleManager) GetAllRules() []*Rule
func (m *RuleManager) GetRulesCount() int
func (m *RuleManager) MayRestrictStoreLoad(isTiKV bool) bool
func (m *RuleManager) updateStoreLoadRestrictions()
func (m *RuleManager) GetGroupsCount() int
func (m *RuleManager) GetRuleConfigForReconcile() ([]*Rule, []*RuleGroup)
func (m *RuleManager) GetRulesByGroup(group string) []*Rule
func (m *RuleManager) GetRulesByGroupLocked(group string) []*Rule
func (m *RuleManager) GetRulesByKey(key []byte) []*Rule
func (m *RuleManager) GetRulesForApplyRegion(region *core.RegionInfo) []*Rule
func (m *RuleManager) GetRulesForApplyRange(start, end []byte) []*Rule
func (m *RuleManager) IsRegionFitCached(storeSet StoreSet, region *core.RegionInfo) bool
func (m *RuleManager) FitRegionWithoutCache(storeSet StoreSet, region *core.RegionInfo) (fit *RegionFit)
func (m *RuleManager) FitRegion(storeSet StoreSet, region *core.RegionInfo) (fit *RegionFit)
func (m *RuleManager) SetRegionFitCache(storeSet StoreSet, region *core.RegionInfo, fit *RegionFit)
func (m *RuleManager) InvalidCache(regionID uint64)
func (m *RuleManager) RemoveStoreCache(storeID uint64)
func (m *RuleManager) SetPlaceholderRegionFitCache(region *core.RegionInfo)
func (m *RuleManager) CheckIsCachedDirectly(regionID uint64) bool
func (m *RuleManager) BeginPatch() *RuleConfigPatch
func (m *RuleManager) TryCommitPatchLocked(patch *RuleConfigPatch) error
func (m *RuleManager) savePatch(p *ruleConfig) error
func (m *RuleManager) SetRules(rules []*Rule) error
type RuleOpType string
const (
type RuleOp struct
func (r RuleOp) String() string
func (m *RuleManager) Batch(todo []RuleOp) error
func (m *RuleManager) GetRuleGroup(id string) *RuleGroup
func (m *RuleManager) GetRuleGroups() []*RuleGroup
func (m *RuleManager) SetRuleGroup(group *RuleGroup) error
func (m *RuleManager) DeleteRuleGroup(id string) error
func (m *RuleManager) GetAllGroupBundles() []GroupBundle
func (m *RuleManager) GetGroupBundle(id string) (b GroupBundle)
func (m *RuleManager) SetAllGroupBundles(groups []GroupBundle, override bool) error
func (m *RuleManager) SetGroupBundle(group GroupBundle) error
func (m *RuleManager) DeleteGroupBundle(id string, regex bool) error
func (m *RuleManager) IsInitialized() bool
func checkRule(rule *Rule, stores []*core.StoreInfo) bool
func (m *RuleManager) SetKeyType(h string) *RuleManager
func getStoresByRegion(storeSet StoreSet, region *core.RegionInfo) []*core.StoreInfo
func getStoreByID(stores []*core.StoreInfo, id uint64) *core.StoreInfo
```

直接 imports：`bytes`, `context`, `encoding/hex`, `encoding/json`, `fmt`, `regexp`, `slices`, `sort`, `strings`, `sync/atomic`, `go.uber.org/zap`, `github.com/pingcap/errors`, `github.com/pingcap/log`, `github.com/tikv/pd/pkg/codec`, `github.com/tikv/pd/pkg/core`, `github.com/tikv/pd/pkg/core/constant`, `github.com/tikv/pd/pkg/errs`, `github.com/tikv/pd/pkg/schedule/config`, `github.com/tikv/pd/pkg/schedule/rangelist`, `github.com/tikv/pd/pkg/slice`, `github.com/tikv/pd/pkg/storage/endpoint`, `github.com/tikv/pd/pkg/storage/kv`, `github.com/tikv/pd/pkg/utils/syncutil`。

## 测试和反向依赖

测试文件：`config_test.go`, `fit_region_test.go`, `fit_test.go`, `label_constraint_test.go`, `region_rule_cache_test.go`, `rule_manager_test.go`, `rule_test.go`。

反向依赖（含工具与测试）：

- `./pkg/mock/mockcluster/config.go`
- `./pkg/mock/mockcluster/mockcluster.go`
- `./pkg/mcs/scheduling/server/cluster.go`
- `./pkg/statistics/region_collection_test.go`
- `./pkg/mcs/scheduling/server/rule/watcher.go`
- `./pkg/mcs/scheduling/server/rule/watcher_test.go`
- `./server/api/rule.go`
- `./pkg/statistics/region_collection.go`
- `./pkg/replication/replication_mode.go`
- `./pkg/cluster/cluster.go`
- `./pkg/replication/replication_mode_test.go`
- `./pkg/schedule/core/cluster_informer.go`
- `./server/server.go`
- `./tests/testutil.go`
- `./pkg/schedule/handler/handler.go`
- `./pkg/schedule/checker/rule_checker.go`
- `./pkg/schedule/checker/replica_strategy.go`
- `./pkg/schedule/checker/split_checker_test.go`
- `./pkg/schedule/checker/priority_inspector.go`
- `./pkg/schedule/checker/merge_checker_test.go`
- `./pkg/schedule/checker/affinity_checker_test.go`
- `./server/cluster/cluster.go`
- `./pkg/schedule/checker/split_checker.go`
- `./pkg/schedule/checker/affinity_checker.go`
- `./pkg/schedule/checker/rule_checker_test.go`
- `./pkg/schedule/checker/merge_checker.go`
- `./server/cluster/cluster_test.go`
- `./pkg/schedule/schedulers/shuffle_region_config.go`
- `./pkg/schedule/schedulers/utils_test.go`
- `./pkg/schedule/schedulers/hot_region_rank_v2_test.go`
- `./pkg/schedule/schedulers/hot_region_solver.go`
- `./pkg/schedule/schedulers/balance_range_test.go`
- `./pkg/schedule/schedulers/scheduler_test.go`
- `./pkg/schedule/schedulers/balance_witness_test.go`
- `./pkg/schedule/schedulers/utils.go`
- `./pkg/schedule/schedulers/balance_benchmark_test.go`
- `./pkg/schedule/scatter/region_scatterer_test.go`
- `./server/cluster/scheduling_controller.go`
- `./pkg/schedule/schedulers/hot_region_test.go`
- `./pkg/schedule/scatter/region_scatterer.go`
- `./pkg/schedule/operator/create_operator.go`
- `./pkg/schedule/operator/create_operator_test.go`
- `./pkg/schedule/filter/region_filters.go`
- `./tools/pd-simulator/simulator/cases/diagnose_rule.go`
- `./pkg/schedule/operator/builder.go`
- `./pkg/schedule/schedulers/hot_region_solver_test.go`
- `./pkg/schedule/filter/filters.go`
- `./tests/integrations/mcs/scheduling/api_test.go`
- `./tools/pd-ctl/pdctl/command/config_command.go`
- `./tests/integrations/mcs/scheduling/rule_test.go`
- `./tests/integrations/client/http_client_test.go`
- `./pkg/schedule/filter/filters_test.go`
- `./tools/pd-ctl/tests/config/config_test.go`
- `./tools/pd-ctl/tests/keyspace/keyspace_test.go`
- `./tests/server/api/cluster_test.go`
- `./tests/server/api/operator_test.go`
- `./tests/server/api/region_test.go`
- `./tests/server/api/rule_test.go`

## 副作用

包内无 failpoint 注入、指标注册或自启后台 goroutine/ticker。RuleManager 使用外部 context/锁/endpoint.RuleStorage/宿主 store informer。规则 key `/pd/{cluster_id}/rules/{hex(group)}-{hex(id)}`；组 key `/pd/{cluster_id}/rule_group/{id}`。本任务不创建 Rust 持久化行为。

Rust 仅依赖 core、serde、serde_json；无 protobuf/transport 或 statistics/filter/operator/schedulers/server/runtime。MetaPeerRole 转换留在协议适配层。

## 分组常量补充

`PeerRoleType`: Voter/Leader/Follower/Learner；`LabelConstraintOp`: In/NotIn/Exists/NotExists；RuleManager: DefaultGroupID/DefaultRuleID（defaultWitnessRuleID 私有）。
