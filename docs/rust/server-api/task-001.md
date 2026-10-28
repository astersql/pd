# 任务001：服务依赖与覆盖基准验收

状态：已完成（文档盘点）；服务实现及缺失领域准入仍未完成。

基准HEAD：`bdec774cc277fc2bb65ffa44cfbfb1849bfbe0e2`。开始Git状态干净，分支asterpd。未修改只读plan.md、来源史诗、Go文件或Go模块；未执行Cargo、Go、make或failpoint启停。

交付清单：[source-inventory.md](source-inventory.md)。

## 新运行的文档验证

- `git rev-parse HEAD`：退出0，值见上。
- `rg --files server pkg cmd/pd-server tests/server client`：退出0，2299条、2299个唯一路径。
- `rg -n 'failpoint\.|tokio::spawn|go ' server pkg cmd/pd-server`：退出码与命中数见最终核验补记。
- 来源集合：1909项，直接服务生产文件182项，导出声明1392项。按路径去重，原字节SHA-256，完整manifest摘要`a6765e82a34496928c803b5cd616955b17c430bd5ebd243aa16239c9c5cdee91`。
- 直接生产扫描：worker候选49条、所有failpoint形式94条、持久化候选304条。每行有owner；返回辅助宏与注释不视为独立注入点。
- 102个任务均有来源/当前目标/准入/所需测试/证据缺口映射。直接来源公开声明及worker/持久化/failpoint定位完整列入source-inventory；协议与路由详表归任务2，配置/指标/CLI详表归任务3。

## 主题6准入证据重新核验（历史运行，不是本次重跑）

`repaired-provenance.json` sources校验：179项匹配，0项发生后继变更。
后继差异：无。来源及当前版本均已冻结；差异不自动推翻原始日志，也不代表当前服务通过历史测试。
`dependency-audit.json`记录23个领域入口的normal传递依赖没有proto/prost/tonic/server/runtime路径；`normal-tree.json`是历史Cargo真实树。task-156的授权只覆盖当时源码推导+Rust真实测试，不替代主题7新Go差分。

| 历史实际命令 | 退出码 | 通过数 | 原日志（相对task_156） | 压缩日志SHA-256 |
| --- | --- | --- | --- | --- |
| `cargo fmt --all` | 0 | 0 | `repaired-0.log.gz` | `5b86412802ffc8d14e8cf895dc9b65c51d3a7238fc519c952ae69d348e821bdc` |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_acceptance -- --nocapture` | 0 | 4 | `repaired-1.log.gz` | `bf99c88df5efeea1bb43ddf84e757a307301e17f6e299dad69a808738849790b` |
| `cargo test -p astersql-pd-compat-tests --locked` | 0 | 33 | `repaired-2.log.gz` | `cdcc1a8997bfe061a61e8f491843485ceee6a5d2c5a1aba36f4df0c0dc548557` |
| `cargo metadata --format-version 1 --no-deps` | 0 | 0 | `repaired-3.log.gz` | `0c2462f83edf576b2fa9b57f36054ed47359e38b90b53c3c66f3a080311070ca` |
| `cargo fmt --all -- --check` | 0 | 0 | `repaired-4.log.gz` | `f72dfaa05f3158aa47d683dfd1f65c3eea5d65cb85677ee0b51d59fc3ac1d65f` |
| `cargo check --workspace --locked` | 0 | 0 | `repaired-5.log.gz` | `0e1fe4c76fede21a5a274dc400a3211eb217cc61241e872dc70f17f0a978be33` |
| `cargo test --workspace --locked` | 0 | 1925 | `repaired-6.log.gz` | `194872693a4c842ba490729ad2f12183fee966e1b74bc8f047d913162a53e240` |
| `cargo tree --workspace --locked` | 0 | 0 | `repaired-7.log.gz` | `b1cfb46d7dab4bc322749182c96b15b4a2d6ab305f4c077aedf4a1ddcc5b0b5b` |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_058 -- --nocapture` | 0 | 8 | `repaired-8.log.gz` | `95fa04e3c47a48d273bd897b4f372411942f49a55acaf54d0df87a5ee9ae0995` |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_067 -- --nocapture` | 0 | 3 | `repaired-9.log.gz` | `b359a3318f30b34606a1900eba47e087910cac5ca63a3170aeb98b540e3ba9c8` |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_performance -- --nocapture` | 0 | 3 | `repaired-10.log.gz` | `50de671caf4c2d44a32810cf181848a1524c8c11d3dd9ecee2c1007a35a75f81` |
| `cargo tree --locked --edges normal -p astersql-pd-cluster-utils -p astersql-pd-operatorutil -p astersql-pd-progress -p astersql-pd-schedule -p astersql-pd-schedule-affinity -p astersql-pd-schedule-checker -p astersql-pd-schedule-config -p astersql-pd-schedule-core -p astersql-pd-schedule-diagnostic -p astersql-pd-schedule-filter -p astersql-pd-schedule-handler -p astersql-pd-schedule-hbstream -p astersql-pd-schedule-keyrange -p astersql-pd-schedule-labeler -p astersql-pd-schedule-operator -p astersql-pd-schedule-placement -p astersql-pd-schedule-plan -p astersql-pd-schedule-rangelist -p astersql-pd-schedule-scatter -p astersql-pd-schedule-schedulers -p astersql-pd-schedule-splitter -p astersql-pd-schedule-types -p astersql-pd-storage-domain` | 0 | 0 | `normal-tree-0.log.gz` | `22b042852f2742d03d0b5839e24f4cfcb1ea3157e67e186ed87f75b12f539c96` |

