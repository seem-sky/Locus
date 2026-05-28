## Multi-Stage Code Editing Workflow

**IMPORTANT: For code editing tasks, you MUST follow the Read → Implement → Review workflow. Never use `edit` tool directly for substantive code changes.**

### Three-Stage Workflow

#### Stage 1: Read (Analysis)

Before editing, use `read`, `grep`, `list`, or subagent (`task` with `subagent_type: "explorer"`) to:

- Read the target files to understand their structure
- Analyze dependencies and related code
- Understand the existing patterns and conventions

#### Stage 2: Implement (Editing via Subagent)

After analysis, dispatch to `implementer` subagent to implement the changes:

```
use `task` with:
{
  "description": "Implement code changes",
  "prompt": "请根据以下分析实现代码变更：\n\n[分析结果]\n\n[变更需求]\n\n请遵循项目代码规范，确保变更完整且一致。",
  "subagent_type": "implementer"
}
```

The implementer subagent will:

- Make focused, incremental changes
- Follow existing code conventions
- Ensure changes are complete and consistent

#### Stage 3: Review

After implementing, dispatch to `reviewer` subagent to review your changes:

```
use `task` with:
{
  "description": "Review code changes",
  "prompt": "请审核以下代码变更：\n\n[列出变更的文件和内容]\n\n请从以下维度审核：\n1. Quality - 代码质量、复杂度、命名\n2. Security - 安全漏洞、敏感数据\n3. Performance - 性能问题、资源使用\n4. Logic - 业务逻辑正确性",
  "subagent_type": "reviewer"
}
```

### When to Use Subagents

| Task Type                      | Subagent           | Purpose                   |
| ------------------------------ | ------------------ | ------------------------- |
| File exploration, finding code | `explorer`         | Research and analysis     |
| Code implementation            | `implementer`      | Implement code changes    |
| Code review                    | `reviewer`         | Quality/security/review   |
| Knowledge queries              | `knowledge`        | Knowledge base operations |
| Git operations                 | `git`              | Version control           |
| Unity debugging                | `runtime_debugger` | Runtime inspection        |

### Example Workflow

User request: "修改 Seat.lua 添加座位信息显示"

1. **Read**:
   - `read` file: Assets/Lua/Games/Texas/texas/table/view/ui/Seat.lua
   - `grep` for related UI components

2. **Implement**:
   - Dispatch to `implementer` subagent to make the code changes
   - Use `task` with `subagent_type: "implementer"`

3. **Review**:
   - Dispatch to `reviewer` subagent to verify the changes
   - Review the report and fix any issues

### Important Notes

- **Always review after editing**: Code changes should always go through review before being considered complete
- **Use subagents for research**: Don't try to read large codebases directly - use `explorer` subagent
- **Incremental changes**: Break large tasks into smaller, verifiable steps
