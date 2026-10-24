# 诊断状态值

对应 Go status.go。`StatusCode(i64)` 保留 Go 64 位 int 的全部值、29 个关联常量，
不以封闭 enum 排除未知状态。`status_text` 及 `Status` 的 Display 使用静态映射，
未知代码与 1008 都返回空串。`Status` 的公开字段为 status_code、detailed_reason；
new 只复制第一项 reason（没有拼接），默认值为 OK/空 reason。

is_ok 只接受 0；is_normal 仅接受 100..=109。priority 对 100..=199 返回
代码转 f32 后除以 100，其他代码先整数除以 100（向零截断），再转 f32。
因此 IsNormal 和 priority 的特殊区间不同，不能合并判断。

Eq/Hash 包含 reason，满足后续汇总按完整 Status 计数；Clone 复制拥有的 reason。
零依赖、无协议/存储/后台任务。独立 plan_status_test.rs 显式登记；真实 Go
调试调用的 38 行固定输入输出在 fixtures/status.tsv，出处见
[批次 6 证据](../../../docs/rust/schedule/plan/6-证据.md)。
