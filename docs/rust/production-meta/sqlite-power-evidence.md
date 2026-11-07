# SQLite WAL 掉电实验回执

基于 `85f650ecf9bf0cf1e8b8bc7b0948b1b5b969086e`。SQLite 3.50.2 的 source ID
与原提案 rusqlite 0.37.0 对应的 `libsqlite3-sys 0.35.0` amalgamation 相同。
同样实验也运行于 3.51.3。上游在 3.51.3 修复了多连接同时写入或 checkpoint
时可能触发的 WAL reset 损坏，单连接实验无法覆盖该竞态。
[SQLite WAL reset 公告](https://sqlite.org/wal.html#the_wal_reset_bug)。

实验使用 APSW 将故障模型放在 SQLite 实际的 VFS `xWrite`、`xTruncate` 和
`xSync` 回调。写入先进入文件的易失视图，正常同步按顺序提交到临时普通文件。
单连接以独占锁使用 WAL，关闭自动 checkpoint。切断时复制同步镜像到独立恢复
目录，然后用默认 VFS 重开数据库，运行 `integrity_check`，读取 `applied` 和
`revision`，与单独同步的客户端确认记录对账。恢复镜像在关闭写入连接前冻结，
所以关闭连接时的 checkpoint 无法改变对账结果。
[APSW VFS 文档](https://rogerbinns.github.io/apsw/vfs.html)。

| 场景 | 提交和确认 | 重开后两条记录 |
| --- | --- | --- |
| WAL `FULL` | 同步后确认版本 2 | `applied=2, revision=2`，无确认丢失 |
| WAL `NORMAL` | 已启动的 WAL 内追加事务，确认版本 2 时未发生 WAL 同步 | 两条均为 1，检出版本 2 确认丢失 |
| `FULL` 同步拒绝 | WAL 同步返回 `SQLITE_IOERR_FSYNC`，无确认 | 两条均为 1 |
| `FULL` 同步虚报成功 | SQLite 得到成功，客户端确认版本 2 | 两条均为 1，检出版本 2 确认丢失 |
| `FULL` 同步中 WAL 帧撕裂 | 同步时持久化帧头和半页，返回 `SQLITE_IOERR_FSYNC`，无确认 | 两条均为 1，撕裂的新帧未被应用 |

两组均运行 `python -B docs/rust/production-meta/check_sqlite_power.py`，退出码 0。
环境为 Darwin arm64，Python 3.9.6 + APSW 3.50.2.0 及 Python 3.13.18 +
APSW 3.51.3.0。APSW 从 PyPI wheel 安装到独立临时虚拟环境。
10 个场景校验通过，0 失败、0 忽略，其中 4 个场景验证预期的确认丢失。
完整跟踪包含 SQLite source ID、每次写入的偏移/大小/摘要、同步结果、确认和
切断镜像的摘要。WAL salt 随运行变化，复跑比较对账结果和操作边界。
[脚本](check_sqlite_power.py)、[原始跟踪](sqlite-power-evidence.json.gz)。

这是单节点 SQLite 引擎证据。设备模型把命名空间变更视为即时生效，尚未模拟
目录项丢失、任意重排、多个连接并行或宿主掉电。Rust 绑定、OpenRaft 日志和
多数派确认链仍需单独接线和验证。此回执没有满足原生产任务9的完整验收。
