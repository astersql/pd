# 叶子迁移覆盖矩阵与依赖审计

审计日期：2026-10-22。Go oracle：`42a0da684cf28f16a48c74c54575b711031f3c5d`。

本矩阵由任务 103 建立，任务 104 补齐公开接口并更新主题回归证据，覆盖 20 个 canonical 叶子 crate。1–102 号任务文件均已删除；只读总计划未修改。核对该基准的 56 个 Go 生产文件与当前文件逐字节一致。这里记录接口与行为证据，不表示 Go 生产调用链已切换，也不表示所有 Go 接口均已有 Rust 入口。

Go 清单由 go/parser 读取基准提交的直接包文件，包含导出类型、常量、变量、函数、方法和接口方法；两种 build tag 都列出，不统计 struct 字段为独立 API。Rust 清单按生产源文件列出公开声明语法与方法名（同名方法需结合所属 impl；私有模块内的 Sealed 等声明不构成外部 API）；重导出和 serde/Clone/Display 等 trait 实现以源码和下列映射说明为准。普通方法以 snake_case 对应，构造函数以类型的 new/with_* 对应；特殊映射、排除及缺口逐包明确记录。

## 任务 104 主题回归（2026-10-22）

依赖前提：任务 1–103 文件均已删除，批次 34 审计提交为 `1cfd94aa9`。本次重新核对全部 20 个 canonical 叶子 crate 的真实 metadata 依赖闭包（含开发/构建依赖），禁止依赖零命中，所有直接依赖使用 workspace 声明；56 个 Go 生产文件仍与 oracle 基准逐字节一致。未修改只读 plan.md；既有任务清理及 prompt.md 改动不纳入本次提交。

补齐四组公开接口：KeyRanges 的 with_capacity/clean/is_empty/append、RollingPolicy::reduce、NOT_LEADER_ERR/NOT_SERVED_ERR/is_leader_changed、keyspace_modes。RollingCounter 复用 policy 的只读聚合；其余调整是对现有 Go 行为的直接入口。四项新增行为测试先因接口缺失编译失败（退出码均为 101），补齐后在完整 workspace 中实际执行通过：

- `cargo test --locked -p astersql-pd-keyutil --lib key_ranges_collection_lifecycle`
- `cargo test --locked -p astersql-pd-window --lib rolling_policy_reduce_expiry`
- `cargo test --locked -p astersql-pd-errs --lib leader_changed_classification`
- `cargo test --locked -p astersql-pd-codec --lib supported_keyspace_modes`

每个构建验证阶段均按 mkdir 竞争动态取得槽位 **1**，设置 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；各阶段进程全部结束后清理自有 owner/锁目录，保留编译缓存。没有采用任务编号固定分配。

| 当前最终命令 | 退出码 | 有效证据 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | Rust 修改后格式化 |
| `cargo fmt --all -- --check` | 0 | workspace 格式通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 全部 workspace targets 可解析 |
| `cargo metadata --format-version 1 --locked` | 0 | 20 个叶子依赖闭包通过，集中依赖通过 |
| `cargo tree --workspace --locked` | 0 | 实际依赖树审查通过 |
| `cargo check --workspace --locked` | 0 | 全 workspace 检查通过 |
| `cargo test --workspace --locked` | 0 | 234 passed、0 failed、1 ignored、0 filtered；其中 20 个叶子 crate 共 164 项真实行为测试，无 ignored/filtered，每个 crate 至少 2 项 |
| `cargo test --locked -p astersql-pd-kerneltype --features nextgen --lib` | 0 | 1 passed、0 failed/ignored/filtered |
| 下列 `make gotest` Go oracle（另加 `-json` 重跑统计） | 0 | 16 个包通过；117 个顶层测试、含子测试共 130 项 pass，无 fail/skip 测试；4 个包无测试文件，已逐文件比对 Go oracle |
| `git diff --check` | 0 | 本次代码/测试/文档及既有工作区差异无空白错误 |

Go oracle 的确切命令：

```sh
make gotest GOTEST_ARGS='./pkg/core/constant ./pkg/keyspace/constant ./pkg/mcs/utils/constant ./pkg/slice ./pkg/codec ./pkg/balancer ./pkg/movingaverage ./pkg/window ./pkg/btree ./pkg/ratelimit ./pkg/cache ./pkg/core/rangetree ./pkg/errs ./pkg/utils/jsonutil ./pkg/utils/keyutil ./pkg/utils/reflectutil ./pkg/utils/syncutil ./pkg/utils/timerutil ./pkg/utils/typeutil ./pkg/versioninfo/kerneltype -count=1'
# 第二次在 GOTEST_ARGS 末尾追加 -json，按 pass 事件统计顶层/子测试数量。
```

无测试文件的包为 core/constant、keyspace/constant、mcs/utils/constant 和 versioninfo/kerneltype。make gotest 两次均成功结束并禁用 failpoints；Go 源码无差异，无生成 failpoint 文件残留。

完整 workspace 中既有 `tso_processes` 仍按原配置 ignored，需独立进程兼容脚本；health_member 入口在无兼容环境变量时提前返回，不将其算作本主题行为证据。本主题验收依据是全部 164 项叶子测试、nextgen 的 1 项与上述 Go oracle。来源主题可按既定 Rust 映射边界关闭；不据此宣称服务层进程兼容或 Go 调用方切换已完成。

下方任务 103 的逐包测试计数与测试名保留为历史记录；任务 104 对 codec、errs、keyutil、window 各新增 1 项，当前分别为 7、7、10、10 项。

## 任务 103 验证记录

失败验证：补文档前执行 `python3 -c "from pathlib import Path; assert Path('leaf-migration-matrix.md').is_file()"`，断言失败，退出码 1。依赖检查没有失败；本任务没有改动生产代码，不构造行为回归。

动态领取槽位 **1**；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有 Cargo 命令继承该绝对路径；阶段结束释放自有锁并保留缓存。

| 命令 | 退出码 | 当前证据 |
| --- | --- | --- |
| `cargo tree --workspace --locked` | 0 | 实际 workspace 树；服务层允许出现 transport，逐叶子闭包另审计 |
| `cargo metadata --format-version 1 --locked` | 0 | 20 个叶子解析图及传递依赖；含开发/构建边 |
| `cargo check --workspace --locked` | 0 | workspace 编译检查，不计行为测试 |
| `cargo fmt --all -- --check` | 0 | 全 workspace 格式 |
| `cargo test --locked <下表20个-p选择器> --lib` | 0 | 160 passed，0 failed/ignored/filtered |
| `cargo test --locked -p astersql-pd-kerneltype --features nextgen --lib` | 0 | 1 passed，0 failed/ignored/filtered |
| `git diff --check` | 0 | 文档空白检查 |

测试选择器的确切展开：

```sh
cargo test --locked \
  -p astersql-pd-core-constant \
  -p astersql-pd-keyspace-constant \
  -p astersql-pd-mcs-constant \
  -p astersql-pd-kerneltype \
  -p astersql-pd-slice \
  -p astersql-pd-codec \
  -p astersql-pd-reflectutil \
  -p astersql-pd-typeutil \
  -p astersql-pd-syncutil \
  -p astersql-pd-timerutil \
  -p astersql-pd-movingaverage \
  -p astersql-pd-errs \
  -p astersql-pd-jsonutil \
  -p astersql-pd-balancer \
  -p astersql-pd-window \
  -p astersql-pd-btree \
  -p astersql-pd-keyutil \
  -p astersql-pd-ratelimit \
  -p astersql-pd-cache \
  -p astersql-pd-rangetree --lib
```

计数：20 个包、56 个 Go 生产文件、752 条 Go 导出清单项（含接口方法与互斥 build tag 声明，不是去重 API 数）、154 个有效文件链接、20 个依赖闭包、160+1 项实际行为测试。文档审计不需要 Go 包测试；本次 Go oracle 是固定源码逐项接口/常量/布局审查，没有伪称运行 Go 测试。Rust 单测多数是基于 Go 源码/已有序列的契约断言，并非每项都有跨进程 Go/Rust 差分；下文列出本次真实运行的测试名。errs 的目录测试直接 include_str! Go 源码，且该源码已与固定基准比对。没有重跑 workspace 集成/etcd 测试；完整主题回归由任务 104 负责。

## 真实依赖方向

从 Cargo resolve.nodes 对每个叶子根递归遍历全部 dependencies（不把 workspace 服务根的依赖误归到叶子）。禁止集合检查 `astersql-pd-proto`、`tonic`/`tonic-*`、`axum`、`etcd-client`，并额外检查 `prost`、`protobuf`；20/20 闭包无命中。

| 叶子 crate | 直接依赖（含开发依赖） | 传递闭包节点数（不含自身） |
| --- | --- | --- |
| `astersql-pd-core-constant` | 无 | 0 |
| `astersql-pd-keyspace-constant` | 无 | 0 |
| `astersql-pd-mcs-constant` | 无 | 0 |
| `astersql-pd-kerneltype` | 无 | 0 |
| `astersql-pd-slice` | 无 | 0 |
| `astersql-pd-codec` | 无 | 0 |
| `astersql-pd-reflectutil` | `serde_json` | 11 |
| `astersql-pd-typeutil` | `serde`, `serde_json` | 11 |
| `astersql-pd-syncutil` | `tokio` | 16 |
| `astersql-pd-timerutil` | `tokio` | 16 |
| `astersql-pd-movingaverage` | 无 | 0 |
| `astersql-pd-errs` | 无 | 0 |
| `astersql-pd-jsonutil` | `astersql-pd-reflectutil`, `serde`, `serde_json` | 12 |
| `astersql-pd-balancer` | `astersql-pd-syncutil` | 17 |
| `astersql-pd-window` | 无 | 0 |
| `astersql-pd-btree` | 无 | 0 |
| `astersql-pd-keyutil` | `astersql-pd-errs`, `serde`, `serde_json` | 12 |
| `astersql-pd-ratelimit` | `astersql-pd-errs`, `astersql-pd-syncutil`, `prometheus`, `thiserror`, `tokio`, `tokio-util` | 40 |
| `astersql-pd-cache` | `astersql-pd-typeutil`, `tokio`, `tokio-util` | 32 |
| `astersql-pd-rangetree` | `astersql-pd-btree` | 1 |

复核算法（输入为上面 metadata 命令输出；执行 Cargo 前仍须先领取共享槽位）：

```python
import json
from pathlib import Path

metadata = json.loads(Path("metadata.json").read_text())
packages = {p["id"]: p for p in metadata["packages"]}
nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
# leaves 为本表 20 个 astersql-pd-* crate 名；不能选 workspace 服务根。
for leaf in leaves:
    root = next(p["id"] for p in packages.values() if p["name"] == leaf)
    seen, pending = set(), [root]
    while pending:
        package_id = pending.pop()
        if package_id in seen:
            continue
        seen.add(package_id)
        pending.extend(nodes[package_id]["dependencies"])
    names = {packages[p]["name"] for p in seen}
    forbidden = names & {
        "astersql-pd-proto", "tonic", "axum", "etcd-client", "prost", "protobuf"
    }
    forbidden |= {name for name in names if name.startswith("tonic-")}
    assert not forbidden, (leaf, forbidden)
```

当前内部边为 balancer → syncutil、jsonutil → reflectutil、keyutil → errs、ratelimit → errs/syncutil、cache → typeutil、rangetree → btree。window/btree 使用标准库锁；cache 使用标准库容器而未依赖迁移 btree。这是实际解析图，与计划中的保守前置顺序区分；不存在从基础包反向进入服务层。Tokio 运行时功能由 workspace features 统一，不表示库自行创建 runtime。

