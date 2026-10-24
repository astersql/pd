# Package inventory

Baseline `db1b56605b43b973afbd1eef8d03a7e7ba25e0bf`; Go package diff from baseline is empty.

## keyspace_index.go

```text
type keyspaceRuleChunk struct {
type keyspaceRuleSet struct {
func (s *keyspaceRuleSet) get(id uint32) *LabelRule {
func (s *keyspaceRuleSet) set(id uint32, rule *LabelRule) {
func (s *keyspaceRuleSet) replace(id uint32, rule *LabelRule) {
func (s *keyspaceRuleSet) clear(id uint32) {
func (s *keyspaceRuleSet) forEachSlot(lo, hi int, fn func(id uint32) bool) {
func (s *keyspaceRuleSet) forEachBoundary(lo, hi int, fn func(id uint32) bool) {
type keyspaceRuleRange struct {
type keyspaceRuleIndex struct {
func (i *keyspaceRuleIndex) isEmpty() bool {
func (i *keyspaceRuleIndex) ruleSet(mode byte) *keyspaceRuleSet {
func (i *keyspaceRuleIndex) Add(rule *LabelRule) bool {
func (i *keyspaceRuleIndex) Replace(old, rule *LabelRule) bool {
func (i *keyspaceRuleIndex) Remove(ruleID string, rule *LabelRule) bool {
func (i *keyspaceRuleIndex) Contains(rule *LabelRule) bool {
func (i *keyspaceRuleIndex) GetRule(start, end []byte) *LabelRule {
func (i *keyspaceRuleIndex) matchRule(start, end []byte) (*LabelRule, bool) {
func (i *keyspaceRuleIndex) HasSplitKey(start, end []byte) bool {
func (i *keyspaceRuleIndex) GetSplitKeys(start, end []byte) [][]byte {
func keyspaceRanges(rule *LabelRule) ([2]keyspaceRuleRange, int, bool) {
func parseKeyspaceRuleID(ruleID string) (uint32, string, bool) {
func canonicalKeyspaceRange(keyRange *KeyRangeRule, id uint32) (byte, bool) {
func keyspaceBoundary(mode byte, id uint32) [9]byte {
func keyspaceBoundaryBytes(mode byte, id uint32) []byte {
func keyspaceIntervalForKey(key []byte) (byte, uint32, bool) {
func keyspaceBoundaryRange(mode byte, start, end []byte) (lo, hi int) {
func keyspaceModeOverlapsRange(mode byte, start, end []byte) bool {
func keyspaceModesOverlapRange(start, end []byte) bool {
func keyspaceBoundaryBound(mode byte, key []byte, upper bool) int {
func mergeSplitKeys(left, right [][]byte) [][]byte {
```

## labeler.go

```text
type RegionLabeler struct {
func (l *RegionLabeler) Unlock() {
func NewRegionLabeler(ctx context.Context, storage endpoint.RuleStorage, gcInterval time.Duration) (*RegionLabeler, error) {
func (l *RegionLabeler) doGC(gcInterval time.Duration) {
func (l *RegionLabeler) checkAndClearExpiredLabels() {
func (l *RegionLabeler) loadRules() error {
func (l *RegionLabeler) BuildRangeListLocked() {
func (l *RegionLabeler) GetSplitKeys(start, end []byte) [][]byte {
func filterExpiredLabels(rule *LabelRule, now time.Time) *LabelRule {
func (l *RegionLabeler) GetAllLabelRules() []*LabelRule {
func (l *RegionLabeler) GetRuleAndKeyRangeCounts() (ruleCount, keyRangeCount int) {
func (l *RegionLabeler) GetLabelRules(ids []string) ([]*LabelRule, error) {
func (l *RegionLabeler) GetLabelRule(id string) *LabelRule {
func (l *RegionLabeler) GetLabelRuleLocked(id string) *LabelRule {
func (l *RegionLabeler) SetLabelRule(rule *LabelRule) error {
func (l *RegionLabeler) SetLabelRuleLocked(rule *LabelRule) error {
func (l *RegionLabeler) DeleteLabelRule(id string) error {
func (l *RegionLabeler) DeleteLabelRuleLocked(id string) error {
func (l *RegionLabeler) Patch(patch LabelRulePatch) error {
func (l *RegionLabeler) GetRegionLabel(region *core.RegionInfo, key string) string {
func (l *RegionLabeler) ScheduleDisabled(region *core.RegionInfo) bool {
func (l *RegionLabeler) GetRegionLabels(region *core.RegionInfo) []*RegionLabel {
func MakeKeyRanges(keys ...string) []any {
func (l *RegionLabeler) IterateLabelRules(iterator func(rule *LabelRule) bool) {
```

