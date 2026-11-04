# 任务008：复制状态运行周期

来源：replication_mode.go的Run、tickUpdateState、tickReplicateStatus和metrics.go；
server/grpc_service.go的StoreHeartbeat、server/server.go的ReplicateFileToMember/
PersistFile、server/api/replication_mode.go和admin.go的对应边界。

## 当前接线

同一leader term内，ClusterHost在bootstrap成功后或恢复已bootstrap的元数据时
创建唯一ModeManager。宿主共享原有StoreInfo、RuleManager、RegionsInfo、Allocator
与TermStore；保存DR状态继续经过已有租约事务栅栏。反复bootstrap不另启worker。
运行循环使用宿主Tokio handle：等待60秒后，500ms状态检查与5秒文件复制分别
驱动同一manager；两个循环属于一个可取消、可join的worker，不创建私有runtime。
退出时先取消并join，再释放资源；DR状态gauge有退出/abort时的归零保护。

StoreHeartbeat先执行原有store状态处理，再将DR报告送到同一manager；
PutStore与StoreHeartbeat从该manager构造真实protobuf状态（含60秒hint、state ID、
available_stores和pause_region_split）。未配置DR能力的独立适配器仍在修改统计前拒绝。
HTTP /replication_mode/status读取同一snapshot，/cluster/status复用Go compact文本
字段编码。bootstrap前的状态端点返回真实Go对应的500/ErrNotBootstrapped。

文件复制读取真实成员注册表，通过已有可取消HTTP/TLS传输，给每个成员发送
/pd/api/v1/admin/persist-file/DR_STATE；使用PD-Allow-follower-handle，失败成员重试，
只有成功才缓存其state ID。文件仅包含Go选定字段，不把async_start/进度复制进去。
HTTP边界仅接受语法合法JSON和固定文件名；保留原始字节，不以数值模型限制语法。
JSON字符串兼容复用现有go_json_strings，深度限制10000与Go一致。
实际文件IO由宿主执行，只使用listener显式data-dir；出站配置来自该宿主的传输设置，
不把动态配置持久记录当作TLS身份或文件目录。文件写入行为与Go os.WriteFile相同，
此项不构成生产MetaStore durability验收。

指标为Go的5个无label collector：pd_replication_dr_tick_total、dr_state、
dr_state_id、dr_recovered_region和dr_recover_progress（均带pd_replication_前缀）。
沿用listener Registry的事务注册和失败回滚，domain只持克隆句柄，退出释放注册。
状态检查计数与文件发布状态分别按原周期更新，恢复计数/估算在真实恢复扫描时更新。

## 真实差分与失败范围

固定Go输入复用任务005/006的7个store、placement规则与DR配置，显式暂停无关调度。
真实Go先完成sync/1、async_wait/2、报告确认后的async/3，DR_STATE实际内容为
state=async、state_id=3、available_stores=[100,200,300,400]，不含async_start。
原生etcd上的Rust节点通过公开bootstrap、PutStore、StoreHeartbeat和HTTP端点
完成相同状态链；文件由实际HTTP自复制产生，指标实际为state=3、state_id=3且tick>0。
只归一化async_start，不归一化ID、列表或revision；该时间字段的生成窗口由任务006另行差分验证，本项读取确认其为非空时间字符串。

首次夹具Config字段拼写问题只算编译失败。之后真实Go文件/指标先通过，Rust的
DR bootstrap仍返回错误，行为断言失败（退出101）。接线后首次发现调度器预取全局
ID，异步等待ID变成130；不把它归一化，而是在两侧显式halt-scheduling=true。
双节点场景验证leader向follower实际复制DR_STATE，原leader退出后follower接任并
恢复async/3，旧term退出后复制状态记录的值及mod_revision保持不变。

集群compact状态比较曾误用/status路径，两侧都读到null；这不计为文本兼容证明。
现改为/cluster/status，要求Go字段是非空DR_AUTO_SYNC字符串并逐字比较。
另一次Unit test crate与正常依赖的ModeSnapshot类型身份不同造成编译失败，
修正为通过canonical正常依赖的重导出构造文件适配输入，不计行为红灯。

文件边界真实Go/Rust逐项比较：非法JSON400、非法文件名500、普通JSON/null、
非配对surrogate、原始无效UTF-8字符串、1e10000及10000层嵌套200，10001层400。
成功时两侧文件字节完全等于输入。实际目标路径被替换为目录造成写入失败时，
复制不缓存成功；恢复文件路径后重试真实HTTP并写入正确state/ID。
bootstrap前状态差分也先取得Go500/Rust503的行为失败，再修正状态码与错误体。

## 验证记录

[task-008-evidence.json.gz](task-008-evidence.json.gz)保留各次编译/行为失败、
最终真实差分、输入源码SHA及所有原始命令/输出。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-replication --lib mcs_008_ -- --nocapture | 0 | 2通过、0忽略；真实Go/Rust、双节点、JSON/文件故障 |
| cargo test --locked -p astersql-pd-replication --lib mcs_007_ -- --nocapture | 0 | 4相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib wire_store_heartbeat_test::task_008_ -- --nocapture | 0 | 1相邻通过 |
| cargo test --locked -p astersql-pd-server --lib task_086_ -- --nocapture | 0 | 10相邻通过；Registry回滚/退出、TLS传输、取消及label清理 |
| cargo test --locked -p astersql-pd-server --lib store_heartbeat_adapter_test:: -- --nocapture | 0 | 7相邻通过 |
| cargo test --locked -p astersql-pd-server --lib task_046_behavior -- --nocapture | 0 | 1相邻通过；原有cluster/status边界 |
| cargo check --workspace --locked | 0 | 工作区编译 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo fmt --all -- --check | 0 | 格式检查 |
| cargo tree --locked -p astersql-pd-replication --edges normal --prefix none | 0 | 无server/runtime/PD proto/HTTP/etcd-client普通依赖 |

所有阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
Cargo.lock仅加入已存在的workspace依赖关系，没有版本升级；清除无关排序差异后
locked workspace check再次退出0。Go/模块1101项首尾存在性、SHA及Git状态一致，
本项未执行Go构建、Go生成器或Go写入。自有进程结束后释放锁，保留缓存。
按照用户明确选择，完整包/workspace测试统一留到全部实现后；本项保留待回归，
未把领域状态机接线视为整个控制面/MCS或任务102完成。