## 逐包接口与行为证据

### `pkg/core/constant` → `astersql-pd-core-constant`

验证范围：判别值、未知值、大小写敏感字符串和调度组合。

映射与边界：Go 的 String 对应 Display/to_string；NewScheduleKind 对应 ScheduleKind::new；Low/Medium/High/Urgent 等改为类型关联常量，PriorityLevelLen/ResourceKindLen 对应 LEN。无业务 API 排除。

| Go 基准文件 | 导出 API |
| --- | --- |
| [kind.go](pkg/core/constant/kind.go) | `PriorityLevel`, `Low`, `Medium`, `High`, `Urgent`, `PriorityLevelLen`, `PriorityLevel.String`, `ScheduleKind`, `NewScheduleKind`, `ResourceKind`, `LeaderKind`, `RegionKind`, `WitnessKind`, `ResourceKindLen`, `ResourceKind.String`, `SchedulePolicy`, `ByCount`, `BySize`, `SchedulePolicy.String`, `StringToSchedulePolicy`, `KeyType`, `Table`, `Raw`, `Txn`, `KeyType.String`, `StringToKeyType`, `Direction`, `In`, `Out`, `Direction.String` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [kind.rs](pkg/core/constant/kind.rs) | `PriorityLevel`, `LOW`, `MEDIUM`, `HIGH`, `URGENT`, `LEN`, `from_i32`, `as_i32`, `ResourceKind`, `LEADER`, `REGION`, `WITNESS`, `LEN`, `from_i32`, `as_i32`, `SchedulePolicy`, `BY_COUNT`, `BY_SIZE`, `from_i32`, `as_i32`, `string_to_schedule_policy`, `KeyType`, `TABLE`, `RAW`, `TXN`, `from_i32`, `as_i32`, `string_to_key_type`, `Direction`, `IN`, `OUT`, `from_i32`, `as_i32`, `ScheduleKind`, `new` |

任务 103 默认配置 **4 项通过**。测试文件：[kind_test.rs](pkg/core/constant/kind_test.rs)。

任务 103 实际运行测试：`kind_test::direction_display`, `kind_test::priority_and_resource_kind`, `kind_test::key_type_round_trip`, `kind_test::schedule_policy_round_trip`。

### `pkg/keyspace/constant` → `astersql-pd-keyspace-constant`

验证范围：13 项 keyspace 名称、掩码、保留区间和标签常量。

映射与边界：Go CamelCase 常量对应 Rust SCREAMING_SNAKE_CASE；无排除。

| Go 基准文件 | 导出 API |
| --- | --- |
| [constant.go](pkg/keyspace/constant/constant.go) | `DefaultKeyspaceName`, `StartKeyspaceID`, `DefaultKeyspaceID`, `MaxValidKeyspaceID`, `RegionLabelIDPrefix`, `RegionLabelKey`, `ValidKeyspaceIDMask`, `NullKeyspaceID`, `DefaultKeyspaceGroupID`, `ReservedKeyspaceIDCount`, `ReservedKeyspaceIDStart`, `SystemKeyspaceID`, `SystemKeyspaceName` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/keyspace/constant/lib.rs) | `DEFAULT_KEYSPACE_NAME`, `START_KEYSPACE_ID`, `DEFAULT_KEYSPACE_ID`, `MAX_VALID_KEYSPACE_ID`, `REGION_LABEL_ID_PREFIX`, `REGION_LABEL_KEY`, `VALID_KEYSPACE_ID_MASK`, `NULL_KEYSPACE_ID`, `DEFAULT_KEYSPACE_GROUP_ID`, `RESERVED_KEYSPACE_ID_COUNT`, `RESERVED_KEYSPACE_ID_START`, `SYSTEM_KEYSPACE_ID`, `SYSTEM_KEYSPACE_NAME` |

任务 103 默认配置 **2 项通过**。测试文件：[constant_test.rs](pkg/keyspace/constant/constant_test.rs)。

任务 103 实际运行测试：`constant_test::base_keyspace_ids`, `constant_test::reserved_keyspaces_and_labels`。

### `pkg/mcs/utils/constant` → `astersql-pd-mcs-constant`

验证范围：21 项服务名、路径、租约、计时默认值及 keyspace group 上限。

映射与边界：时间常量改为 std::time::Duration；命名改为 SCREAMING_SNAKE_CASE；无排除。

| Go 基准文件 | 导出 API |
| --- | --- |
| [constant.go](pkg/mcs/utils/constant/constant.go) | `RetryInterval`, `TCPNetworkStr`, `DefaultGRPCGracefulStopTimeout`, `DefaultHTTPGracefulShutdownTimeout`, `DefaultLogFormat`, `DefaultLogLevel`, `DefaultDisableErrorVerbose`, `DefaultLease`, `TransferPrimaryLeaseMultiplier`, `PrimaryTickInterval`, `LeaderTickInterval`, `MicroserviceRootPath`, `PDServiceName`, `TSOServiceName`, `ResourceManagerServiceName`, `SchedulingServiceName`, `RouterServiceName`, `MaxKeyspaceGroupCount`, `MaxKeyspaceGroupCountInUse`, `DefaultKeyspaceGroupReplicaCount`, `DefaultKeyspaceGroupReplicaPriority` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/mcs/utils/constant/lib.rs) | `RETRY_INTERVAL`, `TCP_NETWORK_STR`, `DEFAULT_GRPC_GRACEFUL_STOP_TIMEOUT`, `DEFAULT_HTTP_GRACEFUL_SHUTDOWN_TIMEOUT`, `DEFAULT_LOG_FORMAT`, `DEFAULT_LOG_LEVEL`, `DEFAULT_DISABLE_ERROR_VERBOSE`, `DEFAULT_LEASE`, `TRANSFER_PRIMARY_LEASE_MULTIPLIER`, `PRIMARY_TICK_INTERVAL`, `LEADER_TICK_INTERVAL`, `MICROSERVICE_ROOT_PATH`, `PD_SERVICE_NAME`, `TSO_SERVICE_NAME`, `RESOURCE_MANAGER_SERVICE_NAME`, `SCHEDULING_SERVICE_NAME`, `ROUTER_SERVICE_NAME`, `MAX_KEYSPACE_GROUP_COUNT`, `MAX_KEYSPACE_GROUP_COUNT_IN_USE`, `DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT`, `DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY` |

任务 103 默认配置 **3 项通过**。测试文件：[constant_test.rs](pkg/mcs/utils/constant/constant_test.rs)。

任务 103 实际运行测试：`constant_test::keyspace_group_limits`, `constant_test::service_names_and_root`, `constant_test::network_and_timing_defaults`。

### `pkg/versioninfo/kerneltype` → `astersql-pd-kerneltype`

验证范围：Classic 默认值与 nextgen feature，两种配置分别运行。

映射与边界：KernelType → KERNEL_TYPE，IsNextGen → is_next_gen；Go build tag 改为 Cargo feature；不移植 Go 构建机制。

| Go 基准文件 | 导出 API |
| --- | --- |
| [classic.go](pkg/versioninfo/kerneltype/classic.go) | `KernelType`, `IsNextGen` |
| [doc.go](pkg/versioninfo/kerneltype/doc.go) |  |
| [nextgen.go](pkg/versioninfo/kerneltype/nextgen.go) | `KernelType`, `IsNextGen` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/versioninfo/kerneltype/lib.rs) | `KERNEL_TYPE`, `KERNEL_TYPE`, `is_next_gen`, `is_next_gen` |

任务 103 默认配置 **2 项通过**。测试文件：[kerneltype_test.rs](pkg/versioninfo/kerneltype/kerneltype_test.rs)。

任务 103 实际运行测试：`kerneltype_test::classic_kernel_default`, `kerneltype_test::nextgen_kernel_feature`。

### `pkg/slice` → `astersql-pd-slice`

验证范围：短路、空集合、重复元素、无序相等、移除与批大小错误。

映射与边界：公开函数一一转为 snake_case；SplitIntoBatches 的错误用 Result 表示；借用切片替代 Go slice 头部。无排除。

| Go 基准文件 | 导出 API |
| --- | --- |
| [slice.go](pkg/slice/slice.go) | `AnyOf`, `NoneOf`, `AllOf`, `Contains`, `Remove`, `EqualWithoutOrder`, `HasDupInSorted`, `SplitIntoBatches` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/slice/lib.rs) | `any_of`, `none_of`, `all_of`, `contains`, `remove`, `equal_without_order`, `has_dup_in_sorted`, `SplitIntoBatchesError`, `split_into_batches` |

任务 103 默认配置 **4 项通过**。测试文件：[slice_test.rs](pkg/slice/slice_test.rs)。

任务 103 实际运行测试：`slice_test::contains_and_remove`, `slice_test::predicate_quantifiers`, `slice_test::multiset_equal_and_sorted_duplicate`, `slice_test::split_into_batches`。

### `pkg/codec` → `astersql-pd-codec`

验证范围：memcomparable 分组/填充、截断错误、有符号整数极值、原始/编码 keyspace 边界、表/行/index 字节布局。

映射与边界：Key 不单独包装，使用 &[u8]；Key.TableIdentity/MetaOrTable → table_identity/meta_or_table；KeyspaceModes() → keyspace_modes()，按 raw/txn 顺序返回独立数组值。Go error 改为类型化 Result。

| Go 基准文件 | 导出 API |
| --- | --- |
| [codec.go](pkg/codec/codec.go) | `RawKeyspaceModePrefix`, `TxnKeyspaceModePrefix`, `KeyspacePrefixLen`, `KeyspaceModes`, `EncodeKeyspaceBoundary`, `DecodeKeyspaceKey`, `Key`, `MakeKeyspacePrefix`, `ParseKeyspacePrefix`, `TableIdentity`, `Key.TableIdentity`, `Key.MetaOrTable`, `EncodeBytes`, `EncodeInt`, `DecodeInt`, `DecodeBytes`, `GenerateTableKey`, `GenerateRecordKeyPrefix`, `GenerateRowKey`, `GenerateIndexKey` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [bytes.rs](pkg/codec/bytes.rs) | `DecodeBytesError`, `encode_bytes`, `decode_bytes` |
| [keyspace.rs](pkg/codec/keyspace.rs) | `KEYSPACE_PREFIX_LEN`, `ENCODED_KEYSPACE_PREFIX_LEN`, `RAW_KEYSPACE_MODE_PREFIX`, `TXN_KEYSPACE_MODE_PREFIX`, `ParseKeyspacePrefixError`, `DecodeKeyspaceKeyError`, `make_keyspace_prefix`, `encode_keyspace_boundary`, `decode_keyspace_key`, `parse_keyspace_prefix`, `keyspace_modes` |
| [number.rs](pkg/codec/number.rs) | `DecodeIntError`, `encode_int`, `decode_int` |
| [table.rs](pkg/codec/table.rs) | `generate_table_key`, `generate_record_key_prefix`, `generate_row_key`, `generate_index_key`, `TableIdentity`, `table_identity`, `meta_or_table` |

任务 103 默认配置 **6 项通过**。测试文件：[bytes_test.rs](pkg/codec/bytes_test.rs), [keyspace_test.rs](pkg/codec/keyspace_test.rs), [number_test.rs](pkg/codec/number_test.rs), [table_test.rs](pkg/codec/table_test.rs)。

