# 任务044：成员管理 HTTP 与独立 etcd 专属成员

状态：已完成（用户授权的独立etcd专属部署边界）。

基准：`1383a3567`。批次43已完成；本任务的初次阻塞已按用户后续授权恢复：允许独立etcd，并实现显式一对一所有权及生命周期，不能从诊断绑定推定删除权限。Go及plan.md只读。

## 接线与契约

- [HTTP适配](../../../pkg/server/http_member.rs)替换固定单成员响应，读取真实注册表及当前leader，附加[MemberInfo](../../../pkg/member/member_info.rs)和[LeaderPriority](../../../pkg/member/leader_priority.rs)数据。POST按名称更新优先级，保持Go的未知字段/null空操作、名称不存在先于body解析、浮点截断和字符串优先级错误。名称/ID删除接宿主真实能力；没有专属能力时明确失败。
- 优先级事务同时检查成员存在、没有撤销记录及当前领导任期，避免查询后并发删除导致配置复活；不复制领域实现。
- [EtcdMemberAdmin](../../../pkg/runtime/member_admin.rs)使用已有etcd-client。仅显式配置的真实endpoint/member ID可建立专属绑定；启动核对backend cluster ID，正向PD→etcd与全backend反向etcd→PD在一个事务中保留不可变所有权。跨PD namespace重复认领也拒绝。
- [诊断绑定](../../../pkg/runtime/etcd_diagnostics.rs)与专属绑定共用revision CAS守卫。已被其他PD共享诊断绑定的成员不能升级为专属；已专属的成员不能再绑定其他PD诊断。普通共享部署不获得删除权限。此为宿主/操作员明确声明的部署所有权，不声称能探测绕过该接口的任意外部etcd客户端。
- 删除先核对双向所有权、真实MemberList及backend身份，再持久记录`removing`，执行真实MemberRemove，最后在领导任期事务内删除注册表/优先级并记录`removed`。保留所有权与撤销记录，禁止旧身份重新启动或重新竞选。跨etcd重配置和KV事务不伪称原子；不确定响应、取消或清理失败后可按ID重试，依据真实MemberList恢复。未发现真实成员且无删除意图时拒绝。
- [运行时](../../../pkg/runtime/node.rs)持有成员监控，撤销触发宿主取消，停止监听并join角色/worker；专属部署读取撤销状态失败也停止。监控无游离任务或私有runtime。`Member::bootstrap`及竞选事务包含撤销键检查。正常业务授权也检查本机撤销状态。

独立etcd由宿主/部署负责启动与停止，PD不启动Go runtime。新增显式参数：

```sh
pd-server --etcd-endpoints=http://surviving-etcd:2379 \
  --etcd-member-endpoint=http://own-etcd:2379 \
  --etcd-member-id=DECIMAL_ETCD_MEMBER_ID --etcd-member-exclusive=true \
  --client-urls=http://pd:2379 --peer-urls=http://pd:2380 --name=pd-1
```

`--etcd-member-exclusive`默认false；不能单独提供而不指定成员endpoint/ID。库宿主使用`PdNodeBuilder::with_etcd_member_admin`。PD ID仍与etcd ID独立，API ID指真实PD注册身份，删除通过持久映射定位etcd成员，不拿PD ID冒充etcd ID。查询没有映射的etcd leader时不伪造该字段。

过渡边界：拒绝删除最后一个etcd成员，以保留存储与恢复能力；共享部署拒绝删除。目标被撤销后即停止服务，因此删除当前处理节点时客户端可能断连/收到既有转发层的500（绕过转发层时为503），需从存活PD按ID重试以完成清理。`removing`不是成功，只有MemberRemove与清理均完成才返回200。普通删除的名称/ID、状态及body与Go运行差分对照；上述部署边界单独说明，不宣称Go内嵌进程/拓扑等价。

## 来源映射

- `wire-inventory.md` HTTP-118/119/120/121：members GET、按名称DELETE、按ID DELETE、按名称POST。
- `source-inventory.md`：`server/api/member.go` GetMembers/getMembers、SetMemberPropertyByName、DeleteMemberByName/DeleteMemberByID；删除priority及RemoveEtcdMember副作用行；`pkg/member/`身份、发布信息、优先级来源。
- `surface-inventory.md`四条members路由的审计注册仍归任务81，本任务不扩展审计。

## 行为与失败证据

[server独立测试](../../../pkg/server/http_member_test.rs)首次编译运行后失败：双PD共享真实etcd时，HTTP列表长度实际1、期望2；退出101，0通过/1行为失败。不是编译失败或人为破坏代码。

后续实现曾遇到临时字符串借用及monitor对store的借用编译错误，修复后重新测试；不将它们计作TDD失败证据。

[runtime独立测试](../../../pkg/runtime/member_admin_test.rs)覆盖真实独立etcd voter+learner、按名称/ID移除、反向绑定损坏拒绝、未知目标不改变拓扑、停止PD监听、清理优先级、旧身份重启拒绝及重试。独立etcd来自任务43已核验的官方3.5.15可执行文件，环境变量`PD_ETCD_BIN`显式选择；默认ignored不计通过，必须显式运行。

[事务故障测试](../../../pkg/runtime/member_admin_fault_test.rs)只在真实etcd事务边界加大请求或暂停提交，不伪造返回值/调用次数。优先级失败与取消保留旧值；删除的清理事务失败保留`removing`，真实拓扑已移除，重试收敛。

