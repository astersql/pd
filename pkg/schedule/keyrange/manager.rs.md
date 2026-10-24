# Manager：范围追加、删除与补集

`Manager::new` / `Default` 创建空集合；`is_empty` 在锁内读取集合；
`append(&[KeyRange])` 复制边界，在同一 `Mutex<KeyRanges>` 临界区追加并调用
canonical `astersql-pd-keyutil::KeyRanges::sort_and_deduce`。

排序依据为起始字节。只合并相邻边界，重复与普通重叠不做集合并集；空边界遵循
keyutil 既有语义。每次 append（包括空批次）均重新归一。不引入协议、持久化、
后台任务或运行时。Rust 拥有边界，调用者后续修改输入不会影响容器。

Mutex 中毒表示内部 panic，后续访问按 Rust 常规方式 panic；正常领域行为不返回错误。
独立测试在 manager.rs 以 path 显式注册，测试只读内部快照以区分重复/重叠和相邻合并。
`delete(&[KeyRange])` 在同一临界区按输入顺序调用 canonical `KeyRanges::delete`，
裁剪或拆分交叠项，不额外排序。`get_non_overlapping_key_ranges(&KeyRange)` 在锁内
调用 `subtract_key_ranges`，返回拥有边界的补集快照。读者不会看到删除批次的中间状态。
空端点保留 keyutil 的 Go 比较语义；空集合对全空 base 返回空列表。
追加的对称相邻判定可能把 empty..c 与 e..empty 归一为 empty..empty；删除作用于归一后的集合。
删除/补集独立测试在 manager.rs 显式注册，运行 oracle 与验证见
`docs/rust/schedule/keyrange/3-证据.md`。
