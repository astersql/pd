# 任务003：配置指标与命令契约验收

状态：已完成（文档盘点）；配置、指标、命令与安全行为仍按owner逐项待验收。

基准HEAD：`ca5ae0ce726d54df173fe56f6046f08c20425b32`。依赖任务1、2已完成文档且source-inventory/wire-inventory/task-001/task-002均已读取；任务1确认的主题6准入不代替本计划差分。开始已有编号任务1、2删除，原样保留，未纳入本任务提交。

交付：[surface-inventory.md](surface-inventory.md)。

## 范围与审阅口径

源码配置字段按结构名与文件/行定位；默认常量与完整Adjust/Validate/废弃兼容函数原文保留控制条件。CLI以显式注册点去重，共享addFlags对根及services api共用；独立MCS作用域分别列出。指标以构造位置去重，保留Namespace/Subsystem/Name/Help及完整labels/桶配置原文，不按指标名错误合并不同进程。生产server/pkg全量构造独立复核；client controller嵌套配置带入的16项独立指标也明确归主题8。注册/采集与安全入口另列，不声称全量构造都实际暴露。

| 集合 | 数量 |
| --- | --- |
| config | 247 |
| frozen_leaves | 170 |
| flags | 62 |
| env_merge | 40 |
| metric_constructors | 187 |
| registration_security | 812 |
| tests_injection | 862 |
| 来源文件（相对路径排序去重） | 267 |

源码事实、冻结Go输出、实际运行结果分栏；170个冻结叶子来自nested_config_go.json的missing案例，保留显式TOML条件、旧HEAD与binary_sha256，未重跑、未将其宣称为普适默认值。每行沿用清单共同准入、测试矩阵、owner证据位置；缺失新差分全部标明。外部log/metering_sdk/Go collector/grpcprometheus/etcd实际输出须在4/86/90/91/92/101及主题9/10继续固定，清单完成不解除准入。

## 本次真实运行的文档检查

| 命令 | 退出码 | stdout行数 | stdout SHA-256 |
| --- | --- | --- | --- |
| `git rev-parse HEAD` | 0 | 1 | 959af80d0a685fbe5d7d0e330e3356a659dc54910f59b6d2e730a6bf0a3bd369 |
| `rg --files server pkg cmd/pd-server tests/server client` | 0 | 2299 | 43ed17cda5e3a28b5879ee6a78f62456b39f9396f65263dfab8891c554daeabd |
| `rg -n 'failpoint\.\|tokio::spawn\|go ' server pkg cmd/pd-server` | 0 | 1213 | 1f3876a7509bc48209c8fee661a93effc54143984034cb0fe450c0ffd88fab0f |

`python3 /tmp/pd-task003.py`：退出0，Python标准库只读解析与文档生成；临时脚本不是服务实现。`python3 /tmp/pd-task003-verify.py`：检查逐行链接/行锚点、manifest字节哈希、案例唯一性、每行owner非空、server/pkg生产指标构造全覆盖、Go/模块集合及plan.md只读。最终数量与结果见下方补记。

来源manifest摘要：`ce6e64c8cf56aced73e1bb88d6c4eaf14796cfa411d813d9ede578ace3e4843e`。每个来源的原字节SHA-256见清单，未读取任何真实环境变量/凭证。没有执行Cargo、Go、make或failpoint启停；新行为测试0，TDD与Cargo共享槽位不适用。

## 领域边界与交接

主题8拥有独立MCS、keyspace/GC/encryption/controller领域，78/102只能接线/边界验收；主题9拥有audit、profiling、日志/调优/Go运行时观测能力，81/86/92负责接线；主题10保留原生持久化和Go embedded-etcd拓扑等价性。当前Rust配置库可复用，CLI仍最小解析器；TLS/认证/审计和完整指标尚无本任务新运行证据。没有发现需新增来源主题的新需求，没有改写来源史诗、plan.md或任何Go/模块文件。

## 最终核验补记

- 验证脚本退出0：4930个文件/行锚点链接有效，267个manifest文件存在、唯一且哈希匹配；案例ID与数量、每行owner非空检查通过。
- 独立扫描server/pkg生产源码的171个prometheus构造位置全部覆盖；清单187项还包括嵌套controller来源中的16项client指标，均明确移交。
- 开始/结束含ignored/untracked的Go/模块共1983文件，路径集合与每文件原字节SHA-256完全一致；排序(path,hash) JSON摘要 `312d43bcba3afe6923950b4530bebe749de68040540b451f5872acedab8be18e`。Go/模块Git状态没有差异；只读plan.md和来源史诗哈希相同。
- `git diff --check`：退出0；提交前再对两份新增文档运行`git diff --cached --check`。没有提交编号任务1、2的既有删除或其它无关文件。