全部上表日志文件本次重新读取、SHA-256及测试摘要核对通过。零测试命令只作为静态证据；既有`tso_processes`忽略项不是通过项。

## 准入限制与移交

- 主题6领域内核已有历史真实行为/故障与性能证据，可供后续复用；当前runtime未提供心跳/同步可选服务，不能宣称生产装配完成。
- 主题8/9领域边界和主题10原生后端/Go embedded-etcd拓扑等价性未准入；在对应任务保持阻塞，禁止固定响应、复制领域实现或以外部etcd伪装拓扑等价。
- 只读Go验证入口由4完成；本次不运行兼容脚本、不新增Go oracle，不把源码扫描写成运行差分。
- worker中的replication、metering/keyspace、Go GC tuner已映射49/78/95/102作领域移交或不适用边界验收；未发现需要新增服务主题的未映射需求。没有改写主题8/9/10或任何来源计划。

## 复核口径

所有来源链接从本文所在目录回到仓库根；完整manifest逐行核对存在、唯一、owner有效、hash相符。102任务引用目录按实际文件展开；拟建路径只标缺失，不当作存在来源。Go/模块含ignored/untracked全部文件在开始和结束各计算原字节哈希，保留原始集合摘要以核验只读约束。
最终链接/owner/manifest/只读状态及diff检查结果见下面补记。

## 最终核验补记

- 执行 `python3 /tmp/pd-task001-verify.py`：退出0。检查输入为本次清单本身及当前磁盘文件，不执行任何构建。
- 完整manifest 1909行：路径存在、去重、owner编号范围、原字节哈希全部通过；Markdown文件链接2091个全部存在；任务映射102/102。
- keypath常量69行另列原文，涵盖绝对/相对路径；worker/failpoint/持久化候选分别49/94/304行。
- `rg -n 'failpoint\.|tokio::spawn|go ' server pkg cmd/pd-server`：退出0，1213行，原始stdout SHA-256 `7131224f87ce7ac4fb24f27731d53272f024f263dd9219ec1ddcfef5e924400f`。此全扫描含测试/注释/非服务领域，直接生产子集另列，不把文本匹配数当行为通过数。
- Go/模块开始与结束集合共1983个（含ignored/untracked）：路径集合与每文件SHA-256完全一致；排序JSON集合摘要`300a4b0fdad52dd6b67f5daa919a1720ca47c97ecc7a81d364f6896f12d094a2`。初始和最终Go/模块Git状态均无差异。
- `git diff --check`：退出0；新增文档另以 `git diff --cached --check` 纳入提交前复核。只读plan.md及来源主题7/8/9/10在manifest中逐文件核验未变。
- 主题6最终使用`repaired.json`的11份日志及`normal-tree.json`的1份日志（共12份）核验；旧`verified.json`为修复前1924项测试，不能当作最终1925项报告的日志。最终日志支持schedule_acceptance 4通过、compat 33通过/1忽略、workspace 1925通过/1忽略；本次没有重跑这些测试。
- 新运行行为测试0（任务为文档盘点，TDD不适用）；Cargo槽位不适用。领域阻塞只记录在清单，盘点验收不解除领域准入。