任务 103 实际运行测试：`bytes_test::memcomparable_bytes`, `keyspace_test::encoded_keyspace_boundary`, `keyspace_test::raw_keyspace_prefix`, `table_test::record_and_index_keys`, `number_test::comparable_i64`, `table_test::table_identity_and_meta`。

### `pkg/utils/reflectutil` → `astersql-pd-reflectutil`

验证范围：递归 JSON 路径、相同字段判定、空路径/未知路径/非对象描述。

映射与边界：三个函数对应同名 snake_case；输入为 serde_json::Value 字段/类型描述。明确不移植 reflect.Type、Go struct/tag 的运行时反射；调用方须提供 serde 可观察的字段描述，重名字段遍历顺序不能据此承诺与 Go 声明顺序一致。

| Go 基准文件 | 导出 API |
| --- | --- |
| [tag.go](pkg/utils/reflectutil/tag.go) | `FindJSONFullTagByChildTag`, `FindSameFieldByJSON`, `FindFieldByJSONTag` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/utils/reflectutil/lib.rs) | `find_json_full_tag_by_child_tag`, `find_same_field_by_json`, `find_field_by_json_tag` |

任务 103 默认配置 **3 项通过**。测试文件：[tag_test.rs](pkg/utils/reflectutil/tag_test.rs)。

任务 103 实际运行测试：`tag_test::full_tag_by_child_tag`, `tag_test::field_type_by_json_path`, `tag_test::same_fields_by_json`。

### `pkg/utils/typeutil` → `astersql-pd-typeutil`

验证范围：比较、字节/布尔/JSON 转换、Duration/ByteSize/StringSlice/TimeOptional 序列化、墙钟差和 clone 错误。

映射与边界：MarshalJSON/UnmarshalJSON/MarshalText/UnmarshalText → serde 与 FromStr/Display；NewDuration → Duration::from_nanos；ZeroTime 以 TimeOptional/时间表示处理，无同名导出。DeepClone → 原生 Clone 或 deep_clone_json；不移植任意 Go Codec 工厂/反射和 protobuf clone，序列化错误必须返回。

| Go 基准文件 | 导出 API |
| --- | --- |
| [clone.go](pkg/utils/typeutil/clone.go) | `Codec`, `Codec.Marshal`, `Codec.Unmarshal`, `DeepClone` |
| [comparison.go](pkg/utils/typeutil/comparison.go) | `MinUint64`, `MaxUint64`, `MinDuration`, `AreStringSlicesEqual`, `AreStringSlicesEquivalent`, `Float64Equal`, `EqualBaseURLs`, `TrimScheme` |
| [conversion.go](pkg/utils/typeutil/conversion.go) | `BytesToUint64`, `Uint64ToBytes`, `BoolToUint64`, `BoolToInt`, `JSONToUint64Slice` |
| [duration.go](pkg/utils/typeutil/duration.go) | `Duration`, `NewDuration`, `Duration.MarshalJSON`, `Duration.UnmarshalJSON`, `Duration.UnmarshalText`, `Duration.MarshalText`, `SaturatingStdDurationFromSeconds` |
| [size.go](pkg/utils/typeutil/size.go) | `ByteSize`, `ParseMBFromText`, `ByteSize.MarshalJSON`, `ByteSize.UnmarshalJSON`, `ByteSize.UnmarshalText` |
| [string_slice.go](pkg/utils/typeutil/string_slice.go) | `StringSlice`, `StringSlice.MarshalJSON`, `StringSlice.UnmarshalJSON` |
| [time.go](pkg/utils/typeutil/time.go) | `ZeroTime`, `ParseTimestamp`, `SubRealTimeByWallClock`, `SubTSOPhysicalByWallClock`, `TimeOptional`, `TimeOptional.MarshalJSON`, `TimeOptional.UnmarshalJSON` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [clone.rs](pkg/utils/typeutil/clone.rs) | `deep_clone_json` |
| [comparison.rs](pkg/utils/typeutil/comparison.rs) | `min_u64`, `max_u64`, `min_duration`, `float64_equal`, `are_string_slices_equivalent`, `equal_base_urls`, `trim_scheme` |
| [conversion.rs](pkg/utils/typeutil/conversion.rs) | `BytesToU64Error`, `actual_len`, `bytes_to_u64`, `u64_to_bytes`, `bool_to_u64`, `bool_to_int`, `json_to_u64_slice` |
| [duration.rs](pkg/utils/typeutil/duration.rs) | `Duration`, `from_nanos`, `as_nanos`, `ParseDurationError`, `saturating_std_duration_from_seconds` |
| [size.rs](pkg/utils/typeutil/size.rs) | `ByteSize`, `from_bytes`, `as_bytes`, `ParseByteSizeError`, `parse_mb_from_text` |
| [string_slice.rs](pkg/utils/typeutil/string_slice.rs) | `StringSlice`, `as_slice`, `into_inner` |
| [time.rs](pkg/utils/typeutil/time.rs) | `parse_timestamp`, `sub_real_time_by_wall_clock`, `sub_tso_physical_by_wall_clock`, `TimeOptional` |

任务 103 默认配置 **10 项通过**。测试文件：[clone_test.rs](pkg/utils/typeutil/clone_test.rs), [comparison_test.rs](pkg/utils/typeutil/comparison_test.rs), [conversion_test.rs](pkg/utils/typeutil/conversion_test.rs), [duration_test.rs](pkg/utils/typeutil/duration_test.rs), [size_test.rs](pkg/utils/typeutil/size_test.rs), [string_slice_test.rs](pkg/utils/typeutil/string_slice_test.rs), [time_test.rs](pkg/utils/typeutil/time_test.rs)。

任务 103 实际运行测试：`comparison_test::comparison_helpers`, `conversion_test::conversion_helpers`, `clone_test::deep_clone_contract_codec_errors`, `clone_test::deep_clone_contract`, `duration_test::duration_serde_and_saturation`, `string_slice_test::string_slice_json`, `size_test::byte_size_serde_text_and_mb_fallback`, `time_test::timestamp_and_wall_clock_differences`, `time_test::time_optional_json`, `time_test::timestamp_and_wall_clock_parse`。

### `pkg/utils/syncutil` → `astersql-pd-syncutil`

验证范围：真实锁互斥、按 ID/hash 串行化、single-flight 合并/顺序/取消/执行计数与并发。

映射与边界：Mutex/RWMutex → blocking::{Mutex,RwLock} 和 asynchronous::{Mutex,RwLock}；Lock/Unlock → lock().await 的守卫与 Drop；WithHash/WithRemoveEntryOnUnlock → 构造方法。Do/ExecCount → call/call_cancellable/exec_count（见源文件）。不移植 deadlock build tag 与 Go 锁对象复制语义。

| Go 基准文件 | 导出 API |
| --- | --- |
| [lock_group.go](pkg/utils/syncutil/lock_group.go) | `LockGroup`, `LockGroupOption`, `WithHash`, `WithRemoveEntryOnUnlock`, `NewLockGroup`, `LockGroup.Lock`, `LockGroup.Unlock` |
| [mutex_deadlock.go](pkg/utils/syncutil/mutex_deadlock.go) | `Mutex`, `RWMutex` |
| [mutex_sync.go](pkg/utils/syncutil/mutex_sync.go) | `Mutex`, `RWMutex` |
| [ordered_single_flight.go](pkg/utils/syncutil/ordered_single_flight.go) | `OrderedSingleFlight`, `NewOrderedSingleFlight`, `OrderedSingleFlight.Do`, `OrderedSingleFlight.ExecCount` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lock_group.rs](pkg/utils/syncutil/lock_group.rs) | `LockGroup`, `LockGroupGuard`, `new`, `with_hash`, `with_remove_entry_on_unlock`, `lock` |
| [ordered_single_flight.rs](pkg/utils/syncutil/ordered_single_flight.rs) | `ExecutionCancellation`, `is_cancelled`, `cancelled`, `OrderedSingleFlight`, `new`, `exec_count`, `call`, `call_cancellable` |

任务 103 默认配置 **9 项通过**。测试文件：[lock_group_test.rs](pkg/utils/syncutil/lock_group_test.rs), [mutex_test.rs](pkg/utils/syncutil/mutex_test.rs), [ordered_single_flight_test.rs](pkg/utils/syncutil/ordered_single_flight_test.rs)。

任务 103 实际运行测试：`lock_group_test::lock_group_cleanup`, `lock_group_test::lock_group_serialization`, `ordered_single_flight_test::singleflight_cancellation_pending_can_be_reused`, `ordered_single_flight_test::singleflight_cancellation_noncooperative_execution_keeps_order`, `ordered_single_flight_test::singleflight_cancellation_last_waiter`, `ordered_single_flight_test::singleflight_cancellation_shared_waiters`, `ordered_single_flight_test::singleflight_coalesce_and_order`, `mutex_test::mutex_boundary`, `ordered_single_flight_test::singleflight_randomized`。

### `pkg/utils/timerutil` → `astersql-pd-timerutil`

验证范围：受控时间下到期、归还、重置、复用、取消安全。

映射与边界：GlobalTimerPool 不迁移为全局可变池；宿主持有 TimerPool::new，Get/Put → get/put，Timer 实现 Future；不创建私有 runtime。

| Go 基准文件 | 导出 API |
| --- | --- |
| [pool.go](pkg/utils/timerutil/pool.go) | `GlobalTimerPool`, `timerPool.Get`, `timerPool.Put` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/utils/timerutil/lib.rs) | `TimerPool`, `new`, `get`, `put`, `Timer` |

任务 103 默认配置 **2 项通过**。测试文件：[pool_test.rs](pkg/utils/timerutil/pool_test.rs)。

任务 103 实际运行测试：`pool_test::timer_checkout_and_expiry`, `pool_test::timer_reuse_and_cancel`。

### `pkg/movingaverage` → `astersql-pd-movingaverage`

验证范围：权重分配、EMA/WMA/HMA、median/max、滚动区间与非正常 interval、Set/Clone。

映射与边界：EMA/WMA/HMA → Ema/Wma/Hma；NewX → X::new/with_size/with_decay；Clone → Rust Clone。MovingAvg 接口由具体类型方法表达，没有同名 trait；不保留 Go 接口 ABI。

