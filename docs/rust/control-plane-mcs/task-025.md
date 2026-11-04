# 任务025：键空间标识、边界及label rule

状态：已完成，待统一兼容包及workspace回归。任务102尚未完成。

建立canonical astersql-pd-keyspace，复用codec边界编码、kerneltype、已有键空间常量
和labeler规则类型。ID及名称校验与编码分开：Classic保护DEFAULT/0，NextGeneration
保护SYSTEM/最大ID-1；名称实际限制为20个ASCII字母数字、下划线或短横线，保留Go错误文本。
未按源码注释中的64字符放宽限制，也未让编码函数额外执行仅属于准入的保留ID校验。

最大24位ID的右边界推进mode字节；raw/txn标签范围使用相同canonical codec。
标签ID后缀复用既有parser，完整身份校验保留Go的较宽契约：首个id标签生效，标签值
可有前导零，index/rule_type/TTL不参与此身份判断，与labeler快速索引的严格形状不同。
完整memcomparable解码检查padding；空key返回最大txn空间，解码后的剩余输入按Go忽略。

## 验证

[命令、退出、原始日志、来源SHA及只读保护](task-025-evidence.json.gz)。新crate接通后
编译的red在最大ID真实边界缺失时失败。首次live raw配置误放顶层，Go实际生成txn；
保留该失败，修正为pd-server.key-type后两种实际模式通过。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-keyspace | 0 | 4聚焦通过；0 doctests不计行为 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_025_ -- --nocapture | 0 | 1聚焦，两种真实Go模式 |
| cargo test --locked -p astersql-pd-codec | 0 | 7相邻通过 |
| cargo test --locked -p astersql-pd-schedule-labeler | 0 | 38相邻通过，含实际etcd失败/恢复及事务 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 78 packages |
| cargo tree --locked -p astersql-pd-keyspace -e normal | 0 | 领域无protobuf、具体存储或私有runtime |

去重5聚焦+45相邻通过，0失败、0忽略；全兼容包及workspace测试依用户授权统一运行。
动态槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程退出后释放锁，保留缓存。
Go及模块1101项存在性、SHA和Git状态一致。

已有只读Classic Go节点执行26个名称请求，成功及完整错误文本与Rust领域校验一致。
八个真实CreateKeyspaceByID操作覆盖raw/txn、字节进位及最大ID，实际生成的标签JSON
逐字段相同。Rust规则经现有生产RegionLabeler API写入后，其返回及真实MetaStore JSON
均与Go一致；读取header revision及key mod_revision分别留档，不把不同revision归一化。

NextGeneration保护规则、私有标签身份与非法解码分支使用Go源码/断言对应的Rust单元
测试，不冒称运行了NextGeneration Go或failpoint测试。此项不实现Rust创建键空间API；
该完整生命周期入口仍由任务037等负责，不以labeler接线代替创建元数据实现。
