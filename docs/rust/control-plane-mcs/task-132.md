# 任务132：宿主任期内复用GC manager

PD Application为每个不可变领导任期持有一份canonical GCStateManager。PdNode::gc_manager公开当前任期的同一Arc，follower不发布manager。
GCResources接受实际Leadership、宿主CancellationToken与共享MetaStore，复用已有TermStore和KeyspaceManager；不创建私有runtime、全局信号或第二套GC算法。
GC generation使用宿主token的child，缓存命中前后检查取消；迟到提交不再发布已取消generation的point pair。
retire先取消任期并join GC tracked work，再清理cluster及自身lease。旧Arc即使被调用方保留，也无法读取退役缓存或提交写入。

GC存储事务使用ordered RawTxn，原TermStore对此一律拒绝。本项在既有Leadership中加入无else分支的transact_raw：
追加该任期精确value/lease/mod revision比较，禁止修改自身leader key；比较不满足时重新验证领导身份。
TermStore保留取消及expired检查，拒绝任何else分支；真实GC compare冲突和丢失领导身份保持区分。
Go对照来源：gc_state_lifecycle.go:71起的generation/退役边界，server/server.go:573起的manager装配；
43–47已有同源真实Go域行为及RPC对照，不把缺失的warmup48–51算成本项实现。

验证原始回执见[task-132-evidence.json.gz](task-132-evidence.json.gz)：

- red：真实etcd/两个宿主启动后，serving node没有可取得的canonical manager，行为断言失败；编译import修正另行保存，不计行为失败。
- 2项专项通过：同一任期Arc身份一致；follower无manager；旧任期退出后缓存/写入失败；真实换主读取旧持久值并继续推进，旧Arc不能覆盖新值。
- 在旧token仍有效时，直接用真实etcd替换leader key，重新campaign相同opaque value但新lease/revision；旧prepared RawTxn不能提交，GC值不变，清理旧lease不影响新leader。
- GC包41项、election包全部测试通过，workspace check、metadata、fmt通过。精确命令、退出码及计数保存在回执。
- runtime全包初次90通过/11失败/38 ignored，11项均因遗漏PD_COMPAT_GO_BIN，补齐已核验oracle后99通过/2失败/38 ignored；
  两项资源检查分别遇到并行即时Arc引用计数与端口重新bind竞争，随后分别定向重跑。保留完整失败及重跑日志，不将失败写成全包一次通过。
- 全部Cargo阶段动态取得槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，退出释放锁。
- Go/模块首尾SHA及存在性守卫保持1101项一致；完整workspace test由外层24统一执行。

旧版三项RPC适配与双进程端到端证据继续由外层任务3完成，不将manager公开视作RPC已交付。