| Go 基准文件 | 导出 API |
| --- | --- |
| [avg_over_time.go](pkg/movingaverage/avg_over_time.go) | `AvgOverTime`, `NewAvgOverTime`, `AvgOverTime.Get`, `AvgOverTime.Clear`, `AvgOverTime.Add`, `AvgOverTime.Set`, `AvgOverTime.IsFull`, `AvgOverTime.Clone`, `AvgOverTime.GetIntervalSum`, `AvgOverTime.GetInstantaneous` |
| [exponential_moving_average.go](pkg/movingaverage/exponential_moving_average.go) | `EMA`, `NewEMA`, `EMA.Add`, `EMA.Get`, `EMA.Reset`, `EMA.Set`, `EMA.GetInstantaneous` |
| [hull_moving_average.go](pkg/movingaverage/hull_moving_average.go) | `HMA`, `NewHMA`, `HMA.Add`, `HMA.Get`, `HMA.Reset`, `HMA.Set`, `HMA.GetInstantaneous` |
| [max_filter.go](pkg/movingaverage/max_filter.go) | `MaxFilter`, `NewMaxFilter`, `MaxFilter.Add`, `MaxFilter.Get`, `MaxFilter.Reset`, `MaxFilter.Set`, `MaxFilter.GetInstantaneous` |
| [median_filter.go](pkg/movingaverage/median_filter.go) | `MedianFilter`, `NewMedianFilter`, `MedianFilter.Add`, `MedianFilter.Get`, `MedianFilter.Reset`, `MedianFilter.Set`, `MedianFilter.GetInstantaneous`, `MedianFilter.Clone` |
| [moving_average.go](pkg/movingaverage/moving_average.go) | `MovingAvg`, `MovingAvg.Add`, `MovingAvg.Get`, `MovingAvg.GetInstantaneous`, `MovingAvg.Reset`, `MovingAvg.Set` |
| [time_median.go](pkg/movingaverage/time_median.go) | `TimeMedian`, `NewTimeMedian`, `TimeMedian.Get`, `TimeMedian.Add`, `TimeMedian.Set`, `TimeMedian.GetInstantaneous`, `TimeMedian.Clone` |
| [weight_allocator.go](pkg/movingaverage/weight_allocator.go) | `WeightAllocator`, `NewWeightAllocator`, `WeightAllocator.Get` |
| [weight_moving_average.go](pkg/movingaverage/weight_moving_average.go) | `WMA`, `NewWMA`, `WMA.Add`, `WMA.Get`, `WMA.Reset`, `WMA.Set`, `WMA.GetInstantaneous` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [avg_over_time.rs](pkg/movingaverage/avg_over_time.rs) | `AvgOverTime`, `new`, `add`, `get`, `clear`, `set`, `get_interval_sum`, `get_instantaneous`, `is_full` |
| [ema.rs](pkg/movingaverage/ema.rs) | `Ema`, `new`, `with_decay`, `add`, `get`, `reset`, `set`, `get_instantaneous` |
| [hma.rs](pkg/movingaverage/hma.rs) | `Hma`, `new`, `with_size`, `add`, `get`, `reset`, `set`, `get_instantaneous` |
| [max_filter.rs](pkg/movingaverage/max_filter.rs) | `MaxFilter`, `new`, `add`, `get`, `reset`, `set`, `get_instantaneous` |
| [median_filter.rs](pkg/movingaverage/median_filter.rs) | `MedianFilter`, `new`, `add`, `get`, `reset`, `set`, `get_instantaneous` |
| [time_median.rs](pkg/movingaverage/time_median.rs) | `TimeMedian`, `new`, `get`, `add`, `set`, `get_instantaneous` |
| [weight_allocator.rs](pkg/movingaverage/weight_allocator.rs) | `WeightAllocator`, `new`, `get` |
| [wma.rs](pkg/movingaverage/wma.rs) | `Wma`, `new`, `with_size`, `add`, `get`, `reset`, `set`, `get_instantaneous` |

任务 103 默认配置 **17 项通过**。测试文件：[avg_over_time_test.rs](pkg/movingaverage/avg_over_time_test.rs), [max_filter_test.rs](pkg/movingaverage/max_filter_test.rs), [moving_average_test.rs](pkg/movingaverage/moving_average_test.rs), [weight_allocator_test.rs](pkg/movingaverage/weight_allocator_test.rs)。

任务 103 实际运行测试：`max_filter_test::max_filter_go_sequence_set_reset`, `avg_over_time_test::avg_over_time_window_fractional_margin_and_eviction`, `max_filter_test::max_filter_ieee_comparison_order`, `avg_over_time_test::avg_over_time_window_min_filled_and_pulse`, `avg_over_time_test::avg_over_time_state_set_clear_clone`, `avg_over_time_test::avg_over_time_state_unstable_and_signed_intervals`, `moving_average_test::ema_behavior`, `moving_average_test::median_filter_go_sequence`, `max_filter_test::max_filter_negative_values_and_window_expiry`, `moving_average_test::median_filter_empty_and_invalid_windows`, `max_filter_test::max_filter_invalid_and_empty_windows`, `moving_average_test::time_median_report_intervals`, `moving_average_test::time_median_set_clone_and_zero_interval`, `weight_allocator_test::weight_allocator`, `moving_average_test::wma_behavior`, `moving_average_test::median_filter_windows_and_clone`, `moving_average_test::hma_behavior`。

### `pkg/errs` → `astersql-pd-errs`

验证范围：错误模板、格式化、source 链、相等语义；catalog_completeness 读取真实 Go 声明，核对 239 项领域错误及 15 项 transport 排除。

映射与边界：239 个 Normalize 模板 → catalog::ERR_* 与 ErrorTemplate/TypedError。15 个 gRPC transport 错误仅列 SERVER_TRANSPORT_ERRORS，ZapError 仅列 LOG_ADAPTER_EXPORTS，转换留适配层；NotLeaderErr/NotServedErr → NOT_LEADER_ERR/NOT_SERVED_ERR；IsLeaderChanged → is_leader_changed，保留空错误、大小写敏感子串与带 source 错误文本语义。ErrStreamClosed 属于上述 transport 排除。

