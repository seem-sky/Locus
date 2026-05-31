---
id: kd_code_optimizer_locus
type: skill
path: code-optimizer.md
title: code-optimizer
inheritInjectMode: true
summaryEnabled: true
commandEnabled: true
readOnly: false
aiMaintained: false
explicitMaintenanceRules: false
skillEnabled: true
skillSurface: command
commandTrigger: /code-optimizer
argumentHint: "[path|symbol|scope] [--focus performance|readability|memory|all]"
---

# code-optimizer

## Summary
在保持行为不变的前提下，对指定代码做可验证的优化（性能、内存、可读性、结构），输出最小改动方案与验证证据（Locus / Unity 项目）。

## Content

**开始时宣布：** "我正在使用 code-optimizer 技能。"

### 何时使用

- 用户明确要求「优化」「提速」「减 GC」「简化」「重构但不动行为」
- 已有可运行基线（测试、复现步骤或性能指标），或能先建立基线
- 改动范围已界定（文件、模块、函数或用户给出的 diff）

### 何时不要用

- 功能尚未实现或行为未定义 — 先 `writing-plans` 或正常开发
- 正在排查未知故障 — 先 `systematic-debugging`（`/debug`）
- 仅需审查风险、不做优化 — 用 `code-reviewer`（`/code-reviewer`）

### 优化优先级（不可颠倒）

1. **正确性** — 语义与对外契约不变；边界条件不回归
2. **可观测收益** — 能说明为何更快/更省/更清晰（数据、复杂度、调用链、分配次数）
3. **最小 diff** — 优先局部修改；避免无关重命名、搬文件、大面积格式化
4. **可维护性** — 不引入过度抽象；与项目现有风格一致

### 工作流程

1. **界定范围**
   - 解析用户参数：目标路径/符号、`--focus`（默认 `all`）
   - 复杂改动前用 CodeGraph：`codegraph_context` / `codegraph_impact`，说明影响面

2. **建立基线**
   - 读清相关文件与调用方（`read` / CodeGraph，禁止凭猜测改结构）
   - 记录当前行为：关键测试、日志指标、Profiler 截图或计时方式
   - Unity 热路径：注明是否在 Play Mode、是否 Edit Mode 测试

3. **识别问题（带证据）**
   - 性能：热循环分配、重复计算、N+1、同步 IO、过大数据结构、错误算法阶
   - 内存：泄漏模式、缓存无界增长、装箱/临时集合、Lua table 膨胀
   - 可读性：过长函数、重复逻辑、误导命名、可内联的间接层
   - 每条发现标注：**位置 + 原因 + 预期收益 + 风险**

4. **提出方案**
   - 给出 1–3 个可选方案时，标明推荐项与 trade-off
   - 默认选**最小、可回滚**的方案；大重构需用户明确同意

5. **实施**
   - 按方案编辑；跨文件/公共 API 改动前再次 `codegraph_impact`
   - Unity：Play Mode 下不用 `unity_execute` 改场景；改 `.cs` 后注意编译与 domain reload
   - Locus 前端/Rust：遵循仓库脚本（优先 `rtk` 前缀命令）

6. **验证（必须）**
   - 运行与范围匹配的命令，例如：`rtk cargo test`、`rtk vitest run …`、Unity Profiler / `view_console_read`
   - 对比优化前后：测试通过、指标改善或「无劣化」的说明
   - 未完成验证不得声称「已优化完成」

### 输出格式

```markdown
## 优化范围
- …

## 发现（按严重度）
### 必须处理（正确性/明显浪费）
- [文件:行] 问题 — 证据 — 建议

### 建议处理（可观收益）
- …

### 可选（风格/微优化）
- …

## 实施方案
- 推荐方案：…
- 影响面：…

## 验证
- [ ] 命令/步骤 — 结果
```

### Locus / Unity 提示

| 场景 | 建议 |
|------|------|
| 查调用链、改公共符号 | CodeGraph impact / trace |
| Rust / TS 测试 | `rtk cargo test`、`rtk vitest run` |
| Lua / Unity C# 热路径 | Profiler、`unity_execute` 探针、避免 Update 内分配 |
| 仅文档/配置 | 说明无需代码优化或只做文案精简 |

### 禁止

- 为「看起来更好」做大范围重写而无测量
- 混合无关功能改动或顺手改命名/格式
- 牺牲可读性换取难以证明的微优化
- 无验证输出就宣称完成
