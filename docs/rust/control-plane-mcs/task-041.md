# 任务041：保留键空间初始化

状态：已完成，2026-11-05，基准82edcff2f。
[task-041-evidence.json.gz](task-041-evidence.json.gz)保存原始阶段、两内核Go原例/构建及在线引导/预分配/WAL差分。

Bootstrap复用36的真实labeler，始终不等待Region边界；再通过37的save_new原子保存名称和Enabled metadata。
Classic初始化DEFAULT/0，NextGen初始化SYSTEM/0xFFFFFE并设置keyspace_level GC；bound写入配置。
已存在时忽略Exists，保留原metadata、配置和时间戳，不提前把现存metadata灌入cold cache；每次真实Bootstrap仍重写label rule。
可选TSO manager提供basic配置并加入成员；29增加bootstrap_for_kernel，默认组不再把NextGen的成员错误固定为0。

预分配返回BootstrapWork，宿主驱动preallocate，库不启动不可回收的后台worker。
执行复用37真实分配器/创建路径，保留名字、重复名、非法名的逐项结果。
源码Go忽略重复名的Exists后使用nil KeyspaceMeta.GetId()==0；Rust保留该分组副作用并在NextGen原生测试明确断言，
不伪造已有metadata作为新创建结果。此corner case来自只读源码和Rust实际状态测试，不宣称在线Go NextGen预分配已逐例覆盖。
只读来源keyspace.go:190–259，tso_keyspace_group.go:203–248。

验证：

- 可运行red 1项真实etcd：保留空间不存在导致断言失败，exit101；实现后通过。
- 聚焦5项真实etcd通过，完整keyspace包97项通过、0失败/0忽略。
  覆盖两内核身份/配置、重复初始化保留metadata及时间、bound改变只刷新规则、cold cache行为，
  两内核真实带任期Allocator的host-owned预分配、重复/非法任务以及分组规则。
- 持有真实分组索引锁，把初始化停在规则已提交、metadata尚未保存的实际边界，停止etcd后放行：
  初始化失败、两个metadata索引均不存在、没有半成品cache；同WAL fresh SDK后重试，索引create revision相同。
  另在metadata已保存、成员加入前，实际把组改为split状态，初始化返回InSplit但两个索引均完整；修复组后重试保持原时间。
- Go Classic与NextGen既有GC suite初始化使用真实etcd、group manager及keyspace manager；
  各实际运行TestGetAllKeyspacesGCStatesExcludingGCBarriers一叶例通过，验证各自bootstrap空间及GC管理类型。
  NextGen测试二进制以同源代码、只读依赖缓存和Go1.26.8隔离构建，无failpoint注入。
- 在线Classic Go使用真实配置文件raw模式与两个pre-alloc名字，通过真实Bootstrap RPC初始化，等待两个预分配空间Enabled。
  按Go观察到的实际ID分配顺序驱动Rust同一有效线性化，使用Go实际创建时间作为共享输入；
  DEFAULT与两个预分配空间的完整DTO、metadata/name原始protobuf bytes一致，label JSON语义一致。
  保存原始物理键、bytes、create/mod revision，不把并发分配顺序伪称固定输入次序。
- 首次把普通服务重启误当成重复KeyspaceManager.Bootstrap，规则revision断言失败，保留该记录。
  只读server.go:838–871确认普通重启不调用此方法；修正夹具为只移除其自有集群引导标记，保留所有键空间/WAL，
  重启后从真实Bootstrap RPC再次进入。双方metadata/name revision均不变，保留规则mod revision均推进，
  重复预分配不重新分配ID。记录实际重引导header和夹具marker路径；未触碰外部集群。
- 共享Go fixture仅增加可选args入口，原API模式包装保持；原32在线默认组合并相邻回归1项通过。
- fmt、workspace check通过；sa_002动态metadata守卫1项/5负向控制通过，78包699生产源码。
  完整workspace test按外层任务24统一回归，零doc-tests不计行为证据。
- 各Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  fixture/子进程结束后释放自有锁，保留缓存。

Go服务、SDK、官方etcd来源沿用001，锁定模块安装/只读缓存机制沿用39。
1101项仓库Go/模块首尾SHA及存在性一致；未改Go、模块、failpoint、plan.md或来源史诗。继续42批量回收。
