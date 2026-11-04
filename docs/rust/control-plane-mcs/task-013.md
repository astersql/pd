# 任务013：任期内数据密钥轮换

KeyRotation接受canonical DictionaryStorage和宿主Leadership；无term或本地租约过期时
不轮换。重载、保存及持久读回持有同一个字典操作锁，与watcher加载串行化；轮换只修改
已发布字典的深拷贝，捕获任期的实际事务成功且读回成功后才发布候选key。
后端已替换但本地TTL尚未失效的旧term仍会被canonical value/lease/revision事务拒绝。

保留Go谓词：current ID为0、method改变、was_exposed或严格过期时轮换；精确到期
边界不满足Before。creation_time先按Go转换成int64再比较纳秒；生成时间按Unix秒保存。
历史key永不被碰撞覆盖，最多尝试10次；耗尽返回既有maximum attempts reached错误。
Go此处抑制NewDataKey生成错误的分支保留，不能把未生成key当作成功写入。

set_leadership强制使用当前主密钥重新包装；仅主密钥变化时保留当前ID、密钥及全部
历史字段。明文主密钥会暴露标记全部历史key；换回文件主密钥时暴露的current触发新key，
新key不暴露而历史暴露标记保留。停用数据加密只将current ID置0并保留历史；已经停用
时，即使force也不额外保存，符合Go分支。

## 实际验证

- 初次有效leader真实保存密钥，follower及本地已过期term不写入。
- 控制宿主纳秒时钟，测试严格到期边界、method变化、暴露标记和停用后的历史保留。
- 后端直接撤销旧租约并真正选出替代term，旧term本地TTL仍有效；它的候选写入被拒绝，
  可读Arc及持久revision保持不变。新term成功后旧term再次尝试也不能覆盖新快照。
- 默认熵仍为OsRng。新增宿主CryptoRng入口供真实加密随机源使用；测试采用rand官方
  StdRng，先把同一真实密钥流的前3/10个ID写成历史key，再执行实际轮换。分别核对
  后续候选密钥、旧数据保留和10次耗尽后下一tick的第11个候选；没有mock返回值或
  mock调用次数。Go碰撞策略依据固定源码，不宣称运行了Go碰撞注入。

真实Go oracle复用固定二进制及同一个原生etcd WAL，依次重启执行AES256→AES128、
更换文件主密钥、停用数据加密。每阶段的原字典也作为独立Rust原生后端的实际输入：
方法切换保留原ID/字节，新增随机ID/密钥按角色、长度及metadata比较，创建时间使用
实际观察到的Go新key时间；重新包装和停用阶段解密后的整个字典精确相等。
保留Go各阶段及Rust事务的原始revision，不将两个不同后端的全局revision归一化相等。

Go私有缓存未通过测试钩子读取；不把Rust失败候选保留测试冒充Go内部缓存观察。
Rust明确使用候选副本，满足本任务“事务失败不得发布新key”的要求。没有Go源码/
模块改动、Go构建或failpoint注入，没有私有runtime/全局日志/信号。

## 检查

[原始证据](task-013-evidence.json.gz)保存确切命令、退出、日志及来源SHA。
013-red编译后在“有效leader必须提交初始key”断言失败；green的MutexGuard解引用
编译错误不计作行为失败；green2四项、green3五项通过，最终：

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_013_ -- --nocapture | 0 | 5聚焦通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-encryption --lib mcs_012_ -- --nocapture | 0 | 6相邻通过，含真实后端超时/恢复 |
| cargo test --locked -p astersql-pd-encryption --lib mcs_009_ -- --nocapture | 0 | 4密码操作相邻通过 |
| cargo test --locked -p astersql-pd-encryption --lib config_test:: -- --nocapture | 0 | 2配置相邻通过 |
| cargo test --locked -p astersql-pd-election --lib leadership_test:: -- --nocapture | 0 | 2任期相邻通过 |
| cargo check --workspace --locked | 0 | 工作区编译通过 |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo tree --locked -p astersql-pd-encryption --edges normal --prefix none | 0 | 无PD proto/etcd-client/server普通依赖 |

动态槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；退出后
释放自己的锁并保留缓存。Go/模块1101项存在性、SHA及Git状态一致。
按用户授权，完整包级及workspace测试待全部实现后统一执行，任务保留待回归；
不宣称字典watcher、Region加密宿主或任务102已完成。
