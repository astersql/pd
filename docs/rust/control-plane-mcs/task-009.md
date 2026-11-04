# 任务009：数据密钥密码操作

来源：crypter.go与TestAesGcmCrypter/TestKeyLength/TestNewDataKey；
核对master_key.go的Encrypt/Decrypt、key_manager.go的轮转调用及
region_crypter.go的method/IV检查。保留同一个canonical encryption crate，
不引入protobuf、KMS、存储后端或runtime依赖。

## 实现

支持AES128/192/256的16/24/32字节数据密钥；拒绝plaintext、UNKNOWN及扩展方法。
领域EncryptionMethod保留未知数值，固定值转换不依赖proto。
NewDataKey使用OsRng分别完整填充8字节big-endian ID和对应长度的key，保留调用方
creation_time，was_exposed=false；CTR IV为16字节，GCM nonce为12字节。
没有可替换RNG/mock，生成ID不添加Go未承诺的非零条件。DataKey Debug不输出key字节。

AES-GCM使用缓存的RustCrypto aes=0.8.4、aes-gcm=0.10.3，集中固定workspace版本，
开启实际使用的aes/alloc/zeroize特性；随机数复用既有rand=0.8.8/OsRng。
三个key尺寸均使用96-bit nonce、128-bit tag，无AAD；返回ciphertext+tag。
解密先检查nonce长度，再检查key长度和认证，认证失败不返回明文。
保留PD错误码及Go错误优先级；多余格式参数的显示依据现存pingcap/errors
normalize.go的GenWithStackByArgs/GetMsg实现，不把它当作新的Go进程观察。

## 保存的Go oracle

本项按计划使用已保存Go向量，没有运行或编译Go密码包/Go测试。

- [PD向量](../../../pkg/encryption/rust-fixtures/mcs-009.json)：原样提取
  pkg/encryption/crypter_test.go的key、plaintext、nonce和48字节ciphertext/tag；
  fixture记录原Go文件SHA及TestAesGcmCrypter符号，最终再次核对来源SHA。
- [Go标准库向量](../../../pkg/encryption/rust-fixtures/mcs-009-go-stdlib.json)：
  从现存Go1.26.8工具链的crypto/cipher/gcm_test.go提取12条无AAD、nonce=12、
  tag=16的128/192/256位向量，记录完整源文件SHA。三种长度的空/非空明文均覆盖。
  额外3条tag=12的原数据也保存在fixture的排除说明里；源码429–436明确使用
  NewGCMWithTagSize，而PD使用NewGCM固定tag=16，故不改变PD协议来适配它们。

所有选中向量的加密输出逐字节相等，解密逐字节恢复原明文，无归一化。
PD原例同样修改密文首字节验证认证失败。另覆盖nonce/tag篡改、短于tag的密文、
空明文、非法key长度和nonce错误优先级。随机输出验证长度与metadata契约，
不声称其随机字节与某次Go输出相等，也未用统计唯一性代替CSPRNG选择。

## 验证

[task-009-evidence.json.gz](task-009-evidence.json.gz)包含确切命令、退出码、原始输出、
fixture内容和源码SHA。首次--locked缺少新增dev依赖登记只算准备失败；
offline注册后，最小可编译接口在Go密文/长度断言上2失败，退出101。
实现后3通过；补充标准库向量时发现3条tag=12构造器不匹配，保留该失败。
按PD参数筛选后最终：

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_009_ -- --nocapture | 0 | 4通过、0失败、0忽略；13条保存向量和其余边界 |
| cargo test --locked -p astersql-pd-encryption --lib config_test:: -- --nocapture | 0 | 2相邻通过 |
| cargo check --workspace --locked | 0 | 编译通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo tree --locked -p astersql-pd-encryption --edges normal --prefix none | 0 | 无PD proto/server/runtime/KMS/HTTP普通依赖 |

动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
Cargo.lock增加10个缓存密码依赖包及必要引用，没有升级原有包；清理无关排序差异。
Go/模块1101项存在性/SHA/Git状态一致，Go SDK来源文件SHA也与fixture保持一致。
自有进程结束后释放锁，缓存保留。完整workspace/包测试按照用户授权统一留到
全部实现后，本项保留待回归，不宣称主密钥、轮转、region加密或任务102完成。
