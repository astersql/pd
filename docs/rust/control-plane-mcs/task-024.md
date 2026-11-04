# 任务024：错误、主动终止及超时收尾

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

Controller保留最早错误，丢弃旧计划并清空阶段确认，以新step进入ExitForceLeader。
空计划要求store退出force leader；确认后转Failed，或严格超过两倍计划执行时限后
转Failed。重复abort幂等且不延长退出截止时间；终态不再下发，解除实际调度halt。
已送达的操作可能继续执行，abort保持Go的best-effort语义。

主动终止API已接通正常管理服务与角色转发；Idle/终态返回原400错误。
心跳及Show调用canonical超时检查。无心跳场景由Show推进，匹配Go，不新增私有定时器。
超时错误复用现有宿主ClockMap的时区、墙钟及单调时间投影；领域只接收时间及渲染函数。
阶段错误经过退出编排，RPC不再上抛生成器错误；六种补洞场景已比较完整实际下发帧。

## 验证

[所有阶段命令、退出、原始日志及来源SHA](task-024-evidence.json.gz)。编译后的red在
abort未进入真实退出阶段的断言失败。首次运行期测试构建修正了JSON数组访问；最终通过。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_0 -- --nocapture | 0 | 41通过：3本项+38相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_0 -- --nocapture | 0 | 13通过：2本项+11相邻 |
| cargo test --locked -p astersql-pd-runtime --lib cluster_host::schedule_operator_read::tests::task_062_clock_projection -- --exact --nocapture | 0 | 1相邻时钟投影 |
| cargo test --locked -p astersql-pd-runtime --lib role_worker_test:: -- --nocapture | 0 | 2相邻通过，另1既有ignored不计通过 |
| cargo test --locked -p astersql-pd-runtime --lib start_cleanup_test:: -- --nocapture | 0 | 2相邻资源收尾 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |

去重5聚焦+54相邻有效测试通过。首次时钟exact过滤遗漏cluster_host前缀，得到0 tests，
未计通过；随后正确完整路径实际执行1项。完整兼容包及workspace测试按用户授权统一运行。
动态槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程结束后释放锁、保留缓存。
Go及模块1101项存在性、SHA及Git状态保持一致。

已有只读Go二进制与实际Rust/etcd节点分别验证主动abort后确认、心跳触发整体超时后
确认、无心跳时Show触发整体及退出超时三条路径。比较HTTP原结果、完整心跳帧、
Show历史、终态停止下发及再次启动；新Collect step严格为原Collect+4，旧报告不被计入。
原始绝对截止时间、时区及单调时间字符串留档并验证，最早错误在本轮保持逐字一致。
跨运行仅投影墙钟/单调值、分别核验的cluster身份和step起点、Go无序报告ID列表。

补洞成功及重叠/无TiKV生成错误六种情形现逐字段比较实际完成响应和各store下发帧；
Region/peer ID、后续公开AllocId仍逐值相同，header revision及key mod_revision分别保留。
未将这些实际不同的revision归一化。任务022/023的历史阶段限制由本项补齐。
