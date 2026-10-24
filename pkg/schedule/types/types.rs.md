# schedule/types 领域标识

`CheckerSchedulerType` 是开放字符串值（支持未知、空串和 Unicode），通过 Display 提供 Go String() 的同义行为。26 个常量按 Go 原始拼写公开为 Rust 大写常量。

`CHECKERS`/`SCHEDULERS` 是已知标识枚举，不把未知后缀字符串分类为合法类型。Go 包没有默认 checker 列表，故不虚构对应 API。

`string_to_scheduler_type` 对应 19 项规范 scheduler 输入（包含 scatter-range 别名）；`convert_old_str_to_type` 对应 18 项旧名字；`scheduler_type_compatible` 反向映射回持久化名字。所有匹配区分大小写，不裁剪空白；不存在用 None 表示，与 Go map 的存在位对应。

`default_schedulers(bool)` 使用显式 NextGen 选择，返回独立拥有的列表；`current_kernel_default_schedulers` 复用 canonical kerneltype 的 is_next_gen。无可变全局、协议、I/O、存储、运行时、后台任务或 feature。

验收与 Go 运行差分来源见 [证据](../../../docs/rust/schedule/types/1-证据.md)。
