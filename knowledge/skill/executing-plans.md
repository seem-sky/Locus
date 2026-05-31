---
id: kd_executing_plans_locus
type: skill
path: executing-plans.md
title: executing-plans
inheritInjectMode: true
summaryEnabled: true
commandEnabled: true
readOnly: true
aiMaintained: false
explicitMaintenanceRules: false
skillEnabled: true
skillSurface: command
commandTrigger: /executing-plans
---

# executing-plans

## Summary
按书面计划分批执行复杂任务，每批带验证与审查检查点。

## Content

**开始时宣布：** "我正在使用 executing-plans 技能来实现此计划。"

### 流程

1. **加载计划** — 读取 `docs/superpowers/plans/...` 或用户指定文件
2. **审查计划** — 检查任务顺序、依赖、验证条件是否可执行；有疑问先问用户
3. **TodoWrite** — 将计划任务同步到 todo 列表
4. **逐任务执行** — 每任务：标记进行中 → 实现 → 验证 → 标记完成
5. **批次报告** — 每批完成后简要汇报进度与风险

### 每个实现批次（Locus dev）

即使有计划，代码变更仍须：

1. Read（read / codegraph / task(explorer)）
2. `task(subagent_type: "implementer")`
3. `task(subagent_type: "optimizer")`
4. `task(subagent_type: "reviewer")`

### 验证

- 每任务结束运行计划中写的命令，引用输出
- 不要跳过「运行测试」类步骤

### 阻塞时

- 用 systematic-debugging（`/debug`）处理失败，不要猜测式改码
- 计划与现状严重偏离时，暂停并更新计划或征求用户意见

### 全部完成后

使用 verification-before-completion（`/verify`）做最终验证，再宣称完成。
