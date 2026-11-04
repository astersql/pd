# 任务004：复制模式持久状态

开始前固定Go oracle：pkg/replication/replication_mode.go的NewReplicationModeManager、
loadDRAutoSync、UpdateConfig、drSwitchToSync、drSwitchToSyncRecoverWithLock；
调用方server/server.go的SetReplicationModeConfig先验证模式，再持久化配置，
失败恢复旧配置。Go源码和模块只读。

行为边界：majority不读取旧DR状态；dr-auto-sync缺失/空记录初始化sync并分配ID，
已有记录（包括JSON null）直接恢复；majority转dr-auto-sync总是新建sync_recover，
持久化成功后才发布状态，失败恢复旧配置；同为DR模式更换label不触发状态切换，
按实际Go代码而非其注释推断。非法模式由公共配置入口拒绝，不能发布新状态。

复用现有Rust配置模型、全局ID分配器及JSON存储原语；领域层不得依赖proto/server/runtime。
先在真实Go进程观察恢复/错误，再通过新canonical crate的行为断言获得失败证据。

## 实现与实际证据

新增canonical astersql-pd-replication及独立mode_state_test模块；复制策略配置从
server/config/replication_config.rs移入本crate，由原位置重导出，保留原API和
序列化行为。既有replication_endpoint实现移入storage-domain注册，wire facade通过
原有Deref复用；没有复制存储或ID算法。普通依赖树不包含PD proto、server、runtime、
tonic、hyper或etcd-client；网络/协议依赖仅用于测试。库不创建runtime、信号或全局订阅。

首次测试实际运行真实Go过程，取得HTTP200的async_wait/state_id=42、RPC state_id=42，
及非法模式HTTP500、旧配置不变；Rust最小可编译接口在0!=42恢复断言失败，退出101。
随后实现真实读取/初始化与配置切换。现有null记录保留零状态，不分配ID；空/缺记录
初始化sync；majority不读取坏DR记录；只有majority到精确dr-auto-sync才进入恢复。
所有状态写成功后才发布；非法模式、预取消和写错误均不改变发布的旧状态。
原配置规范化函数仅验证合法性，保留Go接受但未改写的原始模式字符串。

JSON模型保留Go的null字段、字段名大小写及最后重复字段获胜、unknown field忽略；
拒绝数组、负uint64、float32溢出和不可序列化的NaN。时间保留RFC3339时区。
null及大小写重复字段均实际回放Go二进制，未将源码推导当作运行差分。

故障用例的Rust侧在全局ID实际分配后、状态写入前，通过宿主时钟回调停止真实etcd，
状态写因后端不可用失败，配置/缓存保持旧值。重启同一后端后确认状态键未落盘，
再操作成功，ID为2，证明失败的ID1未被复用。
Go侧以真实TCP代理转发其embedded-etcd客户端，在目标状态键的完整gRPC消息进入
后端前取消该stream；不替换响应、不改Go、不停止PD进程或lease流。
实际观察到尝试持久化sync_recover/ID1，公共HTTP500保留完整原始etcd错误，
旧配置/majority状态保持，真实后端没有DR状态键。解除故障后HTTP200、恢复状态ID2、
完整配置及持久模型匹配Rust。仅recover_start按显式受控时钟归一化，Go时间先验证
位于真实请求时间窗；没有归一化state ID、模式、错误或其他状态字段。
代理仅用于小于60KiB、明文HTTP/2的本测试请求，不是生产transport或通用故障服务。

这些确定未提交的注入点才能断言键未变化。一般Unavailable可能已提交：manager不
盲目重试或回退分配器；此时仍保留旧发布状态，不声称所有故障都保证存储未提交。
Go网络取消与Rust后端Unavailable的原始诊断不同，原文均保留，未伪称错误字符串等价。

## 验证

完整命令、退出码、有效数量、源码SHA、原始失败/成功及Go观测保存在
[task-004-evidence.json.gz](task-004-evidence.json.gz)。

| 命令 | 退出 | 实际结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 格式化 |
| cargo check --offline -p astersql-pd-replication（首次注册） | 0 | 新增包及必要chrono serde锁依赖；不更新版本 |
| cargo test --locked -p astersql-pd-replication --lib mcs_004_ -- --nocapture（首次） | 101 | 0通过、1行为失败；Go观察先通过 |
| 同一聚焦命令（最终） | 0 | 4通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-server-config --lib replication_config_ -- --nocapture | 0 | 6通过；原有配置API/JSON/defaults回归 |
| cargo test --locked -p astersql-pd-storage-endpoint --lib replication_ -- --nocapture | 0 | 3通过；真实etcd、Go字节及MemoryStore回归 |
| cargo check --locked -p astersql-pd-replication | 0 | 编译检查 |
| cargo metadata --format-version 1 --no-deps | 0 | 76个package |
| cargo check --workspace --locked | 0 | 工作区编译检查 |
| cargo tree --locked -p astersql-pd-replication --edges normal --prefix none | 0 | 普通依赖边界审查通过 |
| cargo fmt --all -- --check | 0 | 格式检查 |

Cargo.lock仅增加新包、server-config到canonical包的引用和chrono已有版本的serde依赖，
移除Cargo自动生成的无关排序差异，没有升级依赖。各阶段动态槽位1，
CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
所有测试及fixture结束后释放自有锁，保留缓存。Go/模块1101项存在性、SHA和Git状态
首尾一致；未生成或改写Go源码/模块。所有文档链接与git diff --check通过。

依据用户明确选择，完整workspace/兼容包回归统一留到全部实现后；本项保留为待回归。
本项只完成持久模式边界，不宣称4–8整个replication状态机、133公共接线或102完成。


### 实际差异补强与最终复核

真实Go进程额外证明字段名ſTATE/ſtate_ID能够恢复state_id=42；ASCII-only解码
错误得到0，聚焦测试退出101。按Go encoding/json的SimpleFold ASCII等价类补齐
ſ/K后，全部4项聚焦通过。该失败和修复后的输出均保留。
真实Go配置端点还接受并原样保留MAJORİTY；旧Rust使用Unicode full lowercase，
错误地拒绝该模式。原有配置相邻测试也将其错误列为非法，已依据实际Go HTTP200
及原值读取证据修正为majority规范化、原配置字符串保留。随后4项聚焦和6项配置
相邻回归均通过，workspace check和fmt检查退出0。这里只匹配受支持模式名的Go
simple rune lowercase结果，不把full Unicode扩展当作Go行为。

最终Go/模块1101项及所有相对文档链接再次核对通过；验证节奏遵循用户明确授权。

恢复夹具补强：使用在无匹配存活store时保持不变的sync/ID42记录，隔离Go后台tick对
恢复边界的影响；同时比较RPC真实available_stores，保留所有旧async_wait观察。
最终4项聚焦再次通过（38.67秒），无新忽略。