| Go 基准文件 | 导出 API |
| --- | --- |
| [errno.go](pkg/errs/errno.go) | `NotLeaderErr`, `NotServedErr`, `ErrStreamClosed`, `ErrUnknown`, `ErrForwardTSOTimeout`, `ErrTSOProxyRecvFromClientTimeout`, `ErrSendHeartbeatTimeout`, `ErrNotFoundTSOAddr`, `ErrNotFoundSchedulingAddr`, `ErrNotFoundService`, `ErrMaxCountTSOProxyRoutinesExceeded`, `ErrGRPCRateLimitExceeded`, `ErrMismatchClusterID`, `ErrNotLeader`, `ErrNotStarted`, `ErrEtcdNotStarted`, `ErrFollowerHandlingNotAllowed`, `ErrGetSourceStore`, `ErrGetTargetStore`, `ErrIncorrectSystemTime`, `ErrInvalidArgument`, `ErrGetAllocator`, `ErrResetUserTimestamp`, `ErrGenerateTimestamp`, `ErrUpdateTimestamp`, `ErrSaveTimestamp`, `ErrLogicOverflow`, `ErrProxyTSOTimeout`, `ErrKeyspaceGroupIDInvalid`, `ErrLoadKeyspaceGroupsTimeout`, `ErrLoadKeyspaceGroupsTerminated`, `ErrLoadKeyspaceGroupsRetryExhausted`, `ErrKeyspaceGroupNotInitialized`, `ErrKeyspaceNotAssigned`, `ErrGetMinTS`, `ErrKeyspaceGroupIsMerging`, `ErrEtcdLeaderNotFound`, `ErrMarshalMember`, `ErrMarshalParticipant`, `ErrCheckCampaign`, `ErrWrongRangeKeys`, `ErrStoreNotFound`, `ErrStoreRemoved`, `ErrStoreDestroyed`, `ErrStoreUnhealthy`, `ErrStoreServing`, `ErrSlowTrendEvicted`, `ErrStoresNotEnough`, `ErrNoStoreForRegionLeader`, `ErrClientCreateTSOStream`, `ErrClientGetTSO`, `ErrClientGetLeader`, `ErrClientGetMember`, `ErrClientGetMinTSO`, `ErrOperatorNotFound`, `ErrAddOperator`, `ErrRegionInvalidID`, `ErrRegionNotAdjacent`, `ErrRegionNotFound`, `ErrRegionAbnormalPeer`, `ErrPluginNotFound`, `ErrUnexpectedOperatorStatus`, `ErrUnknownOperatorStep`, `ErrMergeOperator`, `ErrCreateOperator`, `ErrSchedulerExisted`, `ErrSchedulerDuplicated`, `ErrSchedulerNotFound`, `ErrScheduleConfigNotExist`, `ErrSchedulerConfig`, `ErrCacheOverflow`, `ErrInternalGrowth`, `ErrSchedulerCreateFuncNotRegistered`, `ErrSchedulerTiKVSplitDisabled`, `ErrNoStoreToBeSelected`, `ErrCheckerNotFound`, `ErrCheckerMergeAgain`, `ErrNoStoreToAdd`, `ErrNoStoreToReplace`, `ErrPeerCannotBeLeader`, `ErrPeerCannotBeWitness`, `ErrNoNewLeader`, `ErrRegionNoLeader`, `ErrEmptyRegion`, `ErrKeyspaceNotFound`, `ErrRegionSplitTimeout`, `ErrRegionSplitFailed`, `ErrKeyspaceExists`, `ErrKeyspaceGroupExists`, `ErrKeyspaceNotInKeyspaceGroup`, `ErrKeyspaceNotInAnyKeyspaceGroup`, `ErrNodeNotInKeyspaceGroup`, `ErrKeyspaceGroupNotEnoughReplicas`, `ErrKeyspaceGroupWithEmptyKeyspace`, `ErrModifyDefaultKeyspaceGroup`, `ErrNoAvailableNode`, `ErrExceedMaxEtcdTxnOps`, `ErrModifyDefaultKeyspace`, `ErrModifyReservedKeyspace`, `ErrIllegalOperation`, `ErrUnsupportedOperationInKeyspace`, `ErrKeyspaceConfigPreconditionFailed`, `ErrKeyspaceGroupPrimaryNotFound`, `ErrKeyspaceGroupNotExists`, `ErrKeyspaceGroupInSplit`, `ErrKeyspaceGroupNotInSplit`, `ErrKeyspaceGroupInMerging`, `ErrKeyspaceGroupNotInMerging`, `ErrKeyspaceGroupModRevisionStale`, `ErrUnknownMetaServiceGroup`, `ErrInvalidAssignmentCount`, `ErrMetaServiceGroupDisabled`, `ErrGroupHasAssignedKeyspaces`, `ErrMetaServiceGroupUnhealthy`, `ErrDiagnosticDisabled`, `ErrSchedulerUndiagnosable`, `ErrNoDiagnosticResult`, `ErrDiagnosticLoadPlan`, `ErrRuleContent`, `ErrLoadRule`, `ErrLoadRuleGroup`, `ErrBuildRuleList`, `ErrPlacementDisabled`, `ErrKeyFormat`, `ErrRuleNotFound`, `ErrRegionRuleContent`, `ErrRegionRuleNotFound`, `ErrNotBootstrapped`, `ErrStoreIsUp`, `ErrInvalidStoreID`, `ErrSchedulingIsHalted`, `ErrFeatureNotExisted`, `ErrRedirect`, `ErrOptionNotExist`, `ErrOptionTypeInvalid`, `ErrRedirectNoLeader`, `ErrRedirectToNotLeader`, `ErrRedirectToNotPrimary`, `ErrSecurityConfig`, `ErrTLSConfig`, `ErrServiceRegistered`, `ErrAPIInformationInvalid`, `ErrClientURLEmpty`, `ErrLeaderNil`, `ErrCancelStartEtcd`, `ErrConfigItem`, `ErrServerNotStarted`, `ErrRateLimitExceeded`, `ErrLeaderFrequentlyChange`, `ErrInitFileLog`, `ErrBytesToUint64`, `ErrNoCPUControllerDetected`, `ErrMaxWaitingTasksExceeded`, `ErrURLParse`, `ErrQueryUnescape`, `ErrGRPCDial`, `ErrCloseGRPCConn`, `ErrGRPCSend`, `ErrGRPCRecv`, `ErrGRPCCloseSend`, `ErrGRPCCreateStream`, `ErrProtoUnmarshal`, `ErrProtoMarshal`, `ErrNewEtcdClient`, `ErrStartEtcd`, `ErrEtcdURLMap`, `ErrEtcdGrantLease`, `ErrEtcdTxnInternal`, `ErrEtcdTxnConflict`, `ErrEtcdTxnResponse`, `ErrEtcdKVPut`, `ErrEtcdKVDelete`, `ErrEtcdKVGet`, `ErrEtcdKVGetResponse`, `ErrEtcdGetCluster`, `ErrEtcdMoveLeader`, `ErrEtcdWatcherCancel`, `ErrCloseEtcdClient`, `ErrEtcdMemberList`, `ErrEtcdMemberRemove`, `ErrDashboardStart`, `ErrDashboardStop`, `ErrStrconvParseBool`, `ErrStrconvParseInt`, `ErrStrconvParseUint`, `ErrStrconvParseFloat`, `ErrPrometheusPushMetrics`, `ErrPrometheusCreateClient`, `ErrPrometheusQuery`, `ErrSendRequest`, `ErrWriteHTTPBody`, `ErrNewHTTPRequest`, `ErrIORead`, `ErrOSOpen`, `ErrReadDirName`, `ErrNetstatTCPSocks`, `ErrHexDecodingString`, `ErrFilePathAbs`, `ErrLoadPlugin`, `ErrLookupPluginFunc`, `ErrJSONMarshal`, `ErrJSONUnmarshal`, `ErrLevelDBClose`, `ErrLevelDBWrite`, `ErrLevelDBOpen`, `ErrSemverNewVersion`, `ErrInitLogger`, `ErrEncryptionInvalidMethod`, `ErrEncryptionInvalidConfig`, `ErrEncryptionGenerateIV`, `ErrEncryptionGCMEncrypt`, `ErrEncryptionGCMDecrypt`, `ErrEncryptionCTREncrypt`, `ErrEncryptionCTRDecrypt`, `ErrEncryptionEncryptRegion`, `ErrEncryptionDecryptRegion`, `ErrEncryptionNewDataKey`, `ErrEncryptionNewMasterKey`, `ErrEncryptionCurrentKeyNotFound`, `ErrEncryptionKeyNotFound`, `ErrEncryptionKeysWatcher`, `ErrEncryptionLoadKeys`, `ErrEncryptionRotateDataKey`, `ErrEncryptionSaveDataKeys`, `ErrEncryptionKMS`, `ErrCryptoX509KeyPair`, `ErrCryptoAppendCertsFromPEM`, `ErrBindJSON`, `ErrUnsafeRecoveryIsRunning`, `ErrUnsafeRecoveryInvalidInput`, `ErrProgressWrongStatus`, `ErrProgressNotFound`, `ErrKeyspaceNotExists`, `ErrKeyspaceNotExistsByName`, `ErrResourceGroupNotExists`, `ErrDeleteReservedGroup`, `ErrInvalidGroup`, `ErrNotFoundSchedulingPrimary`, `ErrSchedulingServer`, `ErrGCOnInvalidKeyspace`, `ErrDecreasingGCSafePoint`, `ErrGCSafePointExceedsTxnSafePoint`, `ErrDecreasingTxnSafePoint`, `ErrGCBarrierTSBehindTxnSafePoint`, `ErrReservedGCBarrierID`, `ErrGlobalGCBarrierTSBehindTxnSafePoint`, `ErrIDExhausted`, `ErrAffinityGroupNotFound`, `ErrAffinityGroupExist`, `ErrAffinityGroupConflict`, `ErrAffinityInternal`, `ErrAffinityGroupContent`, `ErrInvalidLabelRuleFormat`, `ErrEmptyRequest`, `ErrInvalidGroupID`, `ErrInvalidKeyFormat` |
| [errs.go](pkg/errs/errs.go) | `ZapError`, `IsLeaderChanged` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [catalog.rs](pkg/errs/catalog.rs) | `ERR_GET_SOURCE_STORE`, `ERR_GET_TARGET_STORE`, `ERR_INCORRECT_SYSTEM_TIME`, `ERR_INVALID_ARGUMENT`, `ERR_GET_ALLOCATOR`, `ERR_RESET_USER_TIMESTAMP`, `ERR_GENERATE_TIMESTAMP`, `ERR_UPDATE_TIMESTAMP`, `ERR_SAVE_TIMESTAMP`, `ERR_LOGIC_OVERFLOW`, `ERR_PROXY_TSO_TIMEOUT`, `ERR_KEYSPACE_GROUP_ID_INVALID`, `ERR_LOAD_KEYSPACE_GROUPS_TIMEOUT`, `ERR_LOAD_KEYSPACE_GROUPS_TERMINATED`, `ERR_LOAD_KEYSPACE_GROUPS_RETRY_EXHAUSTED`, `ERR_KEYSPACE_GROUP_NOT_INITIALIZED`, `ERR_KEYSPACE_NOT_ASSIGNED`, `ERR_GET_MIN_TS`, `ERR_KEYSPACE_GROUP_IS_MERGING`, `ERR_ETCD_LEADER_NOT_FOUND`, `ERR_MARSHAL_MEMBER`, `ERR_MARSHAL_PARTICIPANT`, `ERR_CHECK_CAMPAIGN`, `ERR_WRONG_RANGE_KEYS`, `ERR_STORE_NOT_FOUND`, `ERR_STORE_REMOVED`, `ERR_STORE_DESTROYED`, `ERR_STORE_UNHEALTHY`, `ERR_STORE_SERVING`, `ERR_SLOW_TREND_EVICTED`, `ERR_STORES_NOT_ENOUGH`, `ERR_NO_STORE_FOR_REGION_LEADER`, `ERR_CLIENT_CREATE_TSO_STREAM`, `ERR_CLIENT_GET_TSO`, `ERR_CLIENT_GET_LEADER`, `ERR_CLIENT_GET_MEMBER`, `ERR_CLIENT_GET_MIN_TSO`, `FOUNDATION_ERRORS`, `ERR_OPERATOR_NOT_FOUND`, `ERR_ADD_OPERATOR`, `ERR_REGION_INVALID_ID`, `ERR_REGION_NOT_ADJACENT`, `ERR_REGION_NOT_FOUND`, `ERR_REGION_ABNORMAL_PEER`, `ERR_PLUGIN_NOT_FOUND`, `ERR_UNEXPECTED_OPERATOR_STATUS`, `ERR_UNKNOWN_OPERATOR_STEP`, `ERR_MERGE_OPERATOR`, `ERR_CREATE_OPERATOR`, `ERR_CHECKER_NOT_FOUND`, `ERR_CHECKER_MERGE_AGAIN`, `ERR_NO_STORE_TO_ADD`, `ERR_NO_STORE_TO_REPLACE`, `ERR_PEER_CANNOT_BE_LEADER`, `ERR_PEER_CANNOT_BE_WITNESS`, `ERR_NO_NEW_LEADER`, `ERR_REGION_NO_LEADER`, `ERR_KEYSPACE_NOT_FOUND`, `ERR_REGION_SPLIT_TIMEOUT`, `ERR_REGION_SPLIT_FAILED`, `ERR_KEYSPACE_EXISTS`, `ERR_KEYSPACE_GROUP_EXISTS`, `ERR_KEYSPACE_NOT_IN_KEYSPACE_GROUP`, `ERR_KEYSPACE_NOT_IN_ANY_KEYSPACE_GROUP`, `ERR_NODE_NOT_IN_KEYSPACE_GROUP`, `ERR_KEYSPACE_GROUP_NOT_ENOUGH_REPLICAS`, `ERR_KEYSPACE_GROUP_WITH_EMPTY_KEYSPACE`, `ERR_MODIFY_DEFAULT_KEYSPACE_GROUP`, `ERR_NO_AVAILABLE_NODE`, `ERR_EXCEED_MAX_ETCD_TXN_OPS`, `ERR_MODIFY_DEFAULT_KEYSPACE`, `ERR_MODIFY_RESERVED_KEYSPACE`, `ERR_ILLEGAL_OPERATION`, `ERR_UNSUPPORTED_OPERATION_IN_KEYSPACE`, `ERR_KEYSPACE_CONFIG_PRECONDITION_FAILED`, `ERR_KEYSPACE_GROUP_PRIMARY_NOT_FOUND`, `ERR_KEYSPACE_GROUP_NOT_EXISTS`, `ERR_KEYSPACE_GROUP_IN_SPLIT`, `ERR_KEYSPACE_GROUP_NOT_IN_SPLIT`, `ERR_KEYSPACE_GROUP_IN_MERGING`, `ERR_KEYSPACE_GROUP_NOT_IN_MERGING`, `ERR_KEYSPACE_GROUP_MOD_REVISION_STALE`, `ERR_UNKNOWN_META_SERVICE_GROUP`, `ERR_INVALID_ASSIGNMENT_COUNT`, `ERR_META_SERVICE_GROUP_DISABLED`, `ERR_GROUP_HAS_ASSIGNED_KEYSPACES`, `ERR_META_SERVICE_GROUP_UNHEALTHY`, `ERR_REGION_RULE_CONTENT`, `ERR_REGION_RULE_NOT_FOUND`, `SCHEDULING_ERRORS`, `ERR_SERVICE_REGISTERED`, `ERR_API_INFORMATION_INVALID`, `ERR_CLIENT_URL_EMPTY`, `ERR_LEADER_NIL`, `ERR_CANCEL_START_ETCD`, `ERR_CONFIG_ITEM`, `ERR_SERVER_NOT_STARTED`, `ERR_RATE_LIMIT_EXCEEDED`, `ERR_LEADER_FREQUENTLY_CHANGE`, `ERR_BYTES_TO_UINT64`, `ERR_MAX_WAITING_TASKS_EXCEEDED`, `SERVICE_ERRORS`, `ERR_URL_PARSE`, `ERR_QUERY_UNESCAPE`, `ERR_GRPC_DIAL`, `ERR_CLOSE_GRPC_CONN`, `ERR_GRPC_SEND`, `ERR_GRPC_RECV`, `ERR_GRPC_CLOSE_SEND`, `ERR_GRPC_CREATE_STREAM`, `ERR_PROTO_UNMARSHAL`, `ERR_PROTO_MARSHAL`, `ERR_NEW_ETCD_CLIENT`, `ERR_START_ETCD`, `ERR_ETCD_URL_MAP`, `ERR_ETCD_GRANT_LEASE`, `ERR_ETCD_TXN_INTERNAL`, `ERR_ETCD_TXN_CONFLICT`, `ERR_ETCD_TXN_RESPONSE`, `ERR_ETCD_KV_PUT`, `ERR_ETCD_KV_DELETE`, `ERR_ETCD_KV_GET`, `ERR_ETCD_KV_GET_RESPONSE`, `ERR_ETCD_GET_CLUSTER`, `ERR_ETCD_MOVE_LEADER`, `ERR_ETCD_WATCHER_CANCEL`, `ERR_CLOSE_ETCD_CLIENT`, `ERR_ETCD_MEMBER_LIST`, `ERR_ETCD_MEMBER_REMOVE`, `ERR_DASHBOARD_START`, `ERR_DASHBOARD_STOP`, `ERR_STRCONV_PARSE_BOOL`, `ERR_STRCONV_PARSE_INT`, `ERR_STRCONV_PARSE_UINT`, `ERR_STRCONV_PARSE_FLOAT`, `ERR_PROMETHEUS_PUSH_METRICS`, `ERR_PROMETHEUS_CREATE_CLIENT`, `ERR_PROMETHEUS_QUERY`, `ERR_SEND_REQUEST`, `ERR_WRITE_HTTP_BODY`, `ERR_NEW_HTTP_REQUEST`, `ERR_IO_READ`, `ERR_OS_OPEN`, `ERR_READ_DIR_NAME`, `ERR_NETSTAT_TCP_SOCKS`, `ERR_HEX_DECODING_STRING`, `ERR_FILE_PATH_ABS`, `ERR_LOAD_PLUGIN`, `ERR_LOOKUP_PLUGIN_FUNC`, `ERR_JSON_MARSHAL`, `ERR_JSON_UNMARSHAL`, `ERR_LEVEL_DB_CLOSE`, `ERR_LEVEL_DB_WRITE`, `ERR_LEVEL_DB_OPEN`, `ERR_SEMVER_NEW_VERSION`, `ERR_INIT_LOGGER`, `ERR_ENCRYPTION_INVALID_METHOD`, `ERR_ENCRYPTION_INVALID_CONFIG`, `ERR_ENCRYPTION_GENERATE_IV`, `ERR_ENCRYPTION_GCM_ENCRYPT`, `ERR_ENCRYPTION_GCM_DECRYPT`, `ERR_ENCRYPTION_CTR_ENCRYPT`, `ERR_ENCRYPTION_CTR_DECRYPT`, `ERR_ENCRYPTION_ENCRYPT_REGION`, `ERR_ENCRYPTION_DECRYPT_REGION`, `ERR_ENCRYPTION_NEW_DATA_KEY`, `ERR_ENCRYPTION_NEW_MASTER_KEY`, `ERR_ENCRYPTION_CURRENT_KEY_NOT_FOUND`, `ERR_ENCRYPTION_KEY_NOT_FOUND`, `ERR_ENCRYPTION_KEYS_WATCHER`, `ERR_ENCRYPTION_LOAD_KEYS`, `ERR_ENCRYPTION_ROTATE_DATA_KEY`, `ERR_ENCRYPTION_SAVE_DATA_KEYS`, `ERR_ENCRYPTION_KMS`, `ERR_CRYPTO_X509_KEY_PAIR`, `ERR_CRYPTO_APPEND_CERTS_FROM_PEM`, `ERR_BIND_JSON`, `INFRASTRUCTURE_ERRORS`, `DOMAIN_ERROR_GROUPS`, `SERVER_TRANSPORT_ERRORS`, `LOG_ADAPTER_EXPORTS`, `ERR_SCHEDULER_EXISTED`, `ERR_SCHEDULER_DUPLICATED`, `ERR_SCHEDULER_NOT_FOUND`, `ERR_SCHEDULE_CONFIG_NOT_EXIST`, `ERR_SCHEDULER_CONFIG`, `ERR_CACHE_OVERFLOW`, `ERR_INTERNAL_GROWTH`, `ERR_SCHEDULER_CREATE_FUNC_NOT_REGISTERED`, `ERR_SCHEDULER_TI_KVSPLIT_DISABLED`, `ERR_NO_STORE_TO_BE_SELECTED`, `ERR_EMPTY_REGION`, `ERR_DIAGNOSTIC_DISABLED`, `ERR_SCHEDULER_UNDIAGNOSABLE`, `ERR_NO_DIAGNOSTIC_RESULT`, `ERR_DIAGNOSTIC_LOAD_PLAN`, `ERR_RULE_CONTENT`, `ERR_LOAD_RULE`, `ERR_LOAD_RULE_GROUP`, `ERR_BUILD_RULE_LIST`, `ERR_PLACEMENT_DISABLED`, `ERR_KEY_FORMAT`, `ERR_RULE_NOT_FOUND`, `ERR_NOT_BOOTSTRAPPED`, `ERR_STORE_IS_UP`, `ERR_INVALID_STORE_ID`, `ERR_SCHEDULING_IS_HALTED`, `ERR_FEATURE_NOT_EXISTED`, `ERR_REDIRECT`, `ERR_OPTION_NOT_EXIST`, `ERR_OPTION_TYPE_INVALID`, `ERR_REDIRECT_NO_LEADER`, `ERR_REDIRECT_TO_NOT_LEADER`, `ERR_REDIRECT_TO_NOT_PRIMARY`, `ERR_SECURITY_CONFIG`, `ERR_TLSCONFIG`, `ERR_INIT_FILE_LOG`, `ERR_NO_CPUCONTROLLER_DETECTED`, `ERR_UNSAFE_RECOVERY_IS_RUNNING`, `ERR_UNSAFE_RECOVERY_INVALID_INPUT`, `ERR_PROGRESS_WRONG_STATUS`, `ERR_PROGRESS_NOT_FOUND`, `ERR_KEYSPACE_NOT_EXISTS`, `ERR_KEYSPACE_NOT_EXISTS_BY_NAME`, `ERR_RESOURCE_GROUP_NOT_EXISTS`, `ERR_DELETE_RESERVED_GROUP`, `ERR_INVALID_GROUP`, `ERR_NOT_FOUND_SCHEDULING_PRIMARY`, `ERR_SCHEDULING_SERVER`, `ERR_GCON_INVALID_KEYSPACE`, `ERR_DECREASING_GCSAFE_POINT`, `ERR_GCSAFE_POINT_EXCEEDS_TXN_SAFE_POINT`, `ERR_DECREASING_TXN_SAFE_POINT`, `ERR_GCBARRIER_TSBEHIND_TXN_SAFE_POINT`, `ERR_RESERVED_GCBARRIER_ID`, `ERR_GLOBAL_GCBARRIER_TSBEHIND_TXN_SAFE_POINT`, `ERR_IDEXHAUSTED`, `ERR_AFFINITY_GROUP_NOT_FOUND`, `ERR_AFFINITY_GROUP_EXIST`, `ERR_AFFINITY_GROUP_CONFLICT`, `ERR_AFFINITY_INTERNAL`, `ERR_AFFINITY_GROUP_CONTENT`, `ERR_INVALID_LABEL_RULE_FORMAT`, `ERR_EMPTY_REQUEST`, `ERR_INVALID_GROUP_ID`, `ERR_INVALID_KEY_FORMAT`, `ADDITIONAL_DOMAIN_ERRORS` |
| [lib.rs](pkg/errs/lib.rs) | `NOT_LEADER_ERR`, `NOT_SERVED_ERR`, `is_leader_changed`, `ErrorTemplate`, `new`, `code`, `template`, `instantiate`, `with_source`, `TypedError`, `code`, `template`, `args` |

