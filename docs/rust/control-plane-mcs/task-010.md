# 任务010：明文与文件主密钥

实现 `MasterKey::plaintext/from_file/encrypt/decrypt/ciphertext_key`，复用任务9的
AES-256-GCM与OsRng；文件只接受去除Go语义空白后的32字节hex。
错误保留PD错误码、Go错误优先级与hex逐字节诊断，包含无效UTF-8输入；
明文模式原样返回数据并忽略IV。密钥及读取缓冲使用Zeroizing，Debug只输出模式
和密文密钥长度；不安装日志、信号或runtime。生产依赖保持无proto/存储/SDK。

复用任务3只读入口，将已有二进制版本/协议/SHA校验抽成公共函数；任务4仍调用
同一个校验实现。未重建Go，未新增任何Go工具或测试。

## 真实差分

独立Rust测试启动固定Go PD二进制与真实本机etcd，不执行Go源码测试：

- 带ASCII、NBSP、全角空白的文件密钥：解开Go真实保存的EncryptedContent，
  验证KeyDictionary、32字节数据密钥与was_exposed=false。
- 把相同字典用Rust主密钥重新包装，真实事务写回encryption_keys，观察Go watcher
  在该事务revision重新加载；再写入损坏密文，双方拒绝认证且Go未公布错误字典。
- 明文主密钥：真实Go字典无IV/ciphertext_key，Rust输出逐字节一致，was_exposed=true。
- 缺失文件、非hex、奇数hex、奇数且含非法字节、短密钥、无效UTF-8共6例，
  Go/Rust完整错误逐字相等，实际路径相同、不归一化错误。Debug及Go日志不含测试密钥。

随机nonce/ciphertext不做随机字节相等断言；比较交叉解密结果和真实持久revision。
测试密钥来自已有Go公开测试向量，不读取用户凭据。

## 验证

[原始证据](task-010-evidence.json.gz)保留命令、退出、原始日志及来源SHA。
010-red最小接口可编译，在文件模式/缺失配置的真实断言上失败；010-green两例通过。
final编译类型错误不算行为失败；final2三例通过、一例使用错误Go JSON日志字段
导致观察超时，依据实际logger改为message后修复。final3中断前4例通过，但无正常
进程退出，不将其当作完整回归；最终final4重新执行。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_010_ -- --nocapture | 0 | 4聚焦通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-encryption --lib mcs_009_ -- --nocapture | 0 | 4相邻通过 |
| cargo test --locked -p astersql-pd-compat-tests --lib task_004_ -- --nocapture | 0 | 3相邻通过 |
| cargo test --locked -p astersql-pd-replication --lib mcs_004_ -- --nocapture | 0 | 4相邻通过 |
| cargo check --workspace --locked | 0 | 工作区编译通过 |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo tree --locked -p astersql-pd-encryption --edges normal --prefix none | 0 | 生产依赖边界通过 |

动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
实际进程退出码与完整日志见证据；结束后释放自己的锁并保留缓存。
Go/模块1101项存在性、SHA、Git状态不变。依用户明确授权，包级与workspace全量
测试留至全部实现后统一运行，本项保留“已完成，待回归”，不宣称任务102完成。
