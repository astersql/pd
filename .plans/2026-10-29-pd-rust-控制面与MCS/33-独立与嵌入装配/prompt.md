# 独立与嵌入装配任务提示词

按顺序复制以下提示到独立对话框执行。先阅读总plan.md、本包plan.md和对应任务文件；plan.md仅为只读架构指南，不作为进度来源。

本子计划服从全局编号和批次；一批次一个任务，全部串行，不与其他子计划重叠执行。全局前序任务见[总提示列表](/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/prompt.md)。

# 批次 131

## 任务 131: 接通键空间领域服务【批次 131】 依赖批次 130（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/131-接通键空间领域服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为131，依赖批次 130（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/131-接通键空间领域服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 132

## 任务 132: 接通GC领域服务【批次 132】 依赖批次 131（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/132-接通GC领域服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为132，依赖批次 131（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/132-接通GC领域服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 133

## 任务 133: 接通复制模式领域服务【批次 133】 依赖批次 132（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/133-接通复制模式领域服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为133，依赖批次 132（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/133-接通复制模式领域服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 134

## 任务 134: 接通加密领域服务【批次 134】 依赖批次 133（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/134-接通加密领域服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为134，依赖批次 133（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/134-接通加密领域服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 135

## 任务 135: 接通恢复控制器领域服务【批次 135】 依赖批次 134（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/135-接通恢复控制器领域服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为135，依赖批次 134（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/135-接通恢复控制器领域服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 136

## 任务 136: 装配嵌入时间戳服务【批次 136】 依赖批次 135（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/136-装配嵌入时间戳服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为136，依赖批次 135（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/136-装配嵌入时间戳服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 137

## 任务 137: 装配嵌入路由服务【批次 137】 依赖批次 136（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/137-装配嵌入路由服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为137，依赖批次 136（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/137-装配嵌入路由服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 138

## 任务 138: 装配嵌入资源管理服务【批次 138】 依赖批次 137（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/138-装配嵌入资源管理服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为138，依赖批次 137（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/138-装配嵌入资源管理服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 139

## 任务 139: 装配嵌入调度服务【批次 139】 依赖批次 138（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/139-装配嵌入调度服务.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 和该任务文件，确认依赖已满足。当前批次为139，依赖批次 138（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/139-装配嵌入调度服务.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/33-独立与嵌入装配/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```