## metrics.go

```text
	LabelerEventCounter = prometheus.NewCounterVec(
func init() {
```

## plan.go

```text
type Plan struct {
func (l *RegionLabeler) NewPlan() *Plan {
func (p *Plan) SetLabelRule(rule *LabelRule) error {
func (p *Plan) DeleteLabelRule(id string) error {
func (p *Plan) CommitOps() []func(txn kv.Txn) error {
func (p *Plan) Apply() {
```

## rule_index.go

```text
type labelRuleIndex struct {
func newLabelRuleIndex(rules map[string]*LabelRule) labelRuleIndex {
func (i *labelRuleIndex) set(rule *LabelRule) {
func (i *labelRuleIndex) delete(id string) {
func (i *labelRuleIndex) removeExpiredLabels(rule *LabelRule, now time.Time) bool {
func (i *labelRuleIndex) buildRanges() {
func (i *labelRuleIndex) getSplitKeys(start, end []byte) [][]byte {
func (i *labelRuleIndex) getRangeRules(start, end []byte) ([]any, *LabelRule, bool) {
```

## rules.go

```text
type RegionLabel struct {
func (l *RegionLabel) String() string {
type LabelRule struct {
func (rule *LabelRule) String() string {
func (rule *LabelRule) GetKeyRanges() []*KeyRangeRule {
func NewLabelRuleFromJSON(data []byte) (*LabelRule, error) {
func NewLabelRuleFromJSONWithoutCheck(data []byte) (*LabelRule, error) {
	KeyRange = "key-range"
type KeyRangeRule struct {
type LabelRulePatch struct {
func (l *RegionLabel) expireBefore(t time.Time) bool {
func (l *RegionLabel) checkAndAdjustExpire() (err error) {
func (rule *LabelRule) checkAndRemoveExpireLabels(now time.Time) bool {
func (rule *LabelRule) checkAndAdjust() error {
func (rule *LabelRule) expireBefore(t time.Time) bool {
func initKeyRangeRulesFromLabelRuleData(data any) ([]*KeyRangeRule, error) {
func initAndAdjustKeyRangeRule(rule any) (*KeyRangeRule, error) {
```

## Imports

