# 固定 kvproto 协议构建

`build.rs`校验固定版本kvproto归档的SHA256，用vendored protoc生成Rust绑定。
归档展开仅提取`.proto`文件到OUT_DIR下的`proto-source`，使用安全相对路径，
既有完整`source`目录不被删除或改写。这使缓存重建不创建、覆盖或删除归档内的
Go源码和模块文件；不改变版本、校验和、生成入口和协议内容。

任务144验证环境用已有归档的只读查找替代`go mod download`，真实Go命令被拒绝。
常规构建仍沿用已有模块获取入口；任务专用拦截器不属于生产协议依赖。
重新生成、workspace检查及测试见[任务144证据](../../docs/rust/schedule-execution/task-144.md)。
