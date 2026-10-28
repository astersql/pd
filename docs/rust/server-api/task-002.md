# 任务002：协议与HTTP路由归属验收

状态：已完成（仅文档盘点）；所有未验收领域/服务仍保持待验收。

基准HEAD：`72b7574b1a9a0ce968cdab855775e24ef03c4d9f`。依赖任务1的 source-inventory.md/task-001.md 已存在且已读取；任务3尚未生成，不作前置。开始已有任务1文件删除，保留该变更；本任务不修改plan.md、来源主题文件、Go/模块，不执行Cargo/Go/make/failpoint。

交付：[wire-inventory.md](wire-inventory.md)。

| 集合 | 数量 |
| --- | --- |
| PD descriptor方法 | 56 |
| 其它descriptor服务 | 10 |
| 其它descriptor方法 | 42 |
| v1方法展开行（含2个deprecated） | 172 |
| v2行（含展开探针） | 45 |
| scheduler子路由注册行 | 31 |
| RM挂载路由行 | 15 |
| HTTP总行 | 263 |
| 注入形式文本命中 | 172 |
| 来源manifest去重文件 | 365 |

## 新运行验证

| 命令 | 退出码 | stdout行数 | stdout SHA-256 |
| --- | --- | --- | --- |
| git rev-parse HEAD | 0 | 1 | 09414d617a920fbbeffa7f8cf35732a5781598048d34f8d256afe1298ad4dc5d |
| rg --files server pkg cmd/pd-server tests/server client | 0 | 2299 | 67b638473291190a10bd5daf7571c8d271cf22ce3fb2164e22d1c9b84104edc0 |
| rg -n failpoint\.\|tokio::spawn\|go  server pkg cmd/pd-server | 0 | 1213 | 6f3670d7d0c7d73a90f5ca119c8f0c995b630e56a487e745c7c2b7d9832617cc |

只读Python标准库解析已有FileDescriptorSet（length-delimited/varint），PD全部方法顺序和名称与固定pdpb.proto一致；表内输入输出、client/server流位直接来自descriptor。额外服务全量列出注册与未注册区别；未把descriptor存在当服务暴露。v1按HTTP方法展开，ANY保留；v2动态livez/readyz展开；调度器动态实例保留注册来源，RM按实际classic挂载列出。

每行均有owner、准入、差分案例及缺失证据位置；Go事实/冻结输出/运行证据分栏口径见清单开头。没有声称未运行行为通过。主题8/9/10仍未准入，盘点完成不允许跳过任务4与领域验收。

来源集合SHA-256：`19c692a402d9a234dc083ada3e346aa1f247f25f8b13f47363c3d6af91b8b501`。协议ZIP预期值见build.rs，未重新下载；proto原字节与缓存descriptor逐文件摘要见清单。缓存descriptor与固定proto的方法签名/流标记将作最终复核。

最终逐行链接/owner/去重/只读状态及diff核验补记如下。

## 最终核验补记

- `python3 /tmp/pd-task002-verify.py`：退出0；Python标准库只读核验，没有构建或行为测试。核验器逐行解析Markdown链接与行锚点、manifest原字节SHA-256；从固定proto解析service/rpc签名与双向stream位，逐项与descriptor生成表匹配；按源码literal注册点逐条检查覆盖，动态探针另展开。临时核验脚本不是服务产物。
- 882个文件/行锚点链接全部有效；365个manifest路径存在、唯一且SHA-256完全相符；集合摘要 `19c692a402d9a234dc083ada3e346aa1f247f25f8b13f47363c3d6af91b8b501`。
- 98个方法签名全部匹配（56个PD + 42个额外/传递导入方法）；263条HTTP行全部有有效owner及唯一案例ID，v1 literal/v2 literal/动态探针/scheduler委托注册无遗漏。保留同一路径不同方法和同名动态调度器不同来源，不能把263解释为263个独立可访问URI。
- 固定kvproto ZIP只读SHA-256与build.rs预期完全一致；8个生成根proto的模块缓存字节与ZIP原始entry完全一致；导入deadlock/tracepb同样纳入descriptor签名核对和协议源SHA-256表。未下载或调用生成器。
- 172条注入形式文本命中逐源定位；辅助宏/测试/注释不当作独立注入或通过场景。
- 开始与结束Go/模块集合共1983文件（含ignored/untracked），路径集合及每文件字节哈希完全一致；排序JSON集合摘要 `300a4b0fdad52dd6b67f5daa919a1720ca47c97ecc7a81d364f6896f12d094a2`。开始和最终Git状态均无Go/模块差异。manifest内只读plan.md与来源主题7/8/9/10逐文件哈希一致。
- `git diff --check`：退出0。提交前另执行 `git diff --cached --check`，仅暂存本任务两份文档；不提交任务1删除。
- 新行为测试0；Go实际运行0；Cargo命令0；TDD及Cargo槽位不适用于纯文档盘点。来源任务状态与领域准入均未修改。