```text
pkg/schedule/labeler/keyspace_index.go:23:	"github.com/tikv/pd/pkg/codec"
pkg/schedule/labeler/keyspace_index.go:24:	"github.com/tikv/pd/pkg/keyspace/constant"
pkg/schedule/labeler/rule_test.go:23:	"github.com/stretchr/testify/require"
pkg/schedule/labeler/rule_index.go:20:	"github.com/tikv/pd/pkg/schedule/rangelist"
pkg/schedule/labeler/keyspace_index_test.go:27:	"github.com/stretchr/testify/require"
pkg/schedule/labeler/keyspace_index_test.go:29:	"github.com/tikv/pd/pkg/codec"
pkg/schedule/labeler/keyspace_index_test.go:30:	"github.com/tikv/pd/pkg/core"
pkg/schedule/labeler/keyspace_index_test.go:31:	"github.com/tikv/pd/pkg/keyspace/constant"
pkg/schedule/labeler/keyspace_index_test.go:32:	"github.com/tikv/pd/pkg/schedule/rangelist"
pkg/schedule/labeler/keyspace_index_test.go:33:	"github.com/tikv/pd/pkg/storage/endpoint"
pkg/schedule/labeler/keyspace_index_test.go:34:	"github.com/tikv/pd/pkg/storage/kv"
pkg/schedule/labeler/plan_test.go:22:	"github.com/stretchr/testify/require"
pkg/schedule/labeler/plan_test.go:24:	"github.com/tikv/pd/pkg/errs"
pkg/schedule/labeler/plan_test.go:25:	"github.com/tikv/pd/pkg/storage/endpoint"
pkg/schedule/labeler/plan_test.go:26:	"github.com/tikv/pd/pkg/storage/kv"
pkg/schedule/labeler/plan.go:18:	"github.com/tikv/pd/pkg/storage/kv"
pkg/schedule/labeler/metrics.go:17:import "github.com/prometheus/client_golang/prometheus"
pkg/schedule/labeler/labeler.go:24:	"github.com/pingcap/log"
pkg/schedule/labeler/labeler.go:26:	"github.com/tikv/pd/pkg/core"
pkg/schedule/labeler/labeler.go:27:	"github.com/tikv/pd/pkg/errs"
pkg/schedule/labeler/labeler.go:28:	"github.com/tikv/pd/pkg/storage/endpoint"
pkg/schedule/labeler/labeler.go:29:	"github.com/tikv/pd/pkg/storage/kv"
pkg/schedule/labeler/labeler.go:30:	"github.com/tikv/pd/pkg/utils/keyutil"
pkg/schedule/labeler/labeler.go:31:	"github.com/tikv/pd/pkg/utils/logutil"
pkg/schedule/labeler/labeler.go:32:	"github.com/tikv/pd/pkg/utils/syncutil"
pkg/schedule/labeler/rules.go:28:	"github.com/pingcap/failpoint"
pkg/schedule/labeler/rules.go:29:	"github.com/pingcap/log"
pkg/schedule/labeler/rules.go:31:	"github.com/tikv/pd/pkg/errs"
pkg/schedule/labeler/labeler_test.go:27:	"github.com/stretchr/testify/require"
pkg/schedule/labeler/labeler_test.go:30:	"github.com/pingcap/failpoint"
pkg/schedule/labeler/labeler_test.go:32:	"github.com/tikv/pd/pkg/core"
pkg/schedule/labeler/labeler_test.go:33:	"github.com/tikv/pd/pkg/storage"
pkg/schedule/labeler/labeler_test.go:34:	"github.com/tikv/pd/pkg/storage/endpoint"
pkg/schedule/labeler/labeler_test.go:35:	"github.com/tikv/pd/pkg/storage/kv"
pkg/schedule/labeler/labeler_test.go:36:	"github.com/tikv/pd/pkg/utils/etcdutil"
pkg/schedule/labeler/labeler_test.go:37:	"github.com/tikv/pd/pkg/utils/testutil"
pkg/schedule/labeler/labeler_test.go:429:	re.NoError(failpoint.Enable("github.com/tikv/pd/pkg/schedule/labeler/regionLabelExpireSub1Minute", "return(true)"))
pkg/schedule/labeler/labeler_test.go:438:	re.NoError(failpoint.Disable("github.com/tikv/pd/pkg/schedule/labeler/regionLabelExpireSub1Minute"))
pkg/schedule/labeler/labeler_test.go:466:	re.NoError(failpoint.Enable("github.com/tikv/pd/pkg/schedule/labeler/regionLabelExpireSub1Minute", "return(true)"))
pkg/schedule/labeler/labeler_test.go:468:		re.NoError(failpoint.Disable("github.com/tikv/pd/pkg/schedule/labeler/regionLabelExpireSub1Minute"))
```

## Reverse imports

```text
tests/integrations/client/http_client_test.go
tests/integrations/mcs/scheduling/api_test.go
tests/integrations/mcs/scheduling/rule_test.go
tests/server/api/rule_test.go
tests/server/api/region_label_test.go
tests/server/keyspace/keyspace_test.go
tests/integrations/realcluster/scheduler_test.go
server/cluster/split_scatter_test.go
server/cluster/metering_test.go
server/cluster/cluster.go
server/cluster/cluster_test.go
server/api/region_label.go
pkg/mcs/scheduling/server/cluster.go
pkg/mcs/scheduling/server/rule/watcher.go
pkg/mock/mockcluster/mockcluster.go
pkg/mcs/scheduling/server/rule/watcher_test.go
pkg/mcs/scheduling/server/affinity/watcher.go
pkg/mcs/scheduling/server/affinity/watcher_test.go
pkg/schedule/core/cluster_informer.go
pkg/schedule/affinity/txn_test.go
pkg/schedule/labeler/labeler_test.go
pkg/schedule/handler/handler.go
pkg/schedule/affinity/manager.go
pkg/schedule/affinity/group_test.go
pkg/schedule/affinity/group.go
pkg/schedule/affinity/manager_test.go
pkg/schedule/affinity/split_test.go
pkg/keyspace/util.go
pkg/schedule/affinity/txn.go
pkg/schedule/affinity/policy_test.go
pkg/keyspace/util_test.go
pkg/schedule/checker/split_scatter_test.go
pkg/keyspace/keyspace.go
pkg/schedule/checker/checker_controller.go
pkg/schedule/schedulers/scheduler_controller.go
pkg/schedule/checker/merge_checker.go
pkg/schedule/checker/merge_checker_test.go
pkg/schedule/checker/split_checker.go
pkg/schedule/checker/affinity_checker_test.go
pkg/schedule/checker/split_checker_test.go
pkg/schedule/operator/operator_controller_test.go
```

