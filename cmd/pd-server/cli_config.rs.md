# CLI 与配置文件合并

`cli_config.rs` 是独立进程入口的文件 I/O 和参数适配层，复用
`astersql-pd-server-config` 的解码、废弃字段迁移与默认调整。

参数支持 `--key=value`、`--key value`、布尔开关和 `-L`。非空 CLI
字符串覆盖 TOML，空字符串保留文件值；true 覆盖布尔值，false 不覆盖
文件中的 true，与 Go `Config.Parse` 一致。主机名与工作目录由宿主读取；
不增加环境变量到字段的隐式绑定。

`Config::security` 保留完整配置并传入现有 listener/telemetry 装配。
`--config-check` 只校验和输出警告，不创建 executor、listener 或连接存储。
外部 MetaStore 后端在启动前明确拒绝用户显式设置的 data-dir、initial-cluster
和 force-new-cluster，不模拟 embedded-etcd 的拓扑和本地持久化。

行为测试显式注册在 `lib.rs` 的 `cli_config_test` 模块；真实 Go/Rust
进程差分入口为 `scripts/cli-config-oracle.py`，证据见
`docs/rust/server-api/task-090.md`。help/完整退出码契约仍由任务91覆盖，
日志宿主安装由任务92负责，本模块仅合并配置值。