任务 103 默认配置 **6 项通过**。测试文件：[catalog_test.rs](pkg/errs/catalog_test.rs), [error_test.rs](pkg/errs/error_test.rs)。

任务 103 实际运行测试：`error_test::typed_error_core`, `catalog_test::catalog_infrastructure_domains`, `catalog_test::catalog_service_domains`, `catalog_test::catalog_completeness`, `catalog_test::catalog_foundation_domains`, `catalog_test::catalog_scheduling_domains`。

### `pkg/utils/jsonutil` → `astersql-pd-jsonutil`

验证范围：新增/已有/null 字段、未知嵌套字段、合并标志、解析/序列化错误及失败后原值。

映射与边界：AddKeyValue → add_key_value；MergeJSONObject → merge_json_object<T: Serialize + DeserializeOwned>。不移植任意 interface{} 运行时布局；使用 serde 的公开 JSON 表示。

| Go 基准文件 | 导出 API |
| --- | --- |
| [jsonutil.go](pkg/utils/jsonutil/jsonutil.go) | `AddKeyValue`, `MergeJSONObject` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/utils/jsonutil/lib.rs) | `add_key_value`, `merge_json_object` |

任务 103 默认配置 **5 项通过**。测试文件：[jsonutil_test.rs](pkg/utils/jsonutil/jsonutil_test.rs)。

任务 103 实际运行测试：`jsonutil_test::merge_json_object_serialization_failure`, `jsonutil_test::add_key_value_existing_null_and_nested_unknown`, `jsonutil_test::add_key_value_flags`, `jsonutil_test::merge_json_object_invalid_inputs_are_unchanged`, `jsonutil_test::merge_json_object_fields_and_flags`。

### `pkg/balancer` → `astersql-pd-balancer`

验证范围：策略字符串、空节点默认值、轮询、重复插入/删除与并发。

映射与边界：PolicyRoundRobin/PolicyLeast → Policy::ROUND_ROBIN/LEAST；String → Display；GenByPolicy → gen_by_policy；NewRoundRobin → RoundRobin::new。Balancer 接口由 RoundRobin 方法表达；不新增 Least 实现（Go 工厂同样返回 RoundRobin）。

| Go 基准文件 | 导出 API |
| --- | --- |
| [balancer.go](pkg/balancer/balancer.go) | `Policy`, `PolicyRoundRobin`, `PolicyLeast`, `Policy.String`, `Balancer`, `Balancer.Next`, `Balancer.Put`, `Balancer.Delete`, `Balancer.GetAll`, `Balancer.Len`, `GenByPolicy` |
| [round_robin.go](pkg/balancer/round_robin.go) | `RoundRobin`, `NewRoundRobin`, `RoundRobin.Next`, `RoundRobin.GetAll`, `RoundRobin.Put`, `RoundRobin.Delete`, `RoundRobin.Len` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/balancer/lib.rs) | `Policy`, `ROUND_ROBIN`, `LEAST`, `Sealed`, `Node`, `RoundRobin`, `new`, `next`, `put`, `delete`, `get_all`, `len`, `is_empty`, `gen_by_policy` |

任务 103 默认配置 **5 项通过**。测试文件：[balancer_test.rs](pkg/balancer/balancer_test.rs)。

任务 103 实际运行测试：`balancer_test::round_robin_next`, `balancer_test::policy_and_membership`, `balancer_test::concurrent_membership`, `balancer_test::round_robin_next_concurrent`, `balancer_test::delete_duplicate_and_concurrency`。

### `pkg/window` → `astersql-pd-window`

验证范围：bucket/环形遍历、各 reducer、时间跨度轮转、计数器聚合与线程并发。

映射与边界：Metric/Aggregation/RollingCounter 接口 → 具体 RollingCounter；Options/RollingPolicyOpts → 构造参数，RollingCounterOpts 保留。Iterator.Next → has_next，Bucket.Next 的链指针不公开，改为 Window 索引；RollingPolicy.Reduce → RollingPolicy::reduce；RollingCounter::reduce 复用 policy 实现，保留只读聚合、过期时不调用回调及锁作用域。

| Go 基准文件 | 导出 API |
| --- | --- |
| [counter.go](pkg/window/counter.go) | `Metric`, `Metric.Add`, `Metric.Value`, `Aggregation`, `Aggregation.Min`, `Aggregation.Max`, `Aggregation.Avg`, `Aggregation.Sum`, `RollingCounter`, `RollingCounter.Timespan`, `RollingCounter.Reduce`, `RollingCounterOpts`, `NewRollingCounter`, `rollingCounter.Add`, `rollingCounter.Reduce`, `rollingCounter.Avg`, `rollingCounter.Min`, `rollingCounter.Max`, `rollingCounter.Sum`, `rollingCounter.Value`, `rollingCounter.Timespan` |
| [policy.go](pkg/window/policy.go) | `RollingPolicy`, `RollingPolicyOpts`, `NewRollingPolicy`, `RollingPolicy.Add`, `RollingPolicy.Reduce` |
| [reduce.go](pkg/window/reduce.go) | `Sum`, `Avg`, `Min`, `Max`, `Count` |
| [window.go](pkg/window/window.go) | `Bucket`, `Bucket.Append`, `Bucket.Add`, `Bucket.Reset`, `Bucket.Next`, `Iterator`, `Iterator.Next`, `Iterator.Bucket`, `Window`, `Options`, `NewWindow`, `Window.ResetWindow`, `Window.ResetBucket`, `Window.ResetBuckets`, `Window.Append`, `Window.Add`, `Window.Bucket`, `Window.Size`, `Window.Iterator` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [counter.rs](pkg/window/counter.rs) | `RollingCounterOpts`, `RollingCounter`, `new`, `with_clock`, `add`, `reduce`, `avg`, `min`, `max`, `sum`, `value`, `timespan` |
| [policy.rs](pkg/window/policy.rs) | `RollingPolicy`, `new`, `with_clock`, `add`, `reduce` |
| [reduce.rs](pkg/window/reduce.rs) | `Iterator`, `iterator`, `has_next`, `bucket`, `sum`, `avg`, `min`, `max`, `count` |
| [window.rs](pkg/window/window.rs) | `Bucket`, `append`, `add`, `reset`, `Window`, `new`, `size`, `bucket`, `append`, `add`, `reset_bucket`, `reset_buckets`, `reset_window` |

