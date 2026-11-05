# 任务036：键空间Region边界等待

状态：已完成，2026-11-05，基准0cfe54e4f。
[task-036-evidence.json.gz](task-036-evidence.json.gz)保存原始命令、退出码、Go逐例回执、
真实HTTP/心跳/持久状态和动态metadata。

KeyspaceRegionSplit复用canonical RegionLabeler，先持久化并发布keyspaces/ID规则；
等待阶段释放labeler锁，通过共享RegionsInfo查询两个真实start key。
包含该key的较大Region不能代替边界，raw/txn使用25的唯一边界算法。
已有边界也在第一次配置tick检查；失败tick后读取最新interval，timeout在入口固定。
宿主传入labeler、共享region索引、动态配置和取消token，库没有线程、私有runtime或全局信号。
Go splitKeyspaceRegion/waitKeyspaceRegionSplit/checkBound为只读来源（keyspace.go:653–762）。

wait=false成功保留规则；等待超时或取消尝试删除规则；set失败不进行后置清理。
KeyspaceSplitFailure独立保留主错误和可选cleanup错误，清理失败不覆盖原始取消/超时。
Go的等待manager与labeler存储生命周期不同，因此等待token不会自动取消独立labeler写入。
全宿主停服时两者都关闭，清理可能失败并留下持久规则；本项明确保存这一可观察结果。
完整元数据创建、索引、分组及启用流程归37，本项不把Go创建HTTP入口当成Rust流程已完成。

本轮验证：

- 可运行red 1项真实etcd，缺少创建规则导致断言失败（exit101）；实现后通过。
- 聚焦6项全部通过，含4项真实etcd、2项真实Core索引/受控时钟测试。
  覆盖现有边界首tick、动态interval/固定timeout、包含范围拒绝、raw/txn/最大ID，
  timeout/cancel清理、真实停服set失败、真实停服cleanup失败及同WAL恢复。
- 完整keyspace包65项通过、0失败、0忽略；fmt、包check、workspace check通过。
- 只读同源Go测试二进制实际执行TestMakeRegionBound、TestMaxKeyspaceLabelRuleSplitKeys、
  TestMakeLabelRule三例通过。未裸跑在SetupTest启用skipSplitRegion的Go套件。
- admitted Go服务正常模式，真实CreateKeyspaceByID HTTP入口执行五组差分：
  51超时，52等待期间到达边界，53预先已有边界，54不等待，55全宿主停服取消。
  52/53输入真实gRPC RegionHeartbeat，随后以GetRegion逐一确认实际start key；
  同一请求经已有生产core_region_adapter进入Rust共享索引，无固定RPC响应或mock。
- Go超时为HTTP500和精确[PD:keyspace:ErrRegionSplitTimeout]region split timeout，
  双方真实etcd均无该规则；成功三例HTTP200、规则语义全部相等且持久保留。
  原始JSON字节保留在证据中，仅对象字段顺序按JSON语义比较；未抹去revision。
- Go实际SIGTERM取消返回HTTP500、精确[keyspace] wait region split canceled；
  原日志逐行证明context canceled使删除规则失败。Rust实际停止etcd再取消等待，
  保持同一主错误及独立cleanup错误。双方同WAL重启后规则55仍存在且内容相等。
  原始HTTP输出、错误、日志、物理键值及create/mod revision均记录，未伪造关闭时响应。
- 共享启动fixture只增加可选mode，旧API-mode包装保留；原32在线差分相邻回归1项通过。
- sa_002动态metadata守卫1项通过、5负向控制、78包693生产源码。
  完整workspace test按外层任务24统一执行，零doc-tests未计为行为证据。
- 各阶段动态领取槽位1，绝对CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有fixture/子进程结束后释放自有锁，保留缓存。

Go基准沿用任务001准入：kvproto f8ad079c8e9812ed5900b0c12b5f1d97171464c4、
server SHA256 364477a06e13afb29626ba57dbbd0be8245d0e9d68ad30f0dbeeda3dce3a3147；
同源keyspace测试二进制8554eaecb0374e859eff80dee6bb6c2302df592a8bc2d85078cd47ceb43427d5。
1101项Go/模块首尾SHA及存在性一致，无Go源码/模块修改，无failpoint注入。
plan.md及来源史诗保持只读；继续37，GC RPC验收未由本项替代。
