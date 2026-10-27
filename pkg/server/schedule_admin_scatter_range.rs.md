# 范围散射配置传输边界

`schedule_admin_scatter_range::serve` 接受 canonical `ScatterRangeScheduler`、
`StorageEndpoint`、`ClusterPaths` 和 axum 请求；宿主负责同步、挂载路径与线上 HEAD body。
领域 scheduler 不依赖 HTTP，更新继续使用其现有 `update`/`persist`，没有复制调度算法。

只读 Go `scatter_range.go` 定义 POST `/config` 和 GET `/list`；其它方法405且无Allow，
未知路径404。配置列表按声明字段顺序缩进输出，保留原始字符串，不作URL/hex解码，
按Go默认JSON编码转义HTML和U+2028/U+2029。成功更新响应为JSON `null`。
`null`/空对象仍保存；非字符串字段保持原值；range-name不能改变，返回500且不更新。
JSON语法和顶层类型错误返回400与input错误header；请求流I/O错误返回500/internal，
不使用已读的部分输入。错误stack保留Rust调用栈，按语言相关诊断字段归一化。

Go先发布内存配置再保存；失败不会回滚，并继续写入 `null`。
适配层保留500与错误JSON字符串后追加 `null` 的响应，不能把此body当成单个JSON值。
错误文本使用实际后端错误；恢复后reload可回到旧持久化状态，再次更新可成功。

[验收证据](../../docs/rust/schedule-execution/task-148.md) 与固定fixture来自用户授权的
Go源码/测试推导；未执行Go、没有Go运行差分。独立测试覆盖HTTP方法/路径/JSON、
存储/请求流故障、转义字节和HTTP更新后真实leader/region调度，不安装完整server路由。
