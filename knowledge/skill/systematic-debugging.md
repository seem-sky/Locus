---
id: kd_systematic_debugging_locus
type: skill
path: systematic-debugging.md
title: systematic-debugging
inheritInjectMode: true
summaryEnabled: true
commandEnabled: true
readOnly: true
aiMaintained: false
explicitMaintenanceRules: false
skillEnabled: true
skillSurface: command
commandTrigger: /debug
---

# systematic-debugging

## Summary
在提出修复前进行根因调查；禁止未验证的猜测式改码（Unity / Locus 项目）。

## Content

**开始时宣布：** "我正在使用 systematic-debugging 技能。"

### 铁律

```
未完成根因调查，不得提出修复方案
```

### 四阶段

1. **根因调查** — 读完整错误/堆栈；稳定复现；`git diff` / 近期变更；多组件系统在各边界加日志
2. **假设与验证** — 单变量假设；用只读工具或最小探针验证
3. **修复** — 针对根因的最小改动；通过 `task(implementer)` 实施 substantive 代码修改
4. **回归验证** — 复现步骤通过 + 相关测试/构建命令输出

### Locus / Unity 提示

- Play Mode 下勿用 `unity_execute` 改场景/资源（会丢失）
- 脚本改动后注意 domain reload；文件工具改资源后需 reimport/recompile 再验证
- 优先 `unity_execute` / `print` 探针，必要时 `view_console_read`

### 禁止

- 未理解问题就连续 `edit`
- 用破坏性操作（`rm -rf`、force push）清障
- 宣称「应该修好了」而无验证输出

修复验证通过后，回到 executing-plans 或 verification-before-completion。
