# rule.rs

公开 PeerRoleType::{Voter,Leader,Follower,Learner,Unknown,is_valid}；Rule 的 Go 对应字段与 runtime group；Rule::{from_json,key,store_key,group_index}；RuleGroup::{from_json,is_default}；GroupBundle。Serialize/Deserialize/Display 保留 JSON 标签、默认与 omitempty。未知角色保留并判为 invalid。Rule::clone 深拷贝集合与范围字节，清除 runtime group，不解码 hex。StoreKey 对 UTF-8 编码为小写 hex。排序与覆盖留到批次 22，协议角色转换留给适配层。

测试与 Go oracle 见 [批次 21](../../../docs/rust/schedule/placement/21-证据.md)。
