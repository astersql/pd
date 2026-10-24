# 任务37：同步响应应用

起点 `a7c4e7a3958283d780c49ffcd9e946fc8d5d6736`，依赖任务36已完成。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，client.go/client_test.go无漂移。

## 实现前运行oracle

[既有测试输出](task37/oracle.out)：5项通过，错误帧保留索引10且running=false；
全量完成分别索引1、43、1043，Canceled错误码通过。入口见[脚本](task37/oracle.sh)。
TestMain与调用路径只读审查：goleak/连接等待，不启用注入。CGO_ENABLED=0、
GOFLAGS=-mod=readonly、GOPROXY=off，显式编译既有文件，无Go新增或改写。

[字段输入命令](task37/fields.commands)、[运行快照](task37/fields.out)：
既有TestClientWaitsForEmptySnapshotCatchUpCompletion生成真实响应，在client.go:109
用Delve修改输入，三个区域ID1/2/3、相同[0001,0002)范围，版本5/4/6，start10；
stats/buckets完整3项，writtenBytes=101/0/303，最后bucket version7，leaders仅1项。
在151逐项读实际RegionInfo/err，第二项真实stale；最终真实索引map只含ID3、history12。
leader第一项ID2，第三项nil；raw计数保留，bucketMeta version7，source=Sync。
[不匹配输入](task37/mismatch.commands)、[输出](task37/mismatch.out)：stats/buckets仅1项，
leader首项ID0；两次成功区域统计全0、buckets/leader均nil，stale与最终map/index不变。
这些是执行生产函数后读取的字段，不是源码推导；变更输入后在返回前停止，
不将原测试不适用的后续断言计为通过。无人工mock统计/索引结果。

[加载命令](task37/load.commands)、[输出](task37/load.out)：TestLoadRegion执行到既有
failpoint.Inject(slowLoadRegion)，完成4条参数入栈指令后，Delve调用既有fpbody闭包。
输出确认闭包为StorageEndpoint.LoadRegions.func1，捕获rangeLimit10000；实际闭包
将limit设1并Sleep1秒，之后真实加载因context canceled退出，耗时1.030949542秒，
原测试PASS、进程退出0。不是无注入裸跑；没有生成Go注入文件或替换函数结果。
早期入口参数未入栈导致的调试错误和无实际闭包调用的PASS已排除，不作为证据。
Go只读限制允许复用既有调试二进制；这里仅修改进程输入和调用已有闭包。

最初oracle使用动态槽位1；最终可复现oracle及Rust验证使用动态槽位2，
绝对CARGO_TARGET_DIR分别为`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`、
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。资源例外见下文。
真实MemoryStore与EtcdMetaStore验收将在实现后运行；加载启动生命周期属于后续任务，
本任务只迁移响应应用，以上加载证据用于确认既有Go加载/取消基准。

## 最终可复现证据与实现

[入口](task37/snapshot.sh)依次接受fields/mismatch/load；外层用slot.sh领取共享槽位，
GOARCH=arm64并继承readonly/off/CGO=0设置；容器使用Delve1.27.1，真实ARM64 Go调试
二进制由同一组已有文件编译。最终[fields](task37/fields-final.out)、
[mismatch](task37/mismatch-final.out)、[load](task37/load-final.out)均重新执行，
加载耗时1.026777167秒、TestLoadRegion单项通过；退出0。字段快照在函数返回前读取并
退出调试进程，不将修改事件后的原测试断言作为通过证据。
[快照校验](task37/check-oracle.py)退出0，[结果](task37/oracle-check.out)。

红灯见[原生验证](task37/red-native.out)：5项实际执行，0通过/5断言失败，退出101，
包括真实Etcd；期望handled=true但实际false、期望索引12但实际0、运行状态错误。
最小接口编译成功，无未定义符号或零测试替代。早期默认x86运行单列为`/tmp/pd-state37-audit/default-red.out`，
其Memory断言有效，但夹具启动受系统问题阻塞，不计作完整红灯。

`client_apply.rs`复用core precheck/region_guide、BasicCluster真实索引、Storage区域
门面与owned领域类型；适配层传入原始四维统计、可空bucket meta及leader，不引入wire类型。
错误帧提前退出；stats/buckets整体长度匹配、leaders逐位置/非零判定；stale跳过；
cache先于save、save失败抑制记录但继续删除overlaps，delete失败不阻止后续条目。
全量中间位置索引不当作HistoryIndex提交，空帧完成后才running。
`ApplyHistory`桥接已有HistoryBuffer/PersistentHistory，无第二套历史实现；保留100条
检查点、内存先提交、失败检查点计数不回滚。生产模块与独立测试均显式登记，
每个生产文件有版权和相邻rs.md；新增依赖仅既有Storage/Keypath，锁文件仅增加两条引用。

[完整验证](task37/validate.py)、[摘要](task37/summary.json)：cargo fmt --all、聚焦9项、
包级62项、metadata、fmt check、workspace check、workspace test均退出0；workspace
1201通过/0失败/1既有ignored（tso_processes需要独立进程二进制，同任务36）。
随后仅补充检查点失败行为测试，生产代码未改动；
[最终验证](task37/final-focus.py)/[摘要](task37/final-summary.json)退出全0：fmt、
聚焦10通过/0忽略、包级63通过/0忽略、fmt check。所有命令从根目录执行，
GOARCH=arm64用于Go子进程，并持有绝对CARGO_TARGET_DIR槽位2；日志逐命令保存。
额外Rust故障/极值测试为source-derived扩展，不把它们冒充额外Go运行差分。

## 系统阻塞与待回归范围

默认环境go env GOARCH=amd64，而宿主uname为arm64。首次红灯夹具
PID82346（slot1/rust-etcd-fixture-82290，Mach-O x86_64）在macOS陷入U不可中断状态，
SIGTERM/SIGKILL后仍未退出，Rust测试进程82290等待其结束，外层slot持有PID82189
仍存在。另一个只读objdump尝试PID81646也为U，读取槽位1的调试二进制；已发送终止
信号，尚未退出。没有结束其他任务的进程，没有删除活跃槽锁或缓存。

绕开x86执行后，原生ARM64夹具、必需Go oracle、全部Rust行为与workspace验证均通过；
尚无法保证首次默认环境验证进程的回收完成。槽位2的所有测试/容器/Etcd子进程已结束，
锁已释放，缓存保留；槽位1按规则继续保留锁，不能在上述进程仍存在时伪造退出证据。
这是外部系统进程状态问题，不是必要oracle缺失。状态保留为“已完成，待回归”：
待系统允许U进程退出后，确认PID82346/81646/82290及持有PID82189均结束，释放自己的
slot1锁（正常runner退出会自动释放），按相同脚本复核回归与只读状态后再删除编号任务。
无需放宽Go只读，不要求新增Go工具，不需要修改领域边界；不请求重启或更改系统策略。

[只读核对](task37/readonly.out)：1101个Go/模块文件清单、SHA256与Git状态完全一致，
聚合SHA256 ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4；
全部只读plan.md哈希不变，相对链接及git diff检查通过。用户已有计划变更保持原样。

状态：已完成，待回归（仅默认x86验证的系统进程回收；必要行为证据已齐全）。
