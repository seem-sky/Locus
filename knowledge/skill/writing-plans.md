---
id: kd_writing_plans_locus
type: skill
path: writing-plans.md
title: writing-plans
inheritInjectMode: true
summaryEnabled: true
commandEnabled: true
readOnly: true
aiMaintained: false
explicitMaintenanceRules: false
skillEnabled: true
skillSurface: command
commandTrigger: /writing-plans
---

# writing-plans

## Summary
在动手写代码前，为复杂或多步骤任务编写可执行的实现计划（Locus / Unity 项目）。

## Content

**开始时宣布：** "我正在使用 writing-plans 技能创建实现计划。"

### 何时使用

- 预计修改 ≥3 个源码文件，或跨多个顶层目录
- 新功能、重构、架构变更
- 尚无已批准的书面计划

### 计划保存位置

`docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md`（用户指定路径优先）

### 计划必须包含

1. **目标** — 一句话
2. **架构** — 2–3 句方案说明
3. **文件清单** — 将创建/修改的文件及职责
4. **任务分解** — 带 `- [ ]` 复选框的小步骤（每步约 2–5 分钟）
5. **验证方式** — 每条任务明确的命令（如 `cargo test`、`npm test`、Unity 手动测试步骤）

### Locus 约定

- 探索阶段用 `codegraph_context` / `task(explorer)`，不要在计划中假设已读全库
- 每个实现批次在 dev 上仍走 **Read → task(implementer) → task(optimizer) → task(reviewer)**
- Unity 变更注明 Editor 状态前提（editing vs playing）

### 完成后

获得用户对计划的确认，再进入 executing-plans 或实现阶段。
