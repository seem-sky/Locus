---
id: kd_verification_before_completion_locus
type: skill
path: verification-before-completion.md
title: verification-before-completion
inheritInjectMode: true
summaryEnabled: true
commandEnabled: true
readOnly: true
aiMaintained: false
explicitMaintenanceRules: false
skillEnabled: true
skillSurface: command
commandTrigger: /verify
---

# verification-before-completion

## Summary
宣称完成、已修复或测试通过前，必须运行验证命令并引用输出证据。

## Content

**开始时宣布：** "我正在使用 verification-before-completion 技能。"

### 铁律

```
没有本轮新鲜验证证据，不许宣称完成
```

### 门控步骤

1. 确定：什么命令能证明结论？
2. 运行：完整执行（非记忆里的旧结果）
3. 阅读：退出码、失败数、关键日志
4. 结论：仅当输出支持时才可说「通过 / 完成 / 已修复」

### Locus / Unity 常见验证

| 结论 | 示例证据 |
|------|----------|
| Rust 通过 | `cargo test` / `cargo check` 输出 0 errors |
| 前端通过 | `npm test` / `pnpm test` 0 failed |
| Unity 编译 | `unity_recompile` 成功或 Editor 无编译错误 |
| Bug 已修复 | 原复现步骤不再失败 + 相关自动化测试 |
| 代码审查 | `task(reviewer)` 或 `/code-reviewer` 非 BLOCK |

### 禁止措辞（无证据时）

- 「应该可以了」「大概没问题」「看起来正常」
- 「测试会通过」（未运行）
- 仅依赖子 Agent 口头成功报告

### 与三阶段工作流

Review 子 Agent 通过后，仍须本技能要求的**项目级验证命令**（测试/构建/lint）才能对用户宣称任务完成。
