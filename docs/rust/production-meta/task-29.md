# 生产任务 29 原生故障历史

使用三个 release 原生进程和真实 SQLite 文件，种子 731091。四个并发客户端
在每轮写入两个共享键和一枚唯一标记，读取两键的同一线性快照。故障计划用
双向分区与实际 SIGKILL 交替，分区轮次检查隔离节点拒绝线性读和写。

独立检查器用最终线性快照中的标记解析未知写，按 revision 重建事务顺序，
核对每次读的两键值，并核验调用与返回之间的实时时序。未知写从不直接重试。
固定 seed 的 32 轮全部通过，两个故障类别各有 64 个调用与注入点重叠。
261 个客户端确认、507 个未知写结果和 410 个成功读均有原始记录。
未知写中 18 笔实际提交。所有夹具进程均已回收。

`python3 -B docs/rust/production-meta/check_native_history.py FIXTURE NEW_RUN_DIR --cycles 32`
退出码 0。三节点使用的 release 夹具摘要与本地性能和长稳夹具一致。
[驱动和参考检查器](check_native_history.py)、[汇总](native-history.json)、
[压缩后的完整历史](native-histories.jsonl.gz)。
