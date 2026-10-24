# Builder 与 List

对照 builder.go 和 range_list.go；Builder<T: Clone + PartialEq> 持有有序事件，
new/default、set_compare_func、add_item、build 对应 Go 构建入口。空 end 表示正无穷，
空 start 是最小字节键；不验证倒置范围或丢弃空长度范围。删除首个 PartialEq 相等值，
需要指针身份时调用方应提供具有身份相等语义的领域值。比较器返回 Ordering，
相等项插在当前相等项之后。List 拥有每个 segment 的独立数据快照，提供 len/is_empty/get。
查询和分裂点留给任务 5。get 越界与 Go 一致 panic，借用输出防止调用方篡改内部状态。

事件排序复用 event_sort.rs 中 Go 1.26 pdqsort 的行为移植，以保持没有比较器时
同键事件重排对数据次序的影响；不能换成 Rust 稳定排序。所有领域类型无外部依赖。
