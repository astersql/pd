# 生产任务 10 本地裁决

接受 OpenRaft 0.9.22 与 rusqlite 0.39.0、bundled SQLite 3.51.3 的日志重放
组合用于本地原生 MetaStore 验收。上一版持久应用表的第二次同步路径仍保留
历史比较。决定依据是同步完成边界、设备故障模型、快照追赶和冻结 etcd 门槛。
[ADR](adr.md)、[掉电证据](power-evidence.md)、[冻结门槛](etcd-baseline.md)。

生产发布仍需主题 9 的全量准入、生产节点认证和受控成员管理、混合版本及升级、
目标平台和部署 SLO。仓库公开适配器接受宿主注入的网络，本地故障夹具仅监听
loopback。独立 pd-server 的默认后端沿用外部 etcd。