任务 103 默认配置 **9 项通过**。测试文件：[counter_test.rs](pkg/window/counter_test.rs), [policy_test.rs](pkg/window/policy_test.rs), [reduce_test.rs](pkg/window/reduce_test.rs), [window_test.rs](pkg/window/window_test.rs)。

任务 103 实际运行测试：`window_test::window_bucket_mutations`, `policy_test::rolling_policy_rotation`, `counter_test::rolling_counter_aggregates`, `counter_test::rolling_counter_aggregates_integer_conversion`, `reduce_test::iterator_and_reducers`, `window_test::bucket_add_requires_existing_point - should panic`, `window_test::empty_window_bucket_panics_like_go - should panic`, `counter_test::rolling_counter_aggregates_concurrent`, `counter_test::rolling_counter_concurrency`。

### `pkg/btree` → `astersql-pd-btree`

验证范围：节点分裂/重平衡、替换/删除/min/max、升降序边界、排名/索引、COW clone 和 freelist 生命周期。

映射与边界：BTreeG/FreeListG → BTree/FreeList；NewG/NewWithFreeListG → new/with_free_list；Item.Less → 比较闭包 F；ItemIteratorG → FnMut(&T)->bool；Int → 原生整数。DefaultFreeListSize 无同名常量，容量由 FreeList::new 显式传入；Clone 使用 Rust Clone，不丢失排名或 COW 语义。

| Go 基准文件 | 导出 API |
| --- | --- |
| [btree_generic.go](pkg/btree/btree_generic.go) | `Item`, `Item.Less`, `DefaultFreeListSize`, `FreeListG`, `NewFreeListG`, `ItemIteratorG`, `NewG`, `NewWithFreeListG`, `BTreeG`, `BTreeG.Clone`, `BTreeG.ReplaceOrInsert`, `BTreeG.Delete`, `BTreeG.DeleteMin`, `BTreeG.DeleteMax`, `BTreeG.AscendRange`, `BTreeG.AscendLessThan`, `BTreeG.AscendGreaterOrEqual`, `BTreeG.Ascend`, `BTreeG.DescendRange`, `BTreeG.DescendLessOrEqual`, `BTreeG.DescendGreaterThan`, `BTreeG.Descend`, `BTreeG.Get`, `BTreeG.GetWithIndex`, `BTreeG.GetAt`, `BTreeG.Min`, `BTreeG.Max`, `BTreeG.Has`, `BTreeG.Len`, `BTreeG.Clear`, `Int`, `Int.Less` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/btree/lib.rs) | `FreeList`, `new`, `len`, `is_empty`, `BTree`, `new`, `with_free_list`, `clear`, `replace_or_insert`, `delete`, `delete_min`, `delete_max`, `ascend`, `ascend_range`, `ascend_less_than`, `ascend_greater_or_equal`, `descend`, `descend_range`, `descend_less_or_equal`, `descend_greater_than`, `get`, `get_at`, `get_with_index`, `min`, `max`, `has`, `len`, `is_empty` |

任务 103 默认配置 **12 项通过**。测试文件：[btree_test.rs](pkg/btree/btree_test.rs)。

任务 103 实际运行测试：`tests::zero_degree - should panic`, `tests::invalid_degree - should panic`, `tests::clear_releases_payloads_and_reuses_empty_buffers`, `tests::descending_order`, `tests::clone_clear_and_freelist`, `tests::insert_get_and_len`, `tests::descending_iteration`, `tests::ascending_iteration`, `tests::split_replace_min_max`, `tests::delete_min_max`, `tests::delete_and_rebalance`, `tests::order_statistics`。

### `pkg/utils/keyutil` → `astersql-pd-keyutil`

验证范围：空端点/无穷边界、hex JSON、相邻/重叠、排序合并、差集、删除、HTTP 解码与错误分类。

映射与边界：NewKeyRange/NewKeyRanges → KeyRange::new/KeyRanges::new；JSON 方法 → serde；其余已实现行为转 snake_case。KeyRanges.Clean/NewKeyRangesWithSize/IsEmpty/Append → clean/with_capacity/is_empty/append；清空释放范围内容并保留集合容量，追加拥有边界副本。

| Go 基准文件 | 导出 API |
| --- | --- |
| [keyrange.go](pkg/utils/keyutil/keyrange.go) | `KeyRange`, `KeyRange.OverLapped`, `KeyRange.IsAdjacent`, `KeyRange.MarshalJSON`, `KeyRange.UnmarshalJSON`, `NewKeyRange`, `KeyRanges`, `KeyRanges.Clean`, `NewKeyRanges`, `NewKeyRangesWithSize`, `KeyRanges.IsEmpty`, `KeyRanges.Append`, `KeyRanges.SortAndDeduce`, `KeyRanges.Delete`, `KeyRanges.SubtractKeyRanges`, `KeyRanges.Ranges`, `KeyRanges.Merge`, `DecodeHTTPKeyRanges` |
| [util.go](pkg/utils/keyutil/util.go) | `BuildKeyRangeKey`, `BuildKeyRangeMaps`, `MaxKey`, `MinKey`, `MaxStartKey`, `MinEndKey`, `Between` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [http.rs](pkg/utils/keyutil/http.rs) | `HttpKeyRangesError`, `decode_http_key_ranges` |
| [key_range.rs](pkg/utils/keyutil/key_range.rs) | `KeyRange`, `new`, `over_lapped`, `is_adjacent` |
| [key_ranges.rs](pkg/utils/keyutil/key_ranges.rs) | `KeyRanges`, `new`, `with_capacity`, `clean`, `is_empty`, `append`, `ranges`, `sort_and_deduce`, `merge`, `subtract_key_ranges`, `delete` |
| [util.rs](pkg/utils/keyutil/util.rs) | `build_key_range_key`, `build_key_range_maps`, `max_key`, `min_key`, `max_start_key`, `min_end_key`, `between` |

任务 103 默认配置 **9 项通过**。测试文件：[http_test.rs](pkg/utils/keyutil/http_test.rs), [key_range_test.rs](pkg/utils/keyutil/key_range_test.rs), [util_test.rs](pkg/utils/keyutil/util_test.rs)。

任务 103 实际运行测试：`util::tests::key_boundary_comparison`, `http_test::decode_http_key_ranges_escape_errors`, `http_test::decode_http_key_ranges_success`, `key_range_test::sort_deduce_and_merge`, `http_test::decode_http_key_ranges_invalid_arguments`, `key_range_test::delete_key_ranges`, `key_range_test::subtract_key_ranges`, `key_range_test::key_range_codec_and_overlap`, `util::tests::key_range_map_builders`。

### `pkg/ratelimit` → `astersql-pd-ratelimit`

验证范围：token bucket/配置更新/等待取消、concurrency token、公平性、双维 admission、allow-list、QPS 指标及 runner 队列/去重/保留/超时/清理。

映射与边界：NewX → X::new；GetQPSLimiterStatus/GetConcurrencyLimiterStatus → qps_status/concurrency_status；Option/DimensionConfig → ControllerOption 枚举（AllowList、Concurrency、Qps、Dimensions、Init 变体）；DoneFunc → 显式 done。TaskOption/WithRetained → run_task_with_id 的 retained 参数，Task/Runner → 闭包和具体 runner。七个任务名字符串常量无同名导出，调用方传 name。Stop 取消并 join 自有 future，与 Go detached goroutine 生命周期有显式差异；测试证明 Rust 清理语义，不宣称逐调度等价。

| Go 基准文件 | 导出 API |
| --- | --- |
| [concurrency_limiter.go](pkg/ratelimit/concurrency_limiter.go) | `ConcurrencyLimiter`, `NewConcurrencyLimiter`, `ConcurrencyLimiter.GetRunningTasksNum`, `ConcurrencyLimiter.GetWaitingTasksNum`, `ConcurrencyLimiter.AcquireToken`, `ConcurrencyLimiter.ReleaseToken`, `TaskToken` |
| [controller.go](pkg/ratelimit/controller.go) | `Controller`, `NewController`, `Controller.Close`, `Controller.Allow`, `Controller.Update`, `Controller.GetQPSLimiterStatus`, `Controller.GetConcurrencyLimiterStatus`, `Controller.IsInAllowList` |
| [limiter.go](pkg/ratelimit/limiter.go) | `DoneFunc`, `DimensionConfig` |
| [metrics.go](pkg/ratelimit/metrics.go) |  |
| [option.go](pkg/ratelimit/option.go) | `UpdateStatus`, `LimiterNotChanged`, `LimiterUpdated`, `LimiterDeleted`, `InAllowList`, `Option`, `AddLabelAllowList`, `UpdateConcurrencyLimiter`, `UpdateQPSLimiter`, `UpdateDimensionConfig`, `InitLimiter` |
| [ratelimiter.go](pkg/ratelimit/ratelimiter.go) | `RateLimiter`, `NewRateLimiter`, `RateLimiter.Available`, `RateLimiter.Allow`, `RateLimiter.AllowN`, `RateLimiter.SetBurst`, `RateLimiter.SetLimit`, `RateLimiter.Limit`, `RateLimiter.Burst`, `RateLimiter.WaitN` |
| [runner.go](pkg/ratelimit/runner.go) | `HandleStatsAsync`, `ObserveRegionStatsAsync`, `UpdateSubTree`, `HandleOverlaps`, `CollectRegionStatsAsync`, `SaveRegionToKV`, `SyncRegionToFollower`, `Runner`, `Runner.RunTask`, `Runner.Start`, `Runner.Stop`, `Task`, `ConcurrentRunner`, `NewConcurrentRunner`, `TaskOption`, `WithRetained`, `ConcurrentRunner.Start`, `ConcurrentRunner.Stop`, `ConcurrentRunner.RunTask`, `SyncRunner`, `NewSyncRunner`, `SyncRunner.RunTask`, `SyncRunner.Start`, `SyncRunner.Stop` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [concurrency.rs](pkg/ratelimit/concurrency.rs) | `TaskToken`, `AcquireCancelled`, `ConcurrencyLimiter`, `new`, `acquire_token`, `release_token`, `get_waiting_tasks_num`, `allow`, `release`, `get_limit`, `set_limit`, `get_running_tasks_num`, `get_max_concurrency` |
| [controller.rs](pkg/ratelimit/controller.rs) | `ControllerOption`, `ControllerDone`, `done`, `Controller`, `new`, `with_metrics`, `run_metrics`, `close`, `is_closed`, `update`, `allow`, `concurrency_status`, `qps_status`, `is_in_allow_list` |
| [limiter.rs](pkg/ratelimit/limiter.rs) | `DoneFunc`, `done`, `UpdateStatus`, `Limiter`, `new`, `allow`, `concurrency_limiter`, `rate_limiter`, `concurrency_status`, `qps_status`, `update_concurrency_config`, `update_qps_config` |
| [rate_limiter.rs](pkg/ratelimit/rate_limiter.rs) | `INF`, `WaitError`, `RateLimiter`, `new`, `available`, `allow`, `allow_n`, `set_limit`, `set_burst`, `limit`, `burst`, `wait_n` |
| [runner.rs](pkg/ratelimit/runner.rs) | `RunnerError`, `code`, `error_template`, `ConcurrentRunner`, `new`, `with_queue_limits`, `pending_task_count`, `start`, `run_task`, `run_task_with_id`, `stop`, `SyncRunner`, `start`, `run_task`, `stop` |

