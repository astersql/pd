# StoreConfig

对应 `store_config.go`，属于 canonical `astersql-pd-schedule-config`。

公开值：`StoreConfig`、`Coprocessor`、`Storage`、`ReadPool`、`UnifiedReadPool`、
`RAFTSTORE_V2`、`MergeAgainError::{Size, Keys}`。`Clone` 复制所有拥有值；
`PartialEq/Eq` 只比较三个原始配置段，忽略三个 MiB 派生缓存。

`adjust` 复用 typeutil 的单位解析和默认值。get_region_max_size、
get_region_split_size、get_region_max_keys、get_region_split_keys、
is_enable_region_bucket、set_region_bucket_enabled、get_region_bucket_size、
is_raft_kv2、get_unified_read_pool_max_thread_count 对应 Go 同名 getter/setter。
非空尺寸在 adjust 前读取缓存零值，不隐式重新解析；空值 getter 返回默认值。
Rust 接收器是有效拥有值；Go nil 接收器用零配置表达等价读取默认值，
不引入 nullable 领域对象。负数 key 阈值按 Go uint64 转换保留。

check_region_size/check_region_keys 使用分裂余数检查：低于 max 直接成功；
余数为零成功；非零余数小于等于 merge 阈值失败。尺寸 split 为零避免除零。
错误 Display 保留 Go RFC code 和完整模板文本。

Serde 保留 JSON 段、连字符字段名和未知字段容忍，缓存不序列化。
新配置快照中的 null 使用零值；不提供对已有对象的 Go JSON 增量更新接口。
Display 使用两空格 JSON；对象字段顺序与结构声明一致。
无 proto、HTTP、存储、故障注入、全局状态或后台任务。

独立 `store_config_test.rs` 显式注册四项测试；Go 运行证据、源码推导边界、
红绿及 workspace 验证见 [10-证据](../../../../docs/rust/schedule/config/10-证据.md)。
