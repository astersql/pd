# 任务012：密钥字典持久化与快照

DictionaryStorage复用canonical MetaStore及Leadership，读取固定的精确键
`encryption_keys`，初始不存在返回空快照；已加载后删除返回既有PD错误。
读写共用异步操作锁，快照使用同一个RwLock保存mod_revision和不可变Arc字典。
只在完整解码、取得主密钥、认证解密及字典解析都成功后一次发布；旧/相等revision
在解析之前直接忽略。读取错误、损坏密文/协议、缺失主密钥元数据不改变快照。

保存使用当前配置取得主密钥，明文模式将所有历史key标记was_exposed，文件模式
保留历史标记；编码后通过实际Leadership::transact写入。捕获的租约、value、revision
共同防止旧任期写入；过期返回leader expired，网络错误映射事务错误。保存本身不发布，
由loader重新读取后发布，符合Go saveKeys/loadKeys的职责划分。拒绝混用不同MetaStore
的领导权，避免读取和写入落到两个后端。超时写入保持结果未知语义，不声称后端未写入。

## 协议边界

领域KeyDictionary/DataKey/KeyEnvelope不依赖PD protobuf。真正的protobuf转换位于
server/encryption_dictionary.rs的ProtoKeyDictionaryCodec，通过KeyDictionaryCodec注入。
没有新crate、重复协议模型生成或第二套存储契约。单测直接注册同一生产适配器源文件，
用测试限定的self crate别名绑定当前领域类型，避免复制转换代码或引入server开发依赖环；
普通server target也通过实际依赖编译。所有tests仍为单独*_test.rs并显式注册。

KMS vendor改为拥有的String，使协议内未知vendor能原样进入现有错误路径；已知配置
仍为AWS。保留未知EncryptionMethod数值，当前/历史key、创建时间和暴露标记全部往返。
Go当前实现未消费的新KMS嵌套字段及EncryptedContent.metadata同样不作为新能力宣称；
Go saveKeys也仅生成既有字段。编码map顺序不要求与Go随机map遍历相同。

## 实际行为与oracle

- 原生etcd真实租约/事务保存明文字典，再重新读取：current key、全部历史key及
  was_exposed一致；保存成功时缓存仍未发布。相似前缀的备份键不会被当作主字典。
- 真实Go服务生成文件主密钥字典，Rust领域loader读取并解密；用canonical Rust协议
  适配器增加当前/历史key后写入真实etcd，Go在同一事务revision重新加载，Rust再次
  加载得到相同字典，之前取得的快照保持不变。互通写入由测试直接事务执行，不冒充
  Go领导权切换；Rust保存的任期保护由独立真实租约场景验证。
- 相同revision的坏数据被忽略；坏外层protobuf、缺失元数据、坏内层protobuf、
  文件密钥认证失败、已加载后删除均保留旧Arc和revision；修复有效值后发布新revision。
- 暂停真实etcd进程，在实际网络边界让保存和读取失败，确认缓存不变；恢复后按实际
  后端状态重新加载，不假定超时写入未发生。撤销真实租约后旧term保存被拒绝，持久
  mod_revision不变。没有mock MetaStore、调用次数断言或Go failpoint注入。

Go私有快照不通过测试钩子读取；Go侧直接观察的是实际字典、认证解密结果与watcher
在确切revision重新加载。Rust快照保留由真实后端故障测试证明，不混称为Go内部观察。

## 验证

[完整证据](task-012-evidence.json.gz)保留命令、退出码、原始日志和来源SHA。
初始路径/新协议字段登记编译错误不算行为失败；012-real-red编译后2例在“必须真实保存”
和“必须加载Go字典”断言失败。实现后2通过，补充故障后5通过，最终6聚焦通过。
final工作区检查发现server适配器缺少直接errs依赖，登记后重新检查正常。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_012_ -- --nocapture | 0 | 6聚焦通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-encryption --lib mcs_011_ -- --nocapture | 0 | 4相邻通过 |
| cargo test --locked -p astersql-pd-encryption --lib config_test:: -- --nocapture | 0 | 2相邻通过 |
| cargo test --locked -p astersql-pd-election --lib leadership_test:: -- --nocapture | 0 | 2相邻通过 |
| cargo check --workspace --locked | 0 | 工作区编译通过 |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo tree --locked -p astersql-pd-encryption --edges normal --prefix none | 0 | 无PD proto/etcd-client/server普通依赖 |

动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
结束释放自己的锁，保留缓存。Go/模块1101项存在性、SHA及Git状态保持一致。
依用户授权，包级和workspace完整测试留待全部实现后统一执行；保留待回归，尚未宣称
轮换、watcher、Region加密运行时或任务102完成。
