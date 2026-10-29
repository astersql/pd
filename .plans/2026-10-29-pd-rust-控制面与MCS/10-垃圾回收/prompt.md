# 垃圾回收任务提示词

按顺序复制以下提示到独立对话框执行。先阅读总plan.md、本包plan.md和对应任务文件；plan.md仅为只读架构指南，不作为进度来源。

本子计划服从全局编号和批次；一批次一个任务，全部串行，不与其他子计划重叠执行。全局前序任务见[总提示列表](/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/prompt.md)。

# 批次 43

## 任务 43: 读取键空间GC状态【批次 43】 依赖批次 42（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/43-读取键空间GC状态.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为43，依赖批次 42（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/43-读取键空间GC状态.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 44

## 任务 44: 推进事务安全点【批次 44】 依赖批次 43（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/44-推进事务安全点.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为44，依赖批次 43（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/44-推进事务安全点.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 45

## 任务 45: 维护键空间GC屏障【批次 45】 依赖批次 44（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/45-维护键空间GC屏障.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为45，依赖批次 44（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/45-维护键空间GC屏障.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 46

## 任务 46: 维护全局GC屏障【批次 46】 依赖批次 45（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/46-维护全局GC屏障.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为46，依赖批次 45（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/46-维护全局GC屏障.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 47

## 任务 47: 推进GC安全点【批次 47】 依赖批次 46（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/47-推进GC安全点.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为47，依赖批次 46（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/47-推进GC安全点.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 48

## 任务 48: 快照加载启用键空间【批次 48】 依赖批次 47（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/48-快照加载启用键空间.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为48，依赖批次 47（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/48-快照加载启用键空间.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 49

## 任务 49: 恢复启用键空间监听【批次 49】 依赖批次 48（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/49-恢复启用键空间监听.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为49，依赖批次 48（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/49-恢复启用键空间监听.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 50

## 任务 50: 批量预热GC缓存【批次 50】 依赖批次 49（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/50-批量预热GC缓存.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为50，依赖批次 49（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/50-批量预热GC缓存.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 51

## 任务 51: 重试GC预热失败【批次 51】 依赖批次 50（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/51-重试GC预热失败.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为51，依赖批次 50（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/51-重试GC预热失败.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 52

## 任务 52: 隔离GC领导任期【批次 52】 依赖批次 51（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/52-隔离GC领导任期.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为52，依赖批次 51（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/52-隔离GC领导任期.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```

# 批次 53

## 任务 53: 暴露GC状态与屏障指标【批次 53】 依赖批次 52（含此前传递依赖）

```text
请使用技能 /Users/xiangmin/.codex/skills/do-task-plans/SKILL.md 执行 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/53-暴露GC状态与屏障指标.md`。

开始前先阅读 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md`、`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 和该任务文件，确认依赖已满足。当前批次为53，依赖批次 52（含此前传递依赖）。按任务文件中的测试驱动开发/验证计划实施，只处理该任务范围，不扩大为其他子系统重写。
本任务涉及 Rust/Cargo：执行任何 Cargo 构建、检查、测试或调用 Cargo 的脚本前，先阅读并遵守 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/53-暴露GC状态与屏障指标.md` 中的“Cargo 共享槽位规则”，动态领取仓库共享槽位 1–10，设置绝对路径 CARGO_TARGET_DIR；阶段结束后清理自己的锁并保留缓存，最终报告实际槽位和 CARGO_TARGET_DIR。
任务获得所需验证证据后，标记为 `已完成`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新或删除，则无需提交。在最终回复中报告完成证据并删除该任务文件；如果被阻塞，先在任务范围内尽可能突破，只在任务文件中更新为 `已阻塞` 并记录具体原因和已运行的检查；如果因为无关本次修改的原因导致无法运行验证，可以解除 `已阻塞`，修改状态为 `已完成，待回归`，若有实际代码、测试、配置或文档变更，使用技能 `$git-commit` 提交该任务的变更；若只有编号任务文件本身的状态更新，则无需提交。保留任务文件供回归验证。必要领域能力或差分证据缺失必须保持已阻塞，不得降低验收标准。不得提交其他任务和用户已有变更。
注意，`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/plan.md` 和 `/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-29-pd-rust-控制面与MCS/10-垃圾回收/plan.md` 为只读，不要修改这些文件的任何部分，这会增加上下文的大小，还会干扰其他任务的执行。遵守任务文件的Go只读硬约束，不修改来源史诗。
```
