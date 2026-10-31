# CLI 信息输出与退出分类

`cli_exit.rs` 是独立进程的参数语法与信息输出适配器。配置flag清单及布尔语义
复用 `cli_config.rs`，字符串参数归一为 `--key=value` 后交回既有配置合并。

先完整校验语法，再处理help；version在配置校验成功后输出。help/version与
config-check不要求外部etcd。帮助明确Rust使用外部MetaStore，不能嵌入etcd；
版本显示当前Cargo包版本与可选构建元数据，不复制Go oracle的版本值。

`main.rs`持有实际退出分类和信号处理：语法错误stdout/1，配置错误stdout/0，
启动错误stdout/1；过渡模式缺外部etcd参数stderr/1。SIGTERM清理后退出0，
SIGINT/SIGHUP/SIGQUIT清理后退出1。库领域层不退出进程。

独立行为测试为 `cli_exit_test.rs`，真实二进制差分为
`scripts/cli-exit-oracle.py`，证据见 `docs/rust/server-api/task-091.md`。
