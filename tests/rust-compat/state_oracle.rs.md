# 只读 Go 服务状态探针

独立 Rust example/host runtime，通过现有 Go PD 二进制的真实 HTTP/gRPC 接口提交
固定 protobuf 事件并采集运行结果。由 task53/oracle.py 管理隔离端口、数据目录、
进程 TERM/wait 及同 WAL 重启；Cargo/二进制均使用父脚本领取的共享槽位。

主 fixture 保存16步联合输出和实际源 history 的增量/重连帧。重启 fixture 保存
95条追加事件，实际100→重启100→下一事件101的完整同步与KV metadata。
PdClient 解码后的 Go 响应去 transport header 后由 prost 规范化编码；这些是 Go
运行字段的快照，不能称为原始 gogo 序列化字节。原始字节验收仍由任务50完成。

只有明确的主机时间原点和无序集合被归一化，所有 counters/period/version/index
保留。Go heartbeat 输入有限流结束时可能在响应头前或 body 返回 Unknown/EOF，
必须保持流至该终止结果，不能拿到响应头就 drop 而截断事件序列。
不构建/写入 Go 源或模块，不注入 failpoint。来源与验证见
[任务53](../../docs/rust/state/task-53.md)。