任务 103 默认配置 **29 项通过**。测试文件：[concurrency_test.rs](pkg/ratelimit/concurrency_test.rs), [controller_test.rs](pkg/ratelimit/controller_test.rs), [limiter_test.rs](pkg/ratelimit/limiter_test.rs), [rate_limiter_test.rs](pkg/ratelimit/rate_limiter_test.rs), [runner_test.rs](pkg/ratelimit/runner_test.rs)。

任务 103 实际运行测试：`concurrency_test::concurrency_allow_release`, `concurrency_test::concurrency_tokens_cancel_head`, `concurrency_test::concurrency_tokens`, `limiter_test::limiter_config_updates`, `rate_limiter_test::token_bucket_available_does_not_consume_on_running_clock`, `limiter_test::combined_limiter_allow`, `controller_test::controller_labels_and_allowlist`, `rate_limiter_test::token_bucket_fractional_refill_and_burst_growth`, `rate_limiter_test::token_bucket_oracle_and_updates`, `rate_limiter_test::token_bucket_zero_infinite_and_concurrency`, `rate_limiter_test::token_bucket_wait_serializes_and_drop_refunds`, `controller_test::controller_qps_and_metrics`, `rate_limiter_test::token_bucket_wait_cancel_deadline_and_refund`, `controller_test::controller_labels_and_allowlist_parallel`, `limiter_test::combined_limiter_allow_parallel`, `runner::tests::runner_lifecycle_sync`, `runner::tests::runner_lifecycle_unlimited_and_parent_cancel`, `runner::tests::runner_lifecycle_stop_during_acquire_and_restart`, `runner::tests::runner_timeout_and_cancellation_age_boundary`, `runner::tests::runner_lifecycle_dispatch_and_release`, `runner::tests::runner_dedup_and_queue_unlimited_dispatches_without_completion`, `runner::tests::runner_lifecycle_completion_schedules_next`, `runner::tests::runner_dedup_and_queue_age_retained_capacity`, `concurrency_test::concurrency_tokens_parallel`, `concurrency_test::concurrency_allow_release_parallel`, `runner::tests::runner_lifecycle_panic_releases_token`, `runner::tests::runner_timeout_and_cancellation_burst_cleanup`, `runner::tests::runner_timeout_and_cancellation_concurrent_burst_drains`, `runner::tests::runner_dedup_and_queue_storage_reuse_release_fifo`。

### `pkg/cache` → `astersql-pd-cache`

验证范围：LRU、线程安全 facade、FIFO 分组、2Q recent/frequent/ghost、优先队列同优先级碰撞、TTL 边界/GC/取消/回收。

映射与边界：LRU/FIFO → Lru/Fifo；Type/LRUCache/TwoQueueCache/DefaultCacheType → CacheType 枚举及 new_default；NewCache/NewDefaultCache → Cache::new/new_default；NewIDTTL/NewStringTTL → TTLUint64/TTLString::new；PriorityQueueItem.ID → id；Entry.Less → 优先级比较。TTL 显式 shutdown；内部容器/锁使用标准库，不迁移 Go list/tree 实现细节。

| Go 基准文件 | 导出 API |
| --- | --- |
| [cache.go](pkg/cache/cache.go) | `Cache`, `Cache.Put`, `Cache.Get`, `Cache.Peek`, `Cache.Remove`, `Cache.Elems`, `Cache.Len`, `Type`, `LRUCache`, `TwoQueueCache`, `DefaultCacheType`, `threadSafeCache.Put`, `threadSafeCache.Get`, `threadSafeCache.Peek`, `threadSafeCache.Remove`, `threadSafeCache.Elems`, `threadSafeCache.Len`, `NewCache`, `NewDefaultCache` |
| [fifo.go](pkg/cache/fifo.go) | `FIFO`, `NewFIFO`, `FIFO.Put`, `FIFO.Remove`, `FIFO.Elems`, `FIFO.FromElems`, `FIFO.FromLastSameElems`, `FIFO.Len` |
| [lru.go](pkg/cache/lru.go) | `Item`, `LRU`, `LRU.Put`, `LRU.Get`, `LRU.Peek`, `LRU.Remove`, `LRU.Elems`, `LRU.Len` |
| [priority_queue.go](pkg/cache/priority_queue.go) | `PriorityQueue`, `NewPriorityQueue`, `PriorityQueueItem`, `PriorityQueueItem.ID`, `PriorityQueue.Put`, `PriorityQueue.Get`, `PriorityQueue.Elems`, `PriorityQueue.Remove`, `PriorityQueue.Len`, `Entry`, `Entry.Less` |
| [ttl.go](pkg/cache/ttl.go) | `ttlCache.Len`, `ttlCache.Clear`, `ttlCache.UpdateTTL`, `TTLUint64`, `NewIDTTL`, `TTLUint64.Get`, `TTLUint64.Put`, `TTLUint64.GetAllID`, `TTLUint64.Exists`, `TTLUint64.Remove`, `TTLUint64.PutWithTTL`, `TTLString`, `NewStringTTL`, `TTLString.Put`, `TTLString.PutWithTTL`, `TTLString.Pop`, `TTLString.Get`, `TTLString.GetAllID` |
| [two_queue.go](pkg/cache/two_queue.go) | `TwoQueue`, `TwoQueue.Put`, `TwoQueue.Get`, `TwoQueue.Peek`, `TwoQueue.Remove`, `TwoQueue.Elems`, `TwoQueue.Len` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [fifo.rs](pkg/cache/fifo.rs) | `Fifo`, `new`, `put`, `remove`, `len`, `is_empty`, `from_last_same_elems`, `elems`, `from_elems` |
| [lib.rs](pkg/cache/lib.rs) | `CacheType`, `Cache`, `new`, `new_default`, `put`, `get`, `peek`, `remove`, `elems`, `len`, `is_empty` |
| [lru.rs](pkg/cache/lru.rs) | `Item`, `Lru`, `new`, `put`, `get`, `peek`, `remove`, `elems`, `len`, `is_empty` |
| [priority_queue.rs](pkg/cache/priority_queue.rs) | `PriorityQueueItem`, `Entry`, `PriorityQueue`, `new`, `put`, `get`, `peek`, `tail`, `remove`, `len`, `is_empty`, `elems` |
| [ttl.rs](pkg/cache/ttl.rs) | `TTLUint64`, `TTLString`, `TtlCache`, `new`, `put`, `put_with_ttl`, `get`, `exists`, `get_all_id`, `pop`, `remove`, `len`, `is_empty`, `clear`, `update_ttl`, `shutdown` |
| [two_queue.rs](pkg/cache/two_queue.rs) | `TwoQueue`, `new`, `with_ratios`, `put`, `get`, `peek`, `remove`, `elems`, `len`, `is_empty` |

任务 103 默认配置 **17 项通过**。测试文件：[cache_test.rs](pkg/cache/cache_test.rs), [ttl_test.rs](pkg/cache/ttl_test.rs)。

任务 103 实际运行测试：`cache_test::fifo_last_same_group`, `cache_test::fifo_behavior`, `cache_test::two_queue_behavior`, `cache_test::lru_behavior`, `cache_test::two_queue_behavior_ghost_capacity_and_ratios`, `cache_test::priority_queue_put_update`, `cache_test::priority_queue_query_remove`, `cache_test::fifo_behavior_empty_remove_panics - should panic`, `cache_test::thread_safe_cache_factory`, `ttl_test::ttl_cache_lifecycle_update_and_revival`, `ttl_test::ttl_cache_lifecycle_expiration_boundaries`, `ttl_test::ttl_cache_lifecycle_drop_releases_values_and_preserves_parent`, `ttl_test::ttl_cache_lifecycle_gc_restarts_and_cancels`, `cache_test::lru_behavior_unbounded_and_shared_values`, `ttl_test::ttl_cache_lifecycle_concurrent_put_pop`, `cache_test::fifo_behavior_concurrent_insertion`, `cache_test::thread_safe_cache_factory_concurrent_access`。

### `pkg/core/rangetree` → `astersql-pd-rangetree`

验证范围：Find/索引、空树、重叠与邻接、debris 重建、Remove/ScanRange 和随机区间模型。

映射与边界：NewRangeTree → RangeTree::new；RangeItem 保留领域 trait；DebrisFactory → 更新时传入的闭包；Go bool/接口 nil → bool/Option。无业务 API 排除。

| Go 基准文件 | 导出 API |
| --- | --- |
| [range_tree.go](pkg/core/rangetree/range_tree.go) | `RangeItem`, `RangeItem.Less`, `RangeItem.GetStartKey`, `RangeItem.GetEndKey`, `DebrisFactory`, `RangeTree`, `NewRangeTree`, `RangeTree.Update`, `RangeTree.GetOverlaps`, `RangeTree.Find`, `RangeTree.Remove`, `RangeTree.Len`, `RangeTree.ScanRange`, `RangeTree.GetAdjacentItem`, `RangeTree.GetAt`, `RangeTree.GetWithIndex` |

| Rust 文件 | 公开声明/方法 |
| --- | --- |
| [lib.rs](pkg/core/rangetree/lib.rs) | `RangeItem`, `RangeTree`, `DebrisFactory`, `new`, `with_factory`, `update`, `from_items`, `find`, `get_overlaps`, `remove`, `scan_range`, `get_adjacent_item`, `len`, `is_empty`, `get_at`, `get_with_index` |

任务 103 默认配置 **6 项通过**。测试文件：[range_tree_test.rs](pkg/core/rangetree/range_tree_test.rs)。

任务 103 实际运行测试：`tests::update_with_debris_filters_invalid_children_and_inserts_new_last`, `tests::remove_scan_and_adjacent`, `tests::overlap_query`, `tests::update_with_debris`, `tests::find_len_and_index`, `tests::update_with_debris_unbounded_and_many_overlaps`。

## 缺口与后续验收限制

任务 103 发现的 KeyRanges 集合辅助方法、RollingPolicy.Reduce、NotLeaderErr/NotServedErr/IsLeaderChanged 和 KeyspaceModes() 的直接入口已由任务 104 补齐，各有真实红绿验证。KeyspaceModes 是 Go 返回数组值的函数，原审计称其为可变导出有误。反射字段描述、原生 Clone、所有权、构造参数与 trait 等 Rust 接口边界仍按各包映射执行；这些边界不构成 Go ABI 兼容承诺。transport/log 转换仍留适配层，Go 生产调用方未切换。

任务 103 仅新增本文，未修改 Go/Rust 代码、Cargo 配置或模块依赖；当时没有启用 failpoints，无生成源文件或临时构建产物提交。其他会话已有任务删除与 prompt.md 变更保持原样。
