# 多维 TopN 集合

来源：同目录 `topn.go` 的 TopN、singleTopN、indexedHeap；运行与测试证据见
[任务2](../../../docs/rust/state/task-02.md)。

`TopNItem` 提供 `id() -> u64` 和 `less(dimension, other)`；比较必须是严格弱序，
驻留期间不可通过内部可变性改变 ID 或比较值。值变化通过 `TopN::put` 替换。
不强加浮点类型、protobuf、单位转换、相等值的稳定排名或 ID 次序。

`TopN::new(dimensions,n)` 要求两者大于0；`put(T)` 返回是否更新，`remove(id)`
返回最新对象或 None；所有维度在一次独占 `&mut self` 调用中一起维护。
每维 selected 是最小堆，rest 是最大堆；哈希索引允许按ID修复/删除，单次修改
O(dimensions × log(items))，读取最小对象O(1)，复制集合O(返回项数)。
不足N时填满，有余项且比selected最小值大时交换，严格相等不触发交换。

`get`、`get_all`、`get_all_top_n`、`get_top_n_min` 返回 Arc 的独立快照，
调用者保留旧对象不妨碍替换或删除；所有维度共享同一新对象。数组无序，
缺失对象及空最小项返回None，无效维度panic。空集合支持N=1和任意u64 ID。

Rust的独占借用替代Go内部RWMutex；跨线程宿主可持有标准RwLock，库不安装
全局状态、运行时或后台任务。TTL属于后续批次的独立策略；本接口没有隐式过期。
