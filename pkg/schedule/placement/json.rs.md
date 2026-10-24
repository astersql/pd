# json.rs

内部 go_object/go_field 宏处理 object/null、折叠字段名、未知字段、重复字段、标量 null 不覆盖、slice null 清空及字符串 slice null 元素。fold_field 支持 Kelvin/long-s；json_string 为 Display 保留 HTML/U+2028/U+2029 转义；zero 实现 omitempty。非通用 Go JSON 替代。

测试与 Go oracle 见 [批次 21](../../../docs/rust/schedule/placement/21-证据.md)。
