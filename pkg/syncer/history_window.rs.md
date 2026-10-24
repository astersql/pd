# 历史窗口保留与自适应容量

对应只读Go history_buffer.go的retain/release、observeRequiredWindow、maybeShrink、
prepareRequiredWindowLocked与resizeLocked；证据见 [task-31](../../docs/rust/state/task-31.md)。

扩展既有HistoryBuffer，不引入第二个历史容器。with_config对base/max按unit*2^n归一化，
max不低于base；追加时依据最早retain预先增长，达到max后仍覆盖最旧记录。
retained_records_from返回完整Arc快照和next；请求不在[first,next]时返回带三项索引的
HistoryGap，调用者可在协议边界映射ResourceExhausted。等于next成功返回空快照。
普通records_from仍保持Go nil/空Vec契约；增长无法恢复已经丢失的历史。

HistoryRetain为不可克隆、可跨线程移动的所有权guard；同一index可创建多个独立guard。
Drop减少引用计数，失败返回、future取消、栈展开均自动释放，无额外后台任务。
宿主以自己的锁序列化buffer变更；guard仅锁自己的计数表，不反向取得宿主锁。
reset清空记录、恢复base、重置观察和低窗口计数，并替换计数表；旧guard不能释放新代保留。

observe保存本轮最大窗口；大于capacity/2时尝试增长到window*2，并受max限制。
maybe_shrink消费观察值；存在retain、已到base或窗口>capacity/4则清零连续轮数；
否则三轮后最多减半一次，保留最新记录且next不变。释放最后一个guard后重新累计三轮。
增长分配失败返回HistoryError，追加前检查索引溢出，错误不覆盖已有记录。
Rust以饱和乘法、先clamp后归一化避免Go整数溢出；不宣称Go溢出语义相同。
纯领域模块不依赖protobuf、MetaStore、时钟、runtime或全局Registry；存储与流由后续批次接线。