[只读Go supervisor](../../../scripts/http-member-oracle.py)使用已有Go PD binary依次启动真实3节点，读取并修改成员、执行两种删除，并运行Rust比较。Go join要求etcd recent-active健康窗口，早期未等待窗口导致第三节点join失败，已等待真实health及5秒窗口；已有Go binary删除后MemberList曾短暂返回旧缓存，副作用检查改为有界等待真实成员列表收敛，不将源码的force-refresh声明冒充该旧binary行为。Content-Type与普通成功/错误JSON对照；列表仅比较真实成员名称、优先级、PD leader，进程ID、URL及发布元数据是部署差异；etcd拓扑以独立etcd的真实MemberList另行验证。

最终验证结果见下方补记。

取消测试同时保留既有公共适配契约：v1 Redirector在宿主取消时返回500、text/plain及ErrServerNotStarted（含换行），成员handler直接取消为503 JSON。测试最初误要求所有响应都是JSON/503，现按实际公共层契约区分，未修改公共层返回值。


回归修正：首次server全包160通过/2失败/6忽略。task041的allow-follower members断言仍要求旧固定handler的503，已更新为真实读取的200；其转发、循环防护与取消断言保留。task019发现宿主关闭与follower晋升准备相遇时泄露typed ElectionError::Cancelled；角色循环仅在宿主已取消且错误确为该类型时正常收尾，其它错误继续传播。task040的上下文观察从被替换的healthy入口移至成员metadata领域读取，并注册真实成员记录，保留原有网络断开/任务上下文断言。


## 最终验证补记

各阶段从1–10原子领取的实际槽位均为1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有Cargo、只读oracle及子进程继承该绝对路径；验证结束释放自有锁，保留缓存。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-server --lib task_044_ -- --nocapture`（首次） | 101 | 0通过/1行为失败，HTTP错误地仅返回本机 |
| `cargo fmt --all` | 0 | 实现与修正阶段的格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 工作区元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | server静态检查 |
| `cargo check --workspace --locked` | 0 | 全工作区静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_044_ -- --nocapture`（最终） | 0 | 1通过/1默认忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_044_ -- --nocapture`（最终） | 0 | 2通过/1默认忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终） | 0 | 163通过/6默认忽略，含integration target |
| `cargo test --workspace --locked -- --test-threads=1`（最终） | 0 | 2050通过/0失败/42默认忽略，含compile-fail doctest |

默认忽略均未计入成功；只读Go差分及独立etcd删除通过下面的显式supervisor运行，普通8项HTTP观察和2项删除结果比较分别在server/runtime中执行。基础差分测试只归一化部署身份，不将缺失的共享后端etcd leader字段填成PD leader。

```sh
PD_HTTP_MEMBER_REPLAY="$PWD/target/rust-slot-1/http-member-oracle-1793319601887891000.json" \
PD_COMPAT_GO_BIN="$PWD/bin/pd-server" \
PD_ETCD_BIN=/tmp/pd-task043-etcd-v3.5.15/etcd \
python3 scripts/http-member-oracle.py
```


最终显式supervisor退出0：server的`task_044_readonly_go_differential`和runtime的`task_044_remove_real_dedicated_member`各1通过/0失败/0忽略。最终复用同一轮已保存的成功Go捕获（上面REPLAY路径），其SHA-256为`d9ec392a7b1de6cbed39d03c82deb5eee4c4b3ca47c20b91e667172fe9462306`；两个删除的Go响应均为200，最终Go成员列表仅剩member-1，所有普通HTTP观察齐备。该捕获最初的Rust删除测试失败于测试工具对ping Content-Type的错误假设，Go捕获本身成功；失败日志也保留。最终代码重新消费该真实捕获，不将旧Rust结果计为通过。

重新采集的Go运行还观察到非共识leader删除也可能返回`server stopped`，与客户端端点及响应确认时序相关；脚本在这种情况下非零退出，保留原始响应，不把500归一化为200。默认模式仍启动真实Go节点重新采集；REPLAY模式核对Go binary SHA、捕获的成功删除响应、最终实际成员集合及只读守卫后才运行当前Rust比较。它是任务允许的已保存运行oracle，不是源码推导或固定响应。

删除清理故障测试最初的注入条件漏接，显式运行发现实际200而非预期500。修正测试gate后，真实etcd拒绝2 MiB事务，HTTP500、成员已从真实拓扑删除且持久意图仍为removing；随后HTTP重试200，清理注册表与优先级。原失败和修正后通过均保留；生产实现未因该测试修正而改变。


测试gate修正后的最终`cargo test --workspace --locked`退出0：**2050通过、0失败、42默认忽略**；`cargo fmt --all -- --check`退出0。此前串行工作区回归也通过，两轮输出均归档。最后一次显式差分退出0，两个忽略测试已分别显式运行并通过，不计入默认42项忽略的成功数。

开始/结束1101个Go/模块文件（含ignored/untracked）的路径集合、原字节SHA-256和受保护Git状态完全一致；plan.md哈希不变。最终1219个Rust/Cargo源文件哈希在最后验证阶段保持一致。验证进程已结束、自有槽位1锁已释放，缓存保留。文档链接、Python语法和git diff --check通过。

[压缩原始证据](task-044-evidence.json.gz)包含所有确切命令/退出码/输出、失败过程、成功Go捕获及最终回放、binary指纹、源码哈希与只读守卫。SHA-256：`c262e5d693dda82745d1f911587cb9dc07054d261907df52c257b66c908ad459`。
