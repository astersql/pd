# 统计维度与负载换算

对应只读 Go `kind.go`、`constant.go`、`labels.go`，基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。
[任务证据](../../../docs/rust/state/task-01.md)区分真实 Go 运行结果和源码补充契约。

- 四维依次 byte/key/query/cpu；未知名称回退 byte，未知维度字符串为空。
- RegionStatKind 0..7 保留历史编号，read CPU=6、write CPU=7；StoreLoadKind 0..11，read CPU=11。
  SourceKind、RWType、ActionType 均为开放整数值，`from_i32/as_i32` 保留未知值，Display沿用Go文本。
- WRITE/READ 的 region_stats 分别为 `[3,4,5,7]`/`[0,1,2,6]`；无效值返回空切片。
  inverse 仅 WRITE 返回READ，其余返回WRITE。report_interval 为WRITE60秒，其余10秒；
  anti count 为WRITE2，其余12。foreach按READ后WRITE，每类byte/key/query/cpu访问。
- get_load_rates选择完整delta的实际索引，然后除以调用方提供的秒数；零间隔保留IEEE NaN/±Inf。
  不追加CPU百分比、毫秒或其它隐式单位换算。无效RW返回四个零，不访问输入。
- set_full_load_rates只更新对应方向，保留另一方向；输入是彼此独立的借用切片。
  选中索引或对应load不存在时panic，与Go越界一致，不预校验或保证panic前的原子性。
  Go重叠slice的逐元素别名效果不暴露为Rust安全API，调用方应传独立源快照。
- MIN_HOT_THRESHOLDS为 `[8192,128,128,1024,32,32,1,1]`，byte阈值采用KiB=1024。
  默认窗口为1/5/5；常量默认值不可变，需要定制阈值的调用方持有自己的数组。
- single_label返回一个拥有key/value的BTreeMap；允许空值与Unicode，结果互不共享。
  不创建全局标签、Prometheus Registry或指标。

纯领域值没有I/O、后台任务或存储后端；没有protobuf、FFI、私有runtime或调度决策依赖。