## Tests

```text
pkg/schedule/labeler/rule_test.go:26:func TestRegionLabelTTL(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:61:func TestKeyspaceRuleIndex(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:112:func TestKeyspaceRuleIndexRejectsNonCanonicalID(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:123:func TestKeyspaceRuleIndexBoundaries(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:154:func TestKeyspaceRuleIndexMatchesIncompletePrefix(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:220:func TestKeyspaceRuleIndexSparseGap(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:249:func TestKeyspaceBoundaryBoundMatchesBinarySearch(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:284:func TestKeyspaceRuleSetUsesSparseChunks(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:308:func TestKeyspaceRuleSetIteratesChunkBitmapBoundaries(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:339:func TestKeyspaceRuleIndexReplaceReusesSparseStorage(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:396:func TestKeyspaceRuleIndexPreservesLegacyResults(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:479:func TestRegionLabelerUpdatesKeyspaceRulesIncrementally(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:542:func TestRegionLabelerUpdatesMutatedKeyspaceRule(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:564:func TestNewLabelRuleIndexBuildsAuthoritativeSnapshot(t *testing.T) {
pkg/schedule/labeler/keyspace_index_test.go:587:func BenchmarkKeyspaceRuleIndexSparse(b *testing.B) {
pkg/schedule/labeler/keyspace_index_test.go:630:func BenchmarkRegionLabelerNegativeLookup(b *testing.B) {
pkg/schedule/labeler/keyspace_index_test.go:666:func BenchmarkRegionLabelerKeyspaceIndex(b *testing.B) {
pkg/schedule/labeler/plan_test.go:29:func TestPlanSetLabelRule(t *testing.T) {
pkg/schedule/labeler/plan_test.go:75:func TestPlanDeleteLabelRule(t *testing.T) {
pkg/schedule/labeler/plan_test.go:98:func TestPlanCommitOps(t *testing.T) {
pkg/schedule/labeler/plan_test.go:140:func TestPlanApply(t *testing.T) {
pkg/schedule/labeler/plan_test.go:212:func TestPlanApplyWithError(t *testing.T) {
pkg/schedule/labeler/plan_test.go:242:func TestPlanMultipleOperations(t *testing.T) {
pkg/schedule/labeler/plan_test.go:295:func TestPlanDeleteNonExistentRule(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:40:func TestMain(m *testing.M) {
pkg/schedule/labeler/labeler_test.go:44:func TestAdjustRule(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:63:func TestAdjustRule2(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:102:func TestGetSetRule(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:166:func TestTxnWithEtcd(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:235:func TestIndex(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:281:func TestSaveLoadRule(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:332:func TestKeyRange(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:377:func TestLabelerRuleTTL(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:452:func TestExpiredRuleRemovalPublishesRangeIndex(t *testing.T) {
pkg/schedule/labeler/labeler_test.go:487:func TestGC(t *testing.T) {
```

## Ownership and effects

- 14: rules.go RegionLabel/LabelRule/KeyRangeRule, TestRegionLabelTTL and TestAdjustRule/TestAdjustRule2.
- 15: rule_index.go generic range indexing and splitting.
- 16: keyspace_index.go canonical keyspace rules and sparse ownership.
- 17: labeler.go loadRules/Set/Delete and endpoint RuleStorage; key `/pd/{cluster_id}/region_label/{label_id}`.
- 18: plan.go Plan, LabelRulePatch and transactional patch.
- 19: region label resolution and schedule deny.
- 20: doGC/checkAndClearExpiredLabels and metrics.go.

Failpoint: rules.go `regionLabelExpireSub1Minute`; related TTL/removal Go tests are never run naked or injected by task 14.
Metrics: `pd_schedule_labeler_event_counter{type,event}` and `pd_schedule_new_region_labeler_duration_seconds`; Go init registers them.
Background: NewRegionLabeler starts a doGC ticker goroutine, stopped via context cancellation. No background task or persistence in task 14.
Go dependencies include core, rangelist, keyrange, storage/kv and storage/endpoint. Rust task 14 depends only on errs, typeutil, serde/serde_json and chrono calendar arithmetic (std only, no clock/runtime).
