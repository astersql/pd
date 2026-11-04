# 任务011：AWS KMS主密钥

复用官方aws-config默认凭据链与aws-sdk-kms客户端，增加异步
MasterKey::from_config/from_kms_client。生成时发送KeyId/NumberOfBytes=32；恢复时
发送原CiphertextBlob/KeyId，并原样保留密文主密钥。检查返回明文长度恰好32，
敏感字节进入Zeroizing；复用既有AES-GCM进行字典封装，不安装runtime、信号或日志。

region来自元数据，SDK标准环境/profile endpoint继续生效。Go kms.go未使用持久
endpoint字段，Rust保持相同行为，没有把该字段误当作实际生效地址。
Go在AWS_ROLE_ARN和AWS_WEB_IDENTITY_TOKEN_FILE都非空时显式优先使用Web Identity，
Rust也使用官方WebIdentityTokenCredentialsProvider覆盖环境静态凭据；可选session name
由官方提供器读取。错误保留PD错误码及生成/解封装操作，已知AWS服务错误保留类别，
不透传可能回显凭据/明文的任意服务message或SDK response Debug。

依赖精确集中于workspace并提交锁文件：aws-config=1.6.3、aws-sdk-kms=1.77.0、
aws-smithy-http-client=1.0.6、aws-credential-types=1.2.3、runtime-api=1.8.1、types=1.3.2。
保留SSO、profile、process、环境、Web Identity等默认链能力；SDK HTTP连接器显式使用
现有rustls/ring，关闭默认AWS-LC连接器。没有升级或移除既有锁定package。

## Oracle与实际状态

测试按[AWS官方静态回放指南](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/testing-replay.html)
使用StaticReplayClient/ReplayEvent。仅KMS/STS边界使用官方替身；Go二进制不能链接Rust
替身，通过测试内回环HTTP适配器转发其真实SDK请求到同一官方回放实现。真实Go PD、
原生etcd、协议字典、事务与watcher不使用mock。没有访问真实付费资源。

- 逐字段核对GenerateDataKey和Decrypt真实JSON，包括32字节请求、KeyId及原密文。
  核对实际SigV4 region/service和环境测试凭据；两个SDK实际endpoint均指向回放服务。
- Go生成并持久化KMS EncryptedContent；Rust解开后当前数据密钥为32字节且未暴露。
  Rust重新包装该字典并用真实事务写回，Go在同一revision记录重新加载。
- Rust生产默认链在独立子测试进程中运行，显式传入测试环境；无unsafe全局环境修改。
  Web Identity阶段同时提供环境静态凭据和token，Go/Rust均经STS取得凭据，再用STS
  身份签署KMS；验证实际token/role/session请求及签名身份。
- token文件缺失时Go/Rust都返回KMS生成错误，不发KMS请求、不退回环境静态凭据。
- SDK返回短/缺失明文均拒绝；生成/解封装AccessDenied映射正确，服务message含敏感
  标记时TypedError Display/Debug均不输出该标记；Go日志与子进程输出无测试secret/token。

SDK内部诊断文本随语言不同，不逐字比较；比较稳定PD code/operation、请求字段、
解密后的业务状态和事务revision。随机nonce不做随机字节相等断言。

## 验证

[完整证据](task-011-evidence.json.gz)含所有阶段命令、退出码、日志及来源SHA。
011-red最小接口编译后在“不得返回明文主密钥”的行为断言失败；green导入错误不算
行为失败。green2四项通过。green3/debug补充Web Identity时失败：测试服务器只接收
Go的`/sts`路径，Rust SDK使用`/sts/`；补齐回放路由后green4通过，
没有修改生产endpoint或忽略真实请求。最终新增缺失token用例并重新执行。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-encryption --lib mcs_011_ -- --nocapture | 0 | 4聚焦通过，0失败、0忽略；含真实Go/etcd与三个实际执行的子进程 |
| cargo test --locked -p astersql-pd-encryption --lib mcs_010_ -- --nocapture | 0 | 4相邻通过 |
| cargo test --locked -p astersql-pd-encryption --lib mcs_009_ -- --nocapture | 0 | 4相邻通过 |
| cargo test --locked -p astersql-pd-encryption --lib config_test:: -- --nocapture | 0 | 2相邻通过 |
| cargo test --locked -p astersql-pd-compat-tests --lib task_004_ -- --nocapture | 0 | 3相邻通过 |
| cargo test --locked -p astersql-pd-server --lib http_store_read_test::task_051_differential -- --ignored --exact --nocapture | 0 | 1相邻差分通过 |
| cargo check --workspace --locked | 0 | 工作区编译通过 |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 76 package |
| cargo tree --locked -p astersql-pd-encryption --edges normal --prefix none | 0 | 无PD proto/私有runtime/第二TLS后端 |

相邻回归重编译还暴露既有HTTP RE2八进制lookahead类型错误：Option::filter闭包参数
多一层引用。最小修复已独立提交4a0edde26；显式执行现有task_051_differential
（--ignored --exact）实际1项通过，含八进制转义标签查询的真实Go/本机etcd差分。
该检查包含在final2，不把原有ignore标记算作跳过本次验证。

动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1。
使用仓库固定Rust1.99.0工具链验证；本任务未单独验证Rust1.85 MSRV。
未设置子进程模式时helper自行返回，其独立默认运行不计入4项聚焦通过数；父测试明确
启动并要求实际helper执行成功。进程结束释放锁、保留缓存；Go/模块1101项内容及Git
状态一致。按用户授权，包级与workspace完整测试在全部实现后统一执行，保留待回归。
