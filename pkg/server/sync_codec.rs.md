# SyncRegions 协议转换

[sync_codec.rs](sync_codec.rs) 位于 server 适配层，使用固定 kvproto
f8ad079c8e9812ed5900b0c12b5f1d97171464c4。领域 syncer/core 不依赖 protobuf。
生产模块和独立 [测试](sync_codec_test.rs) 在 lib.rs 显式注册。

`request_from_proto` / `request_to_proto` 保留可空 header/member、所有 member 字段和
HistoryIndex；codec 只转换，集群准入和流生命周期由宿主负责。
`response_from_proto` / `response_to_proto` 使用 owned SyncResponse，保留原始四类计数、
region 全部元数据、leader 未知 role/witness、bucket keys/version/region_id/stats/毫秒 period、
可空 header/error（包括 code 0 与未知枚举）、down/pending peer 各组长度与 message presence。
不截短不对齐附属数组，不把缺失消息替换为有值消息。承诺 pinned schema 的字段往返；
prost 对未知 protobuf 字段的处理不在此接口中另建存储。

`SyncResponse::into_apply` 按 Go follower 的消费子集投影到 canonical SyncApplyResponse：
保留 stats/buckets/leaders 原始数组长度；ClientApply 负责 stats/buckets 整组长度匹配、
leader 逐位置及 id=0 判定、错误 header 拒绝和 full-sync 状态。bucket 投影保留 version/keys，
与 Go SetBuckets 相同，不将 report counter 用作 heartbeat；down/pending 字段当前 Go
syncer 不消费，所以只在 adapter envelope 往返。不增加第二套应用或历史算法。

`history_to_proto`、`snapshot_to_proto`、`catch_up_to_proto` 接受已有领域帧，发送端
按 Go buildSyncRegionResponse 保持附属数组对齐，缺失 leader/bucket 写空消息；bucket
发送有效 metadata；独立 bucket report 同时转发原始 stats/毫秒 period，heartbeat
BucketMeta 覆盖时不带 report stats/period。四类 region counter 保持原始单位。snapshot offset 独立于历史索引，
completion 使用捕获的 next history index；转换不推进历史、不注册流、不确认发送结果。

不创建任务、runtime、Registry 或 I/O；没有独立资源需要取消。宿主仍负责发送成功后的
索引推进、取消及 join。真实 Go wire 与领域应用证据见
[任务50](../../docs/rust/state/task-50.md)。
