/*
 * @Author         : seem.sky@gmail.com
 * @Email          : seem.sky@gmail.com
 * @Description    :
 * @FilePath       : \src-tauri\src\agent\workflow\mod.rs
 * @Date           : 2026-05-28 11:12:46
 * @LastEditTime   : 2026-05-29 18:30:04
 * @LastEditors    : seem.sky@gmail.com seem.sky@gmail.com
 */
//! Dev agent code-edit workflow gate: Read → Plan → Implement (subagent) → Optimize (subagent) → Review (subagent).

pub mod completion_report;
pub mod whitelist;

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use whitelist::{
    WorkflowAmbiguousWhitelist, WORKFLOW_TOOL_WHITELIST_FILENAME,
};

const AGENT_DEV_ID: &str = "dev";

/// Reminder injected with workflow gates — applies at every phase (Read / Plan / Implement / Optimize / Review / retry).
const SOURCE_CODE_DISCIPLINE: &str = "At every phase: read relevant source in full, reason about normal/edge/error/runtime behavior, then make only minimal cautious edits.";

/// Options for `ask_user_question` during PLAN phase.
const PLAN_ASK_USER_OPTIONS: &str = "确认执行 / 取消 / 修改";

/// Max auto-continuations when the model ends a turn with text only while workflow is incomplete.
const MAX_WORKFLOW_TEXT_STOP_NUDGES: u32 = 3;

/// Required per-file detail in the modification plan (PLAN phase).
const PLAN_FILE_CHANGE_DETAIL: &str = "For EACH file: path; change type (add/modify/delete); target symbols or line ranges; \
current behavior; planned behavior; before→after snippet or pseudo-diff; runtime/edge-case notes for that file.";

/// Full PLAN content checklist (file list + per-file detail + impact + rollback).
const PLAN_CONTENT_CHECKLIST: &str = "modification plan: (1) file list, (2) detailed per-file changes \
(change type, symbols/lines, current vs planned behavior, before→after snippets, per-file runtime notes), \
(3) impact assessment, (4) rollback strategy";

/// Per-session workflow state (survives across `chat` invocations on the same session).
pub type DevWorkflowGateStore = Arc<Mutex<HashMap<String, WorkflowGate>>>;

/// Write/mutate tools the parent Dev agent must not see in the LLM tool list while the workflow gate blocks them.
const WORKFLOW_BLOCKED_WRITE_TOOLS: &[&str] = &[
    "edit",
    "write"
];

/// Bash is hidden during implement/review (and after complete) but available in READ for read-only commands.
const WORKFLOW_BLOCKED_WRITE_AND_BASH: &[&str] = &["edit", "write", "bash"];

/// CodeGraph tools surfaced first during READ when `codegraph_gate` is still false.
const READ_PHASE_CODEGRAPH_PRIORITY: &[&str] = &[
    "codegraph_context",
    "codegraph_impact",
    "codegraph_trace",
    "codegraph_callers",
    "codegraph_callees",
    "codegraph_search",
    "codegraph_files",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeEditPhase {
    Idle,
    Read,
    Plan,
    Implement,
    Optimize,
    Review,
    Complete,
}

impl CodeEditPhase {
    fn label(self) -> &'static str {
        match self {
            CodeEditPhase::Idle => "idle",
            CodeEditPhase::Read => "read",
            CodeEditPhase::Plan => "plan",
            CodeEditPhase::Implement => "implement",
            CodeEditPhase::Optimize => "optimize",
            CodeEditPhase::Review => "review",
            CodeEditPhase::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanConfirmationChoice {
    Confirm,
    Cancel,
    Modify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Pass,
    PassWithRisks,
    Block,
    Unknown,
}

/// Payload for a pending workflow completion report (one per review cycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionReportTrigger {
    pub review_cycle: u32,
    pub verdict: Option<ReviewVerdict>,
    pub zero_change: bool,
}

#[derive(Debug, Clone)]
pub struct WorkflowGate {
    phase: CodeEditPhase,
    /// File/keyword exploration (`read`, `grep`, `list`, `task(explorer)`, …).
    exploration_satisfied: bool,
    /// Structural analysis via CodeGraph (callers, callees, impact, trace, …).
    codegraph_satisfied: bool,
    /// User confirmed the written modification plan (`ask_user_question` → 确认执行).
    plan_confirmed: bool,
    /// Confirmed plan has no file changes — skip implementer/optimizer/reviewer.
    plan_zero_change: bool,
    strict: bool,
    review_cycle: u32,
    /// Consecutive text-only stops nudged in the current workflow phase.
    workflow_text_stop_nudges: u32,
    /// Set when the workflow cycle completes and a completion report should be emitted.
    completion_report_pending: Option<CompletionReportTrigger>,
    /// Prevents duplicate reports for the same review cycle.
    completion_report_issued_for_cycle: Option<u32>,
}

impl Default for WorkflowGate {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowGate {
    pub fn new() -> Self {
        Self {
            phase: CodeEditPhase::Read,
            exploration_satisfied: false,
            codegraph_satisfied: false,
            plan_confirmed: false,
            plan_zero_change: false,
            strict: dev_workflow_strict_enabled(),
            review_cycle: 0,
            workflow_text_stop_nudges: 0,
            completion_report_pending: None,
            completion_report_issued_for_cycle: None,
        }
    }

    pub fn with_strict(strict: bool) -> Self {
        Self {
            phase: CodeEditPhase::Read,
            exploration_satisfied: false,
            codegraph_satisfied: false,
            plan_confirmed: false,
            plan_zero_change: false,
            strict,
            review_cycle: 0,
            workflow_text_stop_nudges: 0,
            completion_report_pending: None,
            completion_report_issued_for_cycle: None,
        }
    }

    fn mark_completion_report_pending(
        &mut self,
        verdict: Option<ReviewVerdict>,
        zero_change: bool,
    ) {
        self.completion_report_pending = Some(CompletionReportTrigger {
            review_cycle: self.review_cycle,
            verdict,
            zero_change,
        });
    }

    /// Consume a pending completion report trigger once per review cycle.
    pub fn take_completion_report_pending(&mut self) -> Option<CompletionReportTrigger> {
        let trigger = self.completion_report_pending.as_ref()?;
        if self.completion_report_issued_for_cycle == Some(trigger.review_cycle) {
            self.completion_report_pending = None;
            return None;
        }
        let trigger = self.completion_report_pending.take()?;
        self.completion_report_issued_for_cycle = Some(trigger.review_cycle);
        Some(trigger)
    }

    /// READ gate: exploration always required; CodeGraph required only for complex edits (enforced at implementer dispatch).
    pub fn read_satisfied(&self) -> bool {
        self.exploration_satisfied
    }

    /// Tool names to omit from the LLM request list while strict workflow blocks direct edits.
    pub fn hidden_request_tools(&self) -> &'static [&'static str] {
        if !self.strict {
            return &[];
        }
        match self.phase {
            // READ / PLAN: hide write tools only; bash stays available for read-only exploration (git diff, etc.).
            CodeEditPhase::Read | CodeEditPhase::Plan => WORKFLOW_BLOCKED_WRITE_TOOLS,
            CodeEditPhase::Implement
            | CodeEditPhase::Optimize
            | CodeEditPhase::Review
            | CodeEditPhase::Complete
            | CodeEditPhase::Idle => WORKFLOW_BLOCKED_WRITE_AND_BASH,
        }
    }

    /// Move CodeGraph analysis tools to the front of the request list during READ/PLAN (after meta tools).
    pub fn prioritize_request_tools(&self, names: &mut Vec<String>) {
        if self.codegraph_satisfied {
            return;
        }
        if self.phase != CodeEditPhase::Read && self.phase != CodeEditPhase::Plan {
            return;
        }
        let meta_count = names
            .iter()
            .take_while(|name| matches!(name.as_str(), "tool_load" | "tool_call"))
            .count();
        let rest = names.split_off(meta_count);
        let mut priority = Vec::new();
        let mut others = Vec::new();
        for name in rest {
            if READ_PHASE_CODEGRAPH_PRIORITY.contains(&name.as_str()) {
                priority.push(name);
            } else {
                others.push(name);
            }
        }
        priority.sort_by_key(|name| {
            READ_PHASE_CODEGRAPH_PRIORITY
                .iter()
                .position(|candidate| *candidate == name.as_str())
                .unwrap_or(usize::MAX)
        });
        names.extend(priority);
        names.extend(others);
    }

    fn reset_read_gates(&mut self) {
        self.exploration_satisfied = false;
        self.codegraph_satisfied = false;
        self.plan_confirmed = false;
        self.plan_zero_change = false;
        self.workflow_text_stop_nudges = 0;
    }

    fn maybe_advance_read_to_plan(&mut self) {
        if self.phase == CodeEditPhase::Read && self.read_satisfied() {
            self.phase = CodeEditPhase::Plan;
            self.plan_confirmed = false;
            self.plan_zero_change = false;
            self.workflow_text_stop_nudges = 0;
        }
    }

    fn reset_workflow_text_stop_nudges(&mut self) {
        self.workflow_text_stop_nudges = 0;
    }

    fn incomplete_text_stop_nudge(&self) -> Option<&'static str> {
        if !self.strict {
            return None;
        }
        match self.phase {
            CodeEditPhase::Plan if !self.plan_confirmed => Some(
                "Do not end the turn yet. You wrote a modification plan (or said you would) but did not call ask_user_question. \
                 Natural-language \"please confirm\" does NOT show UI — you MUST call ask_user_question with options \
                 确认执行 / 取消 / 修改 so the user can confirm the plan before task(implementer).",
            ),
            CodeEditPhase::Plan if self.plan_confirmed && self.plan_zero_change => None,
            CodeEditPhase::Plan if self.plan_confirmed => Some(
                "Do not end the turn yet. The plan is confirmed — dispatch task(subagent_type=\"implementer\") now. \
                 Do not reply with text only.",
            ),
            CodeEditPhase::Implement => Some(
                "Do not end the turn yet. Dispatch task(subagent_type=\"implementer\") to apply the confirmed plan. \
                 Parent dev agent must not edit/write/bash directly.",
            ),
            CodeEditPhase::Optimize => Some(
                "Do not end the turn yet. Dispatch task(subagent_type=\"optimizer\") to refine the implementer output before review.",
            ),
            CodeEditPhase::Review => Some(
                "Do not end the turn yet. Dispatch task(subagent_type=\"reviewer\") and wait for PASS or PASS_WITH_RISKS.",
            ),
            _ => None,
        }
    }

    /// Whether the agent must not stop on a text-only turn (strict workflow incomplete).
    pub fn needs_incomplete_workflow_continuation(&self) -> bool {
        self.incomplete_text_stop_nudge().is_some()
            && self.workflow_text_stop_nudges < MAX_WORKFLOW_TEXT_STOP_NUDGES
    }

    /// Consume one auto-continuation nudge when the model tried to stop with text only.
    pub fn take_incomplete_text_stop_nudge(&mut self) -> Option<String> {
        let message = self.incomplete_text_stop_nudge()?;
        if self.workflow_text_stop_nudges >= MAX_WORKFLOW_TEXT_STOP_NUDGES {
            return None;
        }
        self.workflow_text_stop_nudges += 1;
        Some(message.to_string())
    }

    fn record_read_tool_success(&mut self, tool_name: &str) {
        if is_exploration_tool(tool_name) {
            self.exploration_satisfied = true;
        }
        if is_codegraph_analysis_tool(tool_name) {
            self.codegraph_satisfied = true;
            if tool_name == "codegraph_context" {
                // Primary onboarding tool — includes snippets, satisfies exploration scope.
                self.exploration_satisfied = true;
            }
        }
    }

    pub fn phase(&self) -> CodeEditPhase {
        self.phase
    }

    pub fn reset(&mut self) {
        self.phase = CodeEditPhase::Read;
        self.reset_read_gates();
        self.review_cycle = 0;
    }

    /// Dev agent build-mode sessions always track workflow phase (hints + subagent transitions).
    pub fn applies(agent_id: &str, mode: &str) -> bool {
        workflow_applies(agent_id, mode)
    }

    /// Returns `Some(error_message)` when the tool must be blocked.
    pub fn check_tool(&mut self, tool_name: &str, args: &Value) -> Option<String> {
        if !self.strict {
            return None;
        }
        let effective = resolve_effective_tool_name(tool_name, args);
        let effective_args = effective_tool_args(tool_name, args);
        if is_exempt_tool_call(&effective, &effective_args) {
            return None;
        }
        if is_unity_editor_workflow_tool(&effective) {
            return None;
        }
        if is_knowledge_or_skill_tool(&effective) {
            return None;
        }

        if is_write_tool(&effective) {
            return self.check_write_tool(&effective, &effective_args);
        }
        if effective == "task" {
            return self.check_task_tool(&effective_args);
        }
        if effective == "bash" {
            return self.check_bash_tool(&effective_args);
        }
        None
    }

    fn check_bash_tool(&self, args: &Value) -> Option<String> {
        match self.phase {
            CodeEditPhase::Read | CodeEditPhase::Plan => {
                if is_read_only_bash_args(args) {
                    return None;
                }
                if is_clearly_mutating_bash_args(args) {
                    return Some(self.block_message(
                        "bash mutating commands are blocked in READ/PLAN phase. Use read-only commands (e.g. grep, rg, git diff, git status, git log, cat, head, find) for exploration, or task(implementer) after plan confirmation.",
                    ));
                }
                // Ambiguous bash: not blocked here — runtime prompts the user before execution.
                return None;
            }
            CodeEditPhase::Complete => None,
            CodeEditPhase::Implement
            | CodeEditPhase::Optimize
            | CodeEditPhase::Review
            | CodeEditPhase::Idle => Some(
                self.block_message(
                    "bash is restricted during the code-edit workflow. Use task(implementer) or task(optimizer) for changes, or complete the review phase first.",
                ),
            ),
        }
    }

    /// Call after a tool executed successfully (not on block).
    /// Returns an optional hint to append to the tool result so the agent continues the pipeline.
    pub fn on_tool_success(
        &mut self,
        tool_name: &str,
        args: &Value,
        output: Option<&str>,
    ) -> Option<String> {
        let effective = resolve_effective_tool_name(tool_name, args);
        let effective_args = effective_tool_args(tool_name, args);
        self.maybe_begin_new_cycle_from_complete(&effective, &effective_args);

        if effective == "ask_user_question" {
            if let Some(answer) = output.and_then(extract_ask_user_answer) {
                self.reset_workflow_text_stop_nudges();
                if self.phase == CodeEditPhase::Plan {
                    return self.handle_plan_confirmation_answer(&answer, &effective_args);
                }
                if self.phase == CodeEditPhase::Read {
                    if !self.read_satisfied() {
                        let codegraph_note = if self.codegraph_satisfied {
                            " codegraph_gate is already true, but exploration_gate still requires read/grep/list on surfaced files (or use codegraph_context, which also satisfies exploration_gate). "
                        } else {
                            " "
                        };
                        return Some(format!(
                            "[Dev workflow] ask_user_question was answered in READ phase while read_gate=false — plan confirmation was NOT recorded (plan_confirmed remains false).{codegraph_note} \
                             Complete exploration_gate first; phase advances to PLAN automatically when read_gate=true. \
                             Then present the modification plan and call ask_user_question again with {PLAN_ASK_USER_OPTIONS}."
                        ));
                    }
                    self.maybe_advance_read_to_plan();
                    return self.handle_plan_confirmation_answer(&answer, &effective_args);
                }
            }
        }

        let was_satisfied = self.read_satisfied();
        if is_read_tool(&effective) {
            self.record_read_tool_success(&effective);
        }
        if effective == "task" {
            self.reset_workflow_text_stop_nudges();
            if let Some(sub) = effective_args.get("subagent_type").and_then(|v| v.as_str()) {
                if sub == "explorer" {
                    self.exploration_satisfied = true;
                }
            }
        }
        if effective == "tool_load" {
            if let Some(hint) = tool_load_codegraph_execute_hint(&effective_args, self) {
                return Some(hint);
            }
        }

        if self.phase == CodeEditPhase::Read {
            if !was_satisfied && self.read_satisfied() {
                self.maybe_advance_read_to_plan();
                return Some(read_complete_hint(self.review_cycle));
            }
            return read_partial_progress_hint(self);
        }

        None
    }

    fn handle_plan_confirmation_answer(&mut self, answer: &str, args: &Value) -> Option<String> {
        match parse_plan_confirmation_choice(answer) {
            PlanConfirmationChoice::Confirm => {
                self.plan_confirmed = true;
                if is_zero_change_plan_confirmation(answer, args) {
                    self.plan_zero_change = true;
                    self.phase = CodeEditPhase::Complete;
                    self.mark_completion_report_pending(None, true);
                    return Some(plan_zero_change_complete_hint());
                }
                Some(plan_confirmed_hint(self.review_cycle, self.codegraph_satisfied))
            }
            PlanConfirmationChoice::Cancel => {
                self.phase = CodeEditPhase::Read;
                self.reset_read_gates();
                Some(plan_cancel_to_read_hint())
            }
            PlanConfirmationChoice::Modify => {
                self.plan_confirmed = false;
                Some(plan_revise_hint(answer))
            }
        }
    }

    /// After PASS the gate stays in `Complete`. Further read/explore means a new edit cycle.
    fn maybe_begin_new_cycle_from_complete(&mut self, tool_name: &str, args: &Value) {
        if self.phase != CodeEditPhase::Complete {
            return;
        }
        let starts_read = is_read_tool(tool_name)
            || (tool_name == "task"
                && args
                    .get("subagent_type")
                    .and_then(|v| v.as_str())
                    == Some("explorer"));
        if starts_read {
            self.phase = CodeEditPhase::Read;
            self.reset_read_gates();
        }
    }

    /// Returns an optional message to append to the subagent tool output for the parent LLM.
    pub fn on_subagent_done(
        &mut self,
        subagent_type: &str,
        is_error: bool,
        output: Option<&str>,
    ) -> Option<String> {
        if is_error {
            return None;
        }
        self.reset_workflow_text_stop_nudges();
        match (self.phase, subagent_type) {
            (CodeEditPhase::Implement | CodeEditPhase::Complete, "implementer") => {
                self.phase = CodeEditPhase::Optimize;
                Some(implementer_complete_hint(self.review_cycle))
            }
            (CodeEditPhase::Optimize, "optimizer") => {
                self.phase = CodeEditPhase::Review;
                Some(optimizer_complete_hint(self.review_cycle))
            }
            (CodeEditPhase::Review, "reviewer") => {
                let verdict = output
                    .map(parse_review_verdict)
                    .unwrap_or(ReviewVerdict::Unknown);
                match verdict {
                    ReviewVerdict::Pass | ReviewVerdict::PassWithRisks => {
                        self.phase = CodeEditPhase::Complete;
                        self.mark_completion_report_pending(Some(verdict), false);
                        Some(workflow_complete_message(verdict))
                    }
                    ReviewVerdict::Block | ReviewVerdict::Unknown => {
                        self.review_cycle = self.review_cycle.saturating_add(1);
                        self.phase = CodeEditPhase::Read;
                        self.reset_read_gates();
                        Some(review_retry_message(self.review_cycle, verdict))
                    }
                }
            }
            _ => None,
        }
    }

    fn check_write_tool(&mut self, tool_name: &str, _args: &Value) -> Option<String> {
        self.maybe_advance_read_to_plan();
        match self.phase {
            CodeEditPhase::Complete => {
                self.phase = CodeEditPhase::Read;
                self.reset_read_gates();
                Some(self.block_message(&format!(
                    "Starting a new code-edit cycle. Phase is now READ. Explore the change scope with read/grep/list (and CodeGraph for complex edits) before '{}'.",
                    tool_name
                )))
            }
            CodeEditPhase::Read => {
                let detail = if self.read_satisfied() {
                    format!(
                        "Tool '{}' is blocked: READ is complete (read_gate=true). Write the modification plan in PLAN phase, then call ask_user_question for user confirmation before task(implementer).",
                        tool_name
                    )
                } else {
                    format!(
                        "Tool '{}' is blocked in READ phase. {}",
                        tool_name,
                        read_gate_requirement_detail(self)
                    )
                };
                Some(self.block_message(&detail))
            }
            CodeEditPhase::Plan => Some(self.block_message(&format!(
                "Tool '{}' is blocked in PLAN phase. Present the {PLAN_CONTENT_CHECKLIST} and call ask_user_question with options {PLAN_ASK_USER_OPTIONS} — do not edit until the user confirms.",
                tool_name
            ))),
            CodeEditPhase::Implement => Some(self.block_message(&format!(
                "Tool '{}' is blocked in IMPLEMENT phase. Dispatch changes with task(subagent_type=implementer) only.",
                tool_name
            ))),
            CodeEditPhase::Optimize => Some(self.block_message(&format!(
                "Tool '{}' is blocked in OPTIMIZE phase. Dispatch refinements with task(subagent_type=optimizer) only.",
                tool_name
            ))),
            CodeEditPhase::Review => {
                let detail = if self.review_cycle > 0 {
                    format!(
                        "Tool '{}' is blocked in REVIEW phase (retry cycle #{}). Dispatch task(subagent_type=reviewer) to obtain the review verdict — do not edit/write/bash on dev. If reviewer returns BLOCK, the workflow resets to READ → PLAN → task(implementer) → task(optimizer) → task(reviewer).",
                        tool_name, self.review_cycle
                    )
                } else {
                    format!(
                        "Tool '{}' is blocked in REVIEW phase. Dispatch task(subagent_type=reviewer) now — parent dev agent must not use edit/write/bash. After a BLOCK verdict, the workflow resets to READ → PLAN → task(implementer) → task(optimizer) → task(reviewer).",
                        tool_name
                    )
                };
                Some(self.block_message(&detail))
            }
            CodeEditPhase::Idle => {
                self.phase = CodeEditPhase::Read;
                Some(self.block_message(&format!(
                    "Tool '{}' requires the Read → Plan → Implement → Optimize → Review workflow. Phase is READ — explore before editing.",
                    tool_name
                )))
            }
        }
    }

    fn check_reviewer_dispatch(&self, _args: &Value) -> Option<String> {
        // Reviewer subagent performs its own read/grep/list (and CodeGraph when needed).
        // Parent dev agent does not need exploration_gate or codegraph_gate before dispatch.
        None
    }

    fn check_task_tool(&self, args: &Value) -> Option<String> {
        let subagent = args
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match self.phase {
            CodeEditPhase::Read => {
                if subagent == "explorer" {
                    return None;
                }
                if subagent == "implementer" {
                    if !self.read_satisfied() {
                        return Some(self.block_message(&format!(
                            "Cannot dispatch implementer yet: {READ_GATE_REQUIREMENT}",
                            READ_GATE_REQUIREMENT = read_gate_requirement_detail(self)
                        )));
                    }
                    return Some(self.block_message(&format!(
                        "Cannot dispatch implementer yet: READ is complete — enter PLAN phase first. \
                         Write the {PLAN_CONTENT_CHECKLIST}. {PLAN_FILE_CHANGE_DETAIL} \
                         Then call ask_user_question with options {PLAN_ASK_USER_OPTIONS} and wait for user confirmation.",
                    )));
                }
                if subagent == "reviewer" {
                    return self.check_reviewer_dispatch(args);
                }
                None
            }
            CodeEditPhase::Plan => {
                if subagent == "explorer" {
                    return None;
                }
                if subagent == "implementer" {
                    if !self.plan_confirmed {
                        return Some(self.block_message(&format!(
                            "Cannot dispatch implementer: plan not confirmed. \
                             Present the {PLAN_CONTENT_CHECKLIST}. {PLAN_FILE_CHANGE_DETAIL} \
                             Then call ask_user_question with options {PLAN_ASK_USER_OPTIONS} and wait for the user.",
                        )));
                    }
                    if self.plan_zero_change {
                        return Some(self.block_message(
                            "Cannot dispatch implementer: the confirmed plan is zero-change (no files to modify). \
                             The workflow cycle is complete — summarize the decision for the user. \
                             Do NOT dispatch implementer, optimizer, or reviewer placeholder tasks.",
                        ));
                    }
                    if let Some(prompt) = args.get("prompt").and_then(|v| v.as_str()) {
                        if is_complex_code_edit_prompt(prompt) && !self.codegraph_satisfied {
                            return Some(self.block_message(
                                "Cannot dispatch implementer for a complex edit: codegraph_gate=false. \
                                 Run CodeGraph first — `codegraph_context` (preferred) or `codegraph_impact` / \
                                 `codegraph_trace` / `codegraph_callers` / `codegraph_callees` on the change scope. \
                                 Simple 1–2 file fixes do not require CodeGraph. \
                                 CodeGraph can be run now in PLAN phase (invoke analysis tools directly — not `tool_load`). \
                                 After codegraph_gate=true, retry task(implementer).",
                            ));
                        }
                    }
                    return None;
                }
                if subagent == "reviewer" {
                    if self.plan_confirmed {
                        return Some(self.block_message(
                            "Plan is confirmed — dispatch task(subagent_type=implementer) next. \
                             task(reviewer) runs after task(optimizer) in the full edit cycle.",
                        ));
                    }
                    return self.check_reviewer_dispatch(args);
                }
                None
            }
            CodeEditPhase::Implement => {
                if subagent == "implementer" {
                    if self.read_satisfied() {
                        return None;
                    }
                    return Some(self.block_message(&format!(
                        "Cannot dispatch implementer: {READ_GATE_REQUIREMENT}",
                        READ_GATE_REQUIREMENT = read_gate_requirement_detail(self)
                    )));
                }
                Some(self.block_message(&format!(
                    "In IMPLEMENT phase only task(subagent_type=implementer) is allowed (got '{}').",
                    subagent
                )))
            }
            CodeEditPhase::Optimize => {
                if subagent == "optimizer" {
                    return None;
                }
                if subagent == "implementer" {
                    return Some(self.block_message(
                        "In OPTIMIZE phase use task(optimizer) first — refine implementer output for efficiency and runtime depth before review.",
                    ));
                }
                if subagent == "reviewer" {
                    return Some(self.block_message(
                        "In OPTIMIZE phase use task(optimizer) before task(reviewer). Optimizer refines the implementation; reviewer validates afterward.",
                    ));
                }
                Some(self.block_message(&format!(
                    "In OPTIMIZE phase only task(subagent_type=optimizer) is allowed (got '{}').",
                    subagent
                )))
            }
            CodeEditPhase::Review => {
                if subagent == "reviewer" {
                    return None;
                }
                if subagent == "implementer" || subagent == "optimizer" {
                    return Some(self.block_message(
                        "In REVIEW phase use task(reviewer) first. If review fails, the workflow will reset to READ for a full Read → Plan → Implement → Optimize → Review loop.",
                    ));
                }
                Some(self.block_message(&format!(
                    "In REVIEW phase only task(subagent_type=reviewer) is allowed (got '{}').",
                    subagent
                )))
            }
            CodeEditPhase::Complete => {
                if subagent == "implementer" {
                    if self.plan_zero_change {
                        return Some(self.block_message(
                            "Cannot dispatch implementer: the confirmed plan was zero-change. \
                             The workflow cycle is complete — summarize for the user.",
                        ));
                    }
                    if !self.read_satisfied() {
                        return Some(self.block_message(&format!(
                            "Previous review cycle completed with PASS. Start a new cycle: {READ_GATE_REQUIREMENT} Then PLAN → task(implementer) → task(optimizer) → task(reviewer).",
                            READ_GATE_REQUIREMENT = read_gate_requirement_detail(self)
                        )));
                    }
                    return Some(self.block_message(
                        "Cannot dispatch implementer from COMPLETE: enter PLAN phase first. \
                         Write the modification plan and call ask_user_question for user confirmation.",
                    ));
                }
                if subagent == "reviewer" {
                    return Some(self.block_message(
                        "Cannot dispatch reviewer after PASS without a new implementer pass. Run read/explore, PLAN, task(implementer), task(optimizer), then task(reviewer).",
                    ));
                }
                if subagent == "optimizer" {
                    return Some(self.block_message(
                        "Cannot dispatch optimizer after PASS without a new implementer pass. Run read/explore, PLAN, task(implementer), task(optimizer), then task(reviewer).",
                    ));
                }
                None
            }
            CodeEditPhase::Idle => None,
        }
    }

    fn block_message(&self, detail: &str) -> String {
        let enforcement = if self.strict {
            "Runtime enforcement is ON."
        } else {
            "Runtime enforcement is OFF (LOCUS_DEV_WORKFLOW_STRICT=0); follow the workflow via rules and reminders."
        };
        format!(
            "[Dev workflow gate] Current phase: {}. read_gate={}. plan_confirmed={}. codegraph_gate={}. exploration_gate={}. review_cycle={}. {}\n\nRequired flow: READ (exploration; CodeGraph mandatory for complex edits) → PLAN (write plan + ask_user_question: {PLAN_ASK_USER_OPTIONS}) → task(implementer) → task(optimizer) → task(reviewer), loop until reviewer returns PASS or PASS_WITH_RISKS. Read-only review: task(reviewer) from READ or PLAN (before plan confirmation) — reviewer explores with read/grep/list itself; parent dev exploration_gate not required. {SOURCE_CODE_DISCIPLINE} {enforcement} Set LOCUS_DEV_WORKFLOW_STRICT=0 to disable blocking.",
            self.phase.label(),
            self.read_satisfied(),
            self.plan_confirmed,
            self.codegraph_satisfied,
            self.exploration_satisfied,
            self.review_cycle,
            detail
        )
    }

    /// Injected each agent-loop iteration so the model sees current phase and next step.
    pub fn status_reminder(&self) -> String {
        let blocked = if self.strict && !self.hidden_request_tools().is_empty() {
            format!(
                "Blocked tools (not in API list): {}. ",
                self.hidden_request_tools().join(", ")
            )
        } else {
            String::new()
        };
        let next = match (self.phase, self.read_satisfied(), self.plan_confirmed) {
            (CodeEditPhase::Read, false, _) if !self.exploration_satisfied && self.review_cycle > 0 => {
                "Next: review retry cycle — read/grep/list affected files to complete exploration_gate; for complex edits also run CodeGraph. Then PLAN + ask_user_question before task(implementer). Do NOT skip to implementer."
            }
            (CodeEditPhase::Read, false, _) if !self.exploration_satisfied => {
                "Next: read/grep/list the target file(s) to complete exploration_gate before PLAN or task(implementer). For read-only review, dispatch task(subagent_type=reviewer) directly — reviewer will explore with read/grep/list. For complex edits, also run CodeGraph (codegraph_context / impact / trace / callers / callees). Tools that cannot be classified as read-only vs edit will prompt you for approval before running."
            }
            (CodeEditPhase::Read, false, _) => {
                "Next: complete exploration_gate with read/grep/list on the change scope."
            }
            (CodeEditPhase::Read, true, _) if !self.codegraph_satisfied => {
                "Next: run CodeGraph for complex/multi-file/refactor work before PLAN. For simple edits, proceed to PLAN after read_gate. Do NOT use edit/write on dev."
            }
            (CodeEditPhase::Read, true, _) => {
                "Next: enter PLAN — write modification plan; task(reviewer) for read-only review (no edits). Do NOT use edit/write on dev."
            }
            (CodeEditPhase::Plan, _, false) => {
                "Next: write the modification plan and call ask_user_question (确认执行 / 取消 / 修改), or dispatch task(subagent_type=reviewer) for read-only review without edits. After plan confirmation, use task(implementer) → task(optimizer) → task(reviewer)."
            }
            (CodeEditPhase::Plan, _, true) if self.plan_zero_change => {
                "Next: zero-change plan confirmed — workflow complete. Summarize for the user; do NOT dispatch implementer/optimizer/reviewer."
            }
            (CodeEditPhase::Plan, _, true) if !self.codegraph_satisfied => {
                "Next: run CodeGraph (codegraph_context preferred) to satisfy codegraph_gate, then task(subagent_type=implementer). CodeGraph can be run in PLAN phase. Do NOT use edit/write on dev."
            }
            (CodeEditPhase::Plan, _, true) => {
                "Next: task(subagent_type=implementer) to apply the confirmed plan. Do NOT use edit/write on dev."
            }
            (CodeEditPhase::Implement, _, _) => {
                "Next: task(subagent_type=implementer) only (parent must not edit directly)."
            }
            (CodeEditPhase::Optimize, _, _) => {
                "Next: task(subagent_type=optimizer) only. Refine implementer output for efficiency, concision, and runtime depth before review."
            }
            (CodeEditPhase::Review, _, _) => {
                "Next: task(subagent_type=reviewer) only. Do NOT use edit/write/bash on dev parent — implementer and optimizer already completed."
            }
            (CodeEditPhase::Complete, _, _) => {
                "Cycle complete. For more application code: READ → PLAN → task(implementer) → task(optimizer) → task(reviewer)."
            }
            (CodeEditPhase::Idle, _, _) => {
                "Next: READ → PLAN → task(implementer) → task(optimizer) → task(reviewer) for substantive code changes."
            }
        };
        format!(
            "[Dev workflow] Phase: {}. read_gate={}. plan_confirmed={}. codegraph_gate={}. exploration_gate={}. review_cycle={}. {blocked}{next} {SOURCE_CODE_DISCIPLINE}",
            self.phase.label(),
            self.read_satisfied(),
            self.plan_confirmed,
            self.codegraph_satisfied,
            self.exploration_satisfied,
            self.review_cycle,
        )
    }
}

pub fn workflow_applies(agent_id: &str, mode: &str) -> bool {
    agent_id == AGENT_DEV_ID && mode == "build"
}

pub fn parse_review_verdict(output: &str) -> ReviewVerdict {
    let normalized: String = output
        .chars()
        .filter(|c| !matches!(*c, '*' | '#' | '`' | '|' | '[' | ']' | '"' | '\''))
        .map(|c| {
            if c.is_whitespace() || c == '-' || c == '_' {
                ' '
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    let joined = tokens.join(" ");

    if output.contains("未通过") || output.contains("不通过") || output.contains("阻塞") {
        return ReviewVerdict::Block;
    }
    if joined.contains("BLOCK")
        || joined.contains("NOT PASS")
        || joined.contains("FAIL")
    {
        return ReviewVerdict::Block;
    }
    if joined.contains("PASS WITH RISKS") || joined.contains("PASS WITH RISK") {
        return ReviewVerdict::PassWithRisks;
    }
    if joined.contains(" PASS")
        || joined.starts_with("PASS")
        || output.contains("审查通过")
        || output.contains("总体通过")
    {
        return ReviewVerdict::Pass;
    }
    ReviewVerdict::Unknown
}

fn read_gate_requirement_detail(gate: &WorkflowGate) -> String {
    let base = if !gate.exploration_satisfied {
        if gate.codegraph_satisfied {
            "Complete exploration_gate: read/grep/list the target file(s) surfaced by CodeGraph (codegraph_context also satisfies exploration_gate in one step). \
             ask_user_question answered in READ phase does NOT record plan confirmation — wait until PLAN phase."
        } else {
            "Complete exploration_gate: read/grep/list the target file(s). For complex edits, also run CodeGraph (codegraph_context / impact / trace / callers / callees)."
        }
    } else {
        "exploration_gate satisfied. For complex edits, run CodeGraph before PLAN and task(implementer)."
    };
    if gate.review_cycle > 0 {
        format!(
            "{base} Review BLOCK reset the workflow (retry cycle #{}). \
             Full READ → PLAN → ask_user_question confirmation → task(implementer) is required — cannot skip directly to implementer.",
            gate.review_cycle
        )
    } else {
        base.to_string()
    }
}

/// Heuristic: complex code edits require CodeGraph before implement/review dispatch.
fn is_complex_code_edit_prompt(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    const KEYWORDS: &[&str] = &[
        "refactor",
        "architecture",
        "architectural",
        "multi-file",
        "multi file",
        "multiple files",
        "cross-file",
        "cross file",
        "cross-module",
        "cross module",
        "重构",
        "架构",
        "多个文件",
        "跨文件",
        "跨模块",
        "新功能",
        "new feature",
    ];
    if KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return true;
    }
    let exts = [".rs", ".ts", ".tsx", ".vue", ".js", ".jsx", ".cs"];
    let path_mentions = exts
        .iter()
        .map(|ext| lower.matches(ext).count())
        .sum::<usize>();
    path_mentions >= 3
}

fn tool_load_requested_names(args: &Value) -> Vec<String> {
    args.get("tools")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn tool_load_codegraph_execute_hint(args: &Value, gate: &WorkflowGate) -> Option<String> {
    if gate.phase != CodeEditPhase::Read || gate.codegraph_satisfied {
        return None;
    }
    let requests_codegraph = tool_load_requested_names(args)
        .iter()
        .any(|name| is_codegraph_analysis_tool(name) || name.starts_with("codegraph_"));
    if !requests_codegraph {
        return None;
    }
    Some(
        "[Dev workflow] tool_load does NOT execute CodeGraph and does NOT satisfy codegraph_gate. \
         In the same turn, call `codegraph_context` directly (preferred) with a non-empty `task`, \
         or `tool_call` with toolName \"codegraph_context\" / \"codegraph_impact\" and the required arguments."
            .to_string(),
    )
}

fn read_partial_progress_hint(gate: &WorkflowGate) -> Option<String> {
    if gate.read_satisfied() {
        return None;
    }
    if gate.codegraph_satisfied && !gate.exploration_satisfied {
        return Some(
            "[Dev workflow] READ partial: codegraph_gate=true, exploration_gate=false. \
             Next: read/grep/list the surfaced files (codegraph_context also satisfies exploration_gate). \
             ask_user_question in READ phase does NOT record plan confirmation — wait for PLAN phase."
                .to_string(),
        );
    }
    if !gate.exploration_satisfied {
        return Some(
            "[Dev workflow] READ started: read/grep/list the target file(s) to satisfy exploration_gate. \
             For complex edits, also run CodeGraph (codegraph_context / impact / trace / callers / callees)."
                .to_string(),
        );
    }
    None
}

fn read_complete_hint(review_cycle: u32) -> String {
    if review_cycle > 0 {
        format!(
            "[Dev workflow] READ gate satisfied (retry cycle #{review_cycle}). Next required step: enter PLAN — write the {PLAN_CONTENT_CHECKLIST} ({PLAN_FILE_CHANGE_DETAIL}) incorporating review feedback, then call ask_user_question ({PLAN_ASK_USER_OPTIONS}) and wait for user confirmation before task(implementer). {SOURCE_CODE_DISCIPLINE}"
        )
    } else {
        format!(
            "[Dev workflow] READ gate satisfied. Next: enter PLAN — write the {PLAN_CONTENT_CHECKLIST} ({PLAN_FILE_CHANGE_DETAIL}), call ask_user_question with options {PLAN_ASK_USER_OPTIONS}, and wait for user confirmation. For read-only review (no edits), dispatch task(subagent_type=\"reviewer\") from PLAN before plan confirmation. {SOURCE_CODE_DISCIPLINE}"
        )
    }
}

fn plan_confirmed_hint(review_cycle: u32, codegraph_satisfied: bool) -> String {
    if !codegraph_satisfied {
        if review_cycle > 0 {
            return format!(
                "[Dev workflow] User confirmed the plan (retry cycle #{review_cycle}). \
                 Complex edits require CodeGraph first — run `codegraph_context` or `codegraph_impact` in PLAN phase to satisfy codegraph_gate, then dispatch task(subagent_type=\"implementer\"). {SOURCE_CODE_DISCIPLINE}"
            );
        }
        return format!(
            "[Dev workflow] User confirmed the plan. \
             Complex edits require CodeGraph first — run `codegraph_context` or `codegraph_impact` in PLAN phase to satisfy codegraph_gate, then dispatch task(subagent_type=\"implementer\"). {SOURCE_CODE_DISCIPLINE}"
        );
    }
    if review_cycle > 0 {
        format!(
            "[Dev workflow] User confirmed the plan (retry cycle #{review_cycle}). Next required step: dispatch task(subagent_type=\"implementer\") with the confirmed plan. {SOURCE_CODE_DISCIPLINE}"
        )
    } else {
        format!(
            "[Dev workflow] User confirmed the plan. Next required step: dispatch task(subagent_type=\"implementer\") with the confirmed plan and analysis above. {SOURCE_CODE_DISCIPLINE}"
        )
    }
}

fn plan_cancel_to_read_hint() -> String {
    "[Dev workflow] User chose 取消 — phase reset to READ. Re-explore the change scope (read/grep/list and CodeGraph if complex), then write a new PLAN.".to_string()
}

fn plan_revise_hint(feedback: &str) -> String {
    format!(
        "[Dev workflow] User requested plan changes (修改): {feedback}\n\
         Revise the {PLAN_CONTENT_CHECKLIST}. {PLAN_FILE_CHANGE_DETAIL} \
         Call ask_user_question again with {PLAN_ASK_USER_OPTIONS}."
    )
}

fn extract_ask_user_answer(output: &str) -> Option<String> {
    const PREFIX: &str = "User answered: ";
    output
        .strip_prefix(PREFIX)
        .map(str::trim)
        .filter(|answer| !answer.is_empty())
        .map(str::to_string)
}

fn parse_plan_confirmation_choice(answer: &str) -> PlanConfirmationChoice {
    let normalized: String = answer
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '-' || c == '_' {
                ' '
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect();
    let joined = normalized.split_whitespace().collect::<Vec<_>>().join(" ");

    if answer.contains("取消")
        || joined.contains("CANCEL")
        || joined.contains("ABORT")
        || answer.contains("返回")
        || joined.contains("RETURN")
        || joined.contains("GO BACK")
        || joined == "BACK"
    {
        return PlanConfirmationChoice::Cancel;
    }
    if answer.contains("确认")
        || joined.contains("CONFIRM")
        || joined.contains("CONFIRM EXECUTION")
        || joined.contains("PROCEED")
        || joined == "YES"
    {
        return PlanConfirmationChoice::Confirm;
    }
    if answer.contains("修改")
        || joined.contains("MODIFY")
        || joined.contains("REVISE")
        || joined.contains("CHANGE PLAN")
    {
        return PlanConfirmationChoice::Modify;
    }
    // Custom text from the 修改 option counts as revision feedback.
    PlanConfirmationChoice::Modify
}

/// Detects a confirmed plan that intentionally changes no files (keep status quo).
fn is_zero_change_plan_confirmation(answer: &str, args: &Value) -> bool {
    if looks_like_zero_change_text(answer) {
        return true;
    }
    let Some(selected_label) = extract_ask_user_selected_label(answer) else {
        return false;
    };
    let Some(options) = args.get("options").and_then(|v| v.as_array()) else {
        return false;
    };
    for opt in options {
        let label = opt.get("label").and_then(|v| v.as_str()).unwrap_or("");
        if !labels_match_for_ask_user(label, &selected_label) {
            continue;
        }
        let description = opt
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if looks_like_zero_change_text(description) {
            return true;
        }
    }
    false
}

fn extract_ask_user_selected_label(answer: &str) -> Option<String> {
    let rest = answer.strip_prefix("User answered: ").unwrap_or(answer).trim();
    if rest.is_empty() {
        return None;
    }
    let label = rest
        .split(['—', '–', '-', ':', '：'])
        .next()
        .unwrap_or(rest)
        .trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

fn labels_match_for_ask_user(option_label: &str, selected_label: &str) -> bool {
    let a = option_label.trim();
    let b = selected_label.trim();
    a == b || a.contains(b) || b.contains(a)
}

fn looks_like_zero_change_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.contains("零修改")
        || text.contains("无修改")
        || text.contains("不修改")
        || text.contains("保持现状")
        || text.contains("不触碰")
        || text.contains("无实际修改")
        || text.contains("无任何代码修改")
        || text.contains("无代码修改")
        || text.contains("不修改任何")
        || text.contains("修改文件") && (text.contains(":无") || text.contains("：无"))
        || text.contains("无文件") && text.contains("修改")
        || lower.contains("no code change")
        || lower.contains("no file change")
        || lower.contains("zero modification")
        || lower.contains("keep as-is")
        || lower.contains("keep status quo")
}

fn plan_zero_change_complete_hint() -> String {
    "[Dev workflow] User confirmed a zero-change plan (keep status quo). \
     This cycle is complete — do NOT dispatch task(implementer), task(optimizer), or task(reviewer). \
     A structured completion report is being generated automatically for the user — do NOT summarize or repeat it; end the turn.".to_string()
}

fn implementer_complete_hint(review_cycle: u32) -> String {
    if review_cycle > 0 {
        format!(
            "[Dev workflow] Implementer finished (retry cycle #{review_cycle}). Next required step: dispatch task(subagent_type=\"optimizer\") with the implementer change summary above — refine for efficiency, concision, and runtime depth before review. Parent dev agent must NOT edit/write/bash; only task(optimizer) is allowed in OPTIMIZE phase. {SOURCE_CODE_DISCIPLINE}"
        )
    } else {
        format!(
            "[Dev workflow] Implementer finished. Next required step: dispatch task(subagent_type=\"optimizer\") with the implementer output above — refine for efficiency, concision, and runtime depth before review. Parent dev agent must NOT edit/write/bash; only task(optimizer) is allowed in OPTIMIZE phase. {SOURCE_CODE_DISCIPLINE}"
        )
    }
}

fn optimizer_complete_hint(review_cycle: u32) -> String {
    if review_cycle > 0 {
        format!(
            "[Dev workflow] Optimizer finished (retry cycle #{review_cycle}). Next required step: dispatch task(subagent_type=\"reviewer\") with the optimizer change summary above — do not stop or reply to the user until review returns PASS or PASS_WITH_RISKS. Parent dev agent must NOT edit/write/bash; only task(reviewer) is allowed in REVIEW phase. {SOURCE_CODE_DISCIPLINE}"
        )
    } else {
        format!(
            "[Dev workflow] Optimizer finished. Next required step: dispatch task(subagent_type=\"reviewer\") with the optimizer output above — do not stop or reply to the user until review returns PASS or PASS_WITH_RISKS. Parent dev agent must NOT edit/write/bash; only task(reviewer) is allowed in REVIEW phase. {SOURCE_CODE_DISCIPLINE}"
        )
    }
}

fn workflow_complete_message(verdict: ReviewVerdict) -> String {
    let label = match verdict {
        ReviewVerdict::PassWithRisks => "PASS_WITH_RISKS",
        _ => "PASS",
    };
    format!(
        "[Dev workflow] Reviewer returned {label}. This Read → Plan → Implement → Optimize → Review cycle is complete.\n\
         A structured completion report is being generated automatically for the user — do NOT summarize or repeat it; end the turn.\n\
         Do not run another reviewer unless there are new code changes.\n\
         If more application code must change: read/grep/list AND CodeGraph (codegraph_context + impact on changed symbols), then \
         PLAN → task(implementer) → task(optimizer) → task(reviewer) (a new cycle starts automatically after read/explore)."
    )
}

fn review_retry_message(cycle: u32, verdict: ReviewVerdict) -> String {
    let reason = match verdict {
        ReviewVerdict::Block => "Review returned BLOCK",
        ReviewVerdict::Unknown => "Review did not report a clear PASS / PASS_WITH_RISKS verdict",
        _ => "Review not accepted",
    };
    format!(
        "[Dev workflow] {reason}. Starting review retry cycle #{cycle}: phase reset to READ.\n\
         Next steps (mandatory loop):\n\
         1. READ — re-read affected files AND re-run CodeGraph (codegraph_context + codegraph_impact on changed symbols); reason about runtime edge cases\n\
         2. PLAN — write {PLAN_CONTENT_CHECKLIST} + ask_user_question ({PLAN_ASK_USER_OPTIONS}); wait for user confirmation\n\
         3. IMPLEMENT — task(subagent_type=implementer) with fixes from review feedback; minimal cautious edits only\n\
         4. OPTIMIZE — task(subagent_type=optimizer) to refine efficiency, concision, and runtime behavior\n\
         5. REVIEW — task(subagent_type=reviewer) again until PASS or PASS_WITH_RISKS\n\
         {SOURCE_CODE_DISCIPLINE}"
    )
}

pub fn dev_workflow_strict_enabled() -> bool {
    match std::env::var("LOCUS_DEV_WORKFLOW_STRICT").as_deref() {
        Ok("0") | Ok("false") | Ok("off") => false,
        _ => true,
    }
}

/// Resolve the inner tool name when `tool_name` is a meta `tool_call`.
pub fn resolve_effective_tool_name(tool_name: &str, args: &Value) -> String {
    if tool_name != "tool_call" {
        return tool_name.to_string();
    }
    args.get("toolName")
        .or_else(|| args.get("tool_name"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| tool_name.to_string())
}

pub(crate) fn effective_tool_args(tool_name: &str, args: &Value) -> Value {
    if tool_name == "tool_call" {
        args.get("arguments")
            .cloned()
            .filter(|v| v.is_object())
            .unwrap_or_else(|| Value::Object(Default::default()))
    } else {
        args.clone()
    }
}

fn is_exempt_tool_call(tool_name: &str, args: &Value) -> bool {
    if tool_name == "tool_load" {
        return true;
    }
    if matches!(tool_name, "write" | "edit") {
        if let Some(path) = tool_path_from_args(args) {
            if is_knowledge_markdown_path(&path) {
                return true;
            }
        }
    }
    false
}

fn tool_path_from_args(args: &Value) -> Option<String> {
    for key in ["path", "file_path", "file", "target"] {
        if let Some(p) = args.get(key).and_then(|v| v.as_str()) {
            if !p.trim().is_empty() {
                return Some(p.replace('\\', "/"));
            }
        }
    }
    None
}

fn is_knowledge_markdown_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    (lower.contains("locus/knowledge/") || lower.starts_with("knowledge/"))
        && lower.ends_with(".md")
}

pub fn is_exploration_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read"
            | "grep"
            | "list"
            | "web_fetch"
            | "unity_ref_search"
            | "unity_asset_search"
            | "unity_yaml_list"
            | "unity_yaml_search"
            | "unity_yaml_read"
    )
}

/// Knowledge and Skill tools are outside the code-edit workflow (never blocked or hidden).
pub fn is_knowledge_or_skill_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "knowledge_list"
            | "knowledge_query"
            | "knowledge_read"
            | "knowledge_create"
            | "knowledge_delete"
            | "knowledge_move"
            | "knowledge_edit"
            | "skill_create"
            | "skill_reload"
            | "skill_list"
    )
}

/// CodeGraph tools that satisfy the mandatory relationship-analysis gate (not index maintenance).
pub fn is_codegraph_analysis_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "codegraph_search"
            | "codegraph_context"
            | "codegraph_callers"
            | "codegraph_callees"
            | "codegraph_impact"
            | "codegraph_files"
            | "codegraph_trace"
    )
}

pub fn is_codegraph_maintenance_tool(tool_name: &str) -> bool {
    matches!(tool_name, "codegraph_status" | "codegraph_sync")
}

pub fn is_read_tool(tool_name: &str) -> bool {
    is_exploration_tool(tool_name)
        || is_codegraph_analysis_tool(tool_name)
        || is_codegraph_maintenance_tool(tool_name)
}

/// Application-source edits gated by Read → Plan → Implement → Optimize → Review.
/// Unity Editor runtime tools (`unity_execute`, etc.) are intentionally excluded:
/// Dev rules require them for scene/asset work and Play Mode inspection alongside read tools.
pub fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write"
            | "edit"
    )
}

/// Unity Editor tools allowed during the dev workflow (not subject to `check_write_tool`).
pub fn is_unity_editor_workflow_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "unity_execute"
            | "unity_recompile"
            | "unity_run_states"
            | "unity_capture_viewport"
    )
}

/// Whether a tool is clearly read-only, clearly write/edit, or ambiguous for the workflow gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolWorkflowKind {
    ReadOnly,
    Write,
    Ambiguous,
}

/// Classify a tool call for READ/PLAN workflow (after resolving `tool_call` targets).
pub fn classify_tool_workflow_kind(tool_name: &str, args: &Value) -> ToolWorkflowKind {
    let effective = resolve_effective_tool_name(tool_name, args);
    let effective_args = effective_tool_args(tool_name, args);

    if is_exempt_tool_call(&effective, &effective_args)
        || is_unity_editor_workflow_tool(&effective)
        || is_knowledge_or_skill_tool(&effective)
    {
        return ToolWorkflowKind::ReadOnly;
    }
    if matches!(
        effective.as_str(),
        "ask_user_question" | "tool_load" | "tool_call" | "todowrite" | "graph_view" | "config_query"
    ) {
        return ToolWorkflowKind::ReadOnly;
    }
    if is_write_tool(&effective) {
        return ToolWorkflowKind::Write;
    }
    if effective == "bash" {
        if is_read_only_bash_args(&effective_args) {
            return ToolWorkflowKind::ReadOnly;
        }
        if is_clearly_mutating_bash_args(&effective_args) {
            return ToolWorkflowKind::Write;
        }
        return ToolWorkflowKind::Ambiguous;
    }
    if effective == "task" {
        return ToolWorkflowKind::ReadOnly;
    }
    if is_read_tool(&effective) {
        return ToolWorkflowKind::ReadOnly;
    }
    ToolWorkflowKind::Ambiguous
}

/// Normalize a bash command for session whitelist matching (whitespace-collapsed, trimmed).
pub fn normalize_bash_whitelist_key(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Short prefix stored when whitelisting common read-only search/git/cargo bash invocations.
pub fn extract_bash_whitelist_prefix(command: &str) -> Option<String> {
    let segment = command.split('|').next()?.trim();
    if segment.is_empty() {
        return None;
    }
    let tokens = split_shell_words(segment);
    if tokens.is_empty() {
        return None;
    }
    let cmd = tokens[0].to_ascii_lowercase();

    const FLAG_RUN_PREFIX: &[&str] = &[
        "grep", "egrep", "fgrep", "zgrep", "bzgrep", "rg", "ripgrep", "ag", "ack", "findstr",
    ];
    if FLAG_RUN_PREFIX.contains(&cmd.as_str()) {
        let mut end = 1usize;
        while end < tokens.len() && tokens[end].starts_with('-') {
            end += 1;
        }
        if end > 1 {
            return Some(tokens[..end].join(" "));
        }
        return Some(cmd);
    }

    if is_git_executable(&cmd) {
        if tokens.len() >= 2 {
            let mut end = 2usize;
            while end < tokens.len() && tokens[end].starts_with('-') {
                end += 1;
            }
            return Some(tokens[..end].join(" "));
        }
    }

    if cmd == "cargo" && tokens.len() >= 2 && !tokens[1].starts_with('-') {
        return Some(format!("{} {}", tokens[0], tokens[1]));
    }

    None
}

/// Key written to the persisted bash whitelist (prefix when extractable, else full command).
pub fn bash_whitelist_storage_key(command: &str) -> String {
    extract_bash_whitelist_prefix(command)
        .map(|prefix| normalize_bash_whitelist_key(&prefix))
        .unwrap_or_else(|| normalize_bash_whitelist_key(command))
}

/// Whether a bash command matches a whitelist entry (exact or prefix: entry + space + more).
pub fn bash_command_matches_whitelist_entry(command: &str, entry: &str) -> bool {
    let cmd = normalize_bash_whitelist_key(command);
    let key = normalize_bash_whitelist_key(entry);
    if cmd == key {
        return true;
    }
    if cmd.len() <= key.len() {
        return false;
    }
    cmd.starts_with(&key) && cmd.as_bytes().get(key.len()) == Some(&b' ')
}

/// READ/PLAN: ambiguous tools (cannot classify as read vs edit) require user approval before run.
pub fn workflow_ambiguous_tool_requires_user_confirm(
    gate: &WorkflowGate,
    tool_name: &str,
    args: &Value,
    persisted_whitelist: &WorkflowAmbiguousWhitelist,
) -> bool {
    if !gate.strict {
        return false;
    }
    if !matches!(gate.phase, CodeEditPhase::Read | CodeEditPhase::Plan) {
        return false;
    }
    if persisted_whitelist.is_whitelisted(tool_name, args) {
        return false;
    }
    classify_tool_workflow_kind(tool_name, args) == ToolWorkflowKind::Ambiguous
}

/// READ/PLAN: whitelisted bash/tools skip workflow ambiguous and per-tool permission confirms.
pub fn workflow_read_plan_whitelist_skips_tool_confirm(
    gate: &WorkflowGate,
    tool_name: &str,
    args: &Value,
    persisted_whitelist: &WorkflowAmbiguousWhitelist,
) -> bool {
    if !gate.strict {
        return false;
    }
    if !matches!(gate.phase, CodeEditPhase::Read | CodeEditPhase::Plan) {
        return false;
    }
    let effective = resolve_effective_tool_name(tool_name, args);
    let effective_args = effective_tool_args(tool_name, args);
    if effective == "bash" && bash_rm_requires_user_confirm(&effective_args) {
        return false;
    }
    persisted_whitelist.is_whitelisted(tool_name, args)
}

/// Whether the tool confirm card may offer "add to READ/PLAN whitelist".
pub fn workflow_read_plan_whitelist_offerable(
    gate: &WorkflowGate,
    tool_name: &str,
    args: &Value,
    workflow_ambiguous_requires_confirm: bool,
) -> bool {
    if !gate.strict {
        return false;
    }
    if !matches!(gate.phase, CodeEditPhase::Read | CodeEditPhase::Plan) {
        return false;
    }
    if tool_name == "bash" && bash_rm_requires_user_confirm(args) {
        return false;
    }
    workflow_ambiguous_requires_confirm || tool_name == "bash"
}

pub const WORKFLOW_AMBIGUOUS_TOOL_CONFIRM_NOTE: &str = "READ/PLAN 阶段：无法判定该工具是只读探索还是修改代码。请确认是否执行。";

pub const BASH_RM_CONFIRM_NOTE: &str =
    "该 bash 命令包含删除操作（rm / rmdir / del / Remove-Item 等）。执行前请二次确认，避免误删文件或目录。";

/// Whether a bash command deletes files/directories and must be confirmed by the user.
pub fn bash_rm_requires_user_confirm(args: &Value) -> bool {
    let Some(command) = args.get("command").and_then(|v| v.as_str()) else {
        return false;
    };
    is_rm_bash_command(command)
}

pub fn is_rm_bash_command(command: &str) -> bool {
    if command.contains("$(") || command.contains('`') {
        return true;
    }
    for segment in iter_bash_atomic_command_segments(command) {
        let tokens = split_shell_words(&segment);
        if shell_tokens_are_rm_destructive(&tokens) {
            return true;
        }
    }
    false
}

fn iter_bash_atomic_command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    for pipe_part in command.split('|') {
        for and_part in pipe_part.split("&&") {
            for or_part in and_part.split("||") {
                for semi_part in or_part.split([';', '\n', '\r']) {
                    let trimmed = semi_part.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    segments
}

fn shell_tokens_are_rm_destructive(tokens: &[String]) -> bool {
    let Some(first) = tokens.first().map(|value| value.to_ascii_lowercase()) else {
        return false;
    };
    const RM_COMMANDS: &[&str] = &["rm", "rmdir", "del", "erase", "remove-item", "ri"];
    if RM_COMMANDS.contains(&first.as_str()) {
        return true;
    }
    if first == "rtk" {
        return tokens
            .get(1)
            .map(|value| value.to_ascii_lowercase())
            .map(|value| RM_COMMANDS.contains(&value.as_str()))
            .unwrap_or(false);
    }
    false
}

/// Whether a bash invocation is read-only (allowed during READ phase).
pub fn is_read_only_bash_args(args: &Value) -> bool {
    let Some(command) = args.get("command").and_then(|v| v.as_str()) else {
        return false;
    };
    is_read_only_bash_command(command)
}

/// Read-only shell commands permitted during READ/PLAN (git inspection, grep, cat, etc.).
pub fn is_read_only_bash_command(command: &str) -> bool {
    if command.contains("$(") || command.contains('`') {
        return false;
    }
    for segment in command.split('|') {
        let tokens = split_shell_words(segment.trim());
        if tokens.is_empty() {
            return false;
        }
        if !shell_tokens_are_read_only(&tokens) {
            return false;
        }
    }
    true
}

fn shell_tokens_contain_output_redirection(tokens: &[String]) -> bool {
    tokens.iter().any(|token| {
        if token == ">" || token.starts_with(">>") {
            return true;
        }
        token.contains('>')
            && !token.starts_with("2>")
            && !token.contains("2>&1")
            && !token.contains("&>")
    })
}

fn shell_tokens_are_read_only(tokens: &[String]) -> bool {
    if shell_tokens_contain_output_redirection(tokens) {
        return false;
    }
    let Some(cmd) = tokens.first().map(|value| value.to_ascii_lowercase()) else {
        return false;
    };
    if matches!(
        cmd.as_str(),
        "echo" | "printf" | "true" | "false" | "pwd" | "which" | "type" | "where"
    ) {
        return true;
    }
    if matches!(
        cmd.as_str(),
        "grep"
            | "egrep"
            | "fgrep"
            | "zgrep"
            | "bzgrep"
            | "rg"
            | "ripgrep"
            | "ag"
            | "ack"
            | "findstr"
    ) {
        return true;
    }
    if matches!(
        cmd.as_str(),
        "cat"
            | "zcat"
            | "head"
            | "tail"
            | "less"
            | "more"
            | "ls"
            | "dir"
            | "wc"
            | "sort"
            | "uniq"
            | "cut"
            | "nl"
            | "file"
            | "stat"
            | "readlink"
            | "realpath"
            | "dirname"
            | "basename"
            | "diff"
            | "cmp"
            | "strings"
            | "tree"
    ) {
        return true;
    }
    if cmd == "find" {
        return find_tokens_are_read_only(tokens);
    }
    if cmd == "sed" {
        return sed_tokens_are_read_only(tokens);
    }
    if cmd == "cargo" {
        return cargo_tokens_are_read_only(tokens);
    }
    if is_git_executable(&cmd) {
        return git_tokens_are_read_only(tokens);
    }
    false
}

fn find_tokens_are_read_only(tokens: &[String]) -> bool {
    const BLOCKED: &[&str] = &[
        "-exec",
        "-execdir",
        "-delete",
        "-ok",
        "-okdir",
        "-fprintf",
        "-fls",
    ];
    !tokens.iter().any(|token| {
        let lower = token.to_ascii_lowercase();
        BLOCKED
            .iter()
            .any(|flag| lower == *flag || lower.starts_with(&format!("{flag}=")))
    })
}

fn sed_tokens_are_read_only(tokens: &[String]) -> bool {
    tokens.iter().skip(1).all(|arg| {
        arg.starts_with('-')
            || arg
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | 'p' | 'q' | '='))
    })
}

fn cargo_tokens_are_read_only(tokens: &[String]) -> bool {
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if token.starts_with('-') {
            index += 1;
            continue;
        }
        let sub = token.to_ascii_lowercase();
        return matches!(
            sub.as_str(),
            "check" | "tree" | "metadata" | "version" | "locate-project"
        );
    }
    false
}

/// Whether a bash invocation is clearly mutating (blocked in READ/PLAN without user bypass).
pub fn is_clearly_mutating_bash_args(args: &Value) -> bool {
    let Some(command) = args.get("command").and_then(|v| v.as_str()) else {
        return false;
    };
    is_clearly_mutating_bash_command(command)
}

pub fn is_clearly_mutating_bash_command(command: &str) -> bool {
    if command.contains("$(") || command.contains('`') {
        return true;
    }
    for segment in command.split('|') {
        let tokens = split_shell_words(segment.trim());
        if tokens.is_empty() {
            return false;
        }
        if !shell_tokens_are_clearly_mutating(&tokens) {
            return false;
        }
    }
    true
}

fn shell_tokens_are_clearly_mutating(tokens: &[String]) -> bool {
    let Some(cmd) = tokens.first().map(|value| value.to_ascii_lowercase()) else {
        return false;
    };
    if matches!(
        cmd.as_str(),
        "rm"
            | "rmdir"
            | "del"
            | "erase"
            | "mv"
            | "move"
            | "cp"
            | "copy"
            | "xcopy"
            | "robocopy"
            | "mkdir"
            | "md"
            | "touch"
            | "chmod"
            | "chown"
            | "chgrp"
            | "npm"
            | "pnpm"
            | "yarn"
            | "npx"
            | "make"
            | "cmake"
            | "pip"
            | "pip3"
            | "python"
            | "python3"
            | "node"
            | "deno"
            | "bun"
            | "dotnet"
            | "msbuild"
            | "rustc"
            | "javac"
            | "gradle"
            | "mvn"
    ) {
        return true;
    }
    if cmd == "cargo" {
        return !cargo_tokens_are_read_only(tokens);
    }
    if is_git_executable(&cmd) {
        return !git_tokens_are_read_only(tokens);
    }
    if cmd == "sed" {
        return !sed_tokens_are_read_only(tokens);
    }
    if cmd == "find" {
        return !find_tokens_are_read_only(tokens);
    }
    false
}

fn is_git_executable(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "git" | "git.exe" | "git.cmd" | "git.bat"
    )
}

fn git_tokens_are_read_only(tokens: &[String]) -> bool {
    let subcommand_index = match git_subcommand_index(tokens) {
        Some(index) => index,
        None => return false,
    };
    let subcommand = tokens
        .get(subcommand_index)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        subcommand.as_str(),
        "status" | "diff" | "log" | "show" | "ls-files" | "rev-parse" | "branch" | "remote"
    )
}

fn git_subcommand_index(tokens: &[String]) -> Option<usize> {
    if !tokens
        .first()
        .map(|value| is_git_executable(value))
        .unwrap_or(false)
    {
        return None;
    }

    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        match token {
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace" => {
                index += 2;
            }
            "--no-pager" | "--paginate" => {
                index += 1;
            }
            value
                if value.starts_with("-c")
                    || value.starts_with("--git-dir=")
                    || value.starts_with("--work-tree=")
                    || value.starts_with("--namespace=") =>
            {
                index += 1;
            }
            value if value.starts_with('-') => {
                index += 1;
            }
            _ => return Some(index),
        }
    }
    None
}

fn split_shell_words(segment: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in segment.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }

    if quote.is_none() && !escaped && !current.is_empty() {
        words.push(current);
    }

    words
}

/// When implementer is allowed in PLAN phase (plan confirmed), advance phase before dispatch.
pub fn advance_to_implement_if_allowed(gate: &mut WorkflowGate, args: &Value) -> bool {
    let sub = args
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if sub != "implementer"
        || !gate.read_satisfied()
        || !gate.plan_confirmed
        || gate.plan_zero_change
    {
        return false;
    }
    if gate.phase == CodeEditPhase::Plan {
        gate.phase = CodeEditPhase::Implement;
        return true;
    }
    false
}

/// When reviewer is allowed for read-only review, advance phase before dispatch.
pub fn advance_to_review_if_allowed(gate: &mut WorkflowGate, args: &Value) -> bool {
    let sub = args
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if sub != "reviewer" {
        return false;
    }
    match gate.phase {
        CodeEditPhase::Read => {
            gate.phase = CodeEditPhase::Review;
            true
        }
        CodeEditPhase::Plan if !gate.plan_confirmed => {
            gate.phase = CodeEditPhase::Review;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_strict_gate_never_blocks_edit() {
        let mut gate = WorkflowGate::with_strict(false);
        let err = gate.check_tool("edit", &serde_json::json!({"path": "src/foo.rs"}));
        assert!(err.is_none());
    }

    #[test]
    fn tool_call_not_globally_exempt() {
        assert!(!is_exempt_tool_call(
            "tool_call",
            &serde_json::json!({
                "toolName": "edit",
                "arguments": {"path": "src/foo.rs"}
            })
        ));
    }

    fn satisfy_read_gates(gate: &mut WorkflowGate) {
        gate.exploration_satisfied = true;
    }

    fn satisfy_full_read_gates(gate: &mut WorkflowGate) {
        gate.exploration_satisfied = true;
        gate.codegraph_satisfied = true;
    }

    fn enter_plan_phase(gate: &mut WorkflowGate) {
        satisfy_read_gates(gate);
        gate.maybe_advance_read_to_plan();
        assert_eq!(gate.phase, CodeEditPhase::Plan);
    }

    fn confirm_plan(gate: &mut WorkflowGate) {
        gate.plan_confirmed = true;
    }

    #[test]
    fn tool_call_edit_blocked_in_read_phase_when_read_gate_satisfied() {
        let mut gate = WorkflowGate::with_strict(true);
        satisfy_read_gates(&mut gate);
        let args = serde_json::json!({
            "toolName": "edit",
            "arguments": {"path": "src/foo.rs"}
        });
        let err = gate.check_tool("tool_call", &args);
        assert!(err.is_some());
        let msg = err.unwrap();
        assert!(msg.contains("read_gate=true"));
        assert!(msg.contains("PLAN"));
    }

    #[test]
    fn read_alone_satisfies_read_gate_without_codegraph() {
        let mut gate = WorkflowGate::with_strict(true);
        assert!(!gate.read_satisfied());
        let args = serde_json::json!({
            "toolName": "read",
            "arguments": {"path": "src/a.rs"}
        });
        let hint = gate.on_tool_success("tool_call", &args, None).unwrap();
        assert!(gate.exploration_satisfied);
        assert!(!gate.codegraph_satisfied);
        assert!(gate.read_satisfied());
        assert!(hint.contains("PLAN"));
        assert_eq!(gate.phase, CodeEditPhase::Plan);
    }

    #[test]
    fn codegraph_context_satisfies_full_read_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        let hint = gate
            .on_tool_success(
                "codegraph_context",
                &serde_json::json!({"task": "auth flow"}),
                None,
            )
            .unwrap();
        assert!(gate.codegraph_satisfied);
        assert!(gate.exploration_satisfied);
        assert!(gate.read_satisfied());
        assert!(hint.contains("PLAN"));
        assert_eq!(gate.phase, CodeEditPhase::Plan);
    }

    #[test]
    fn exploration_plus_codegraph_impact_satisfies_read_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        let hint = gate.on_tool_success("grep", &serde_json::json!({"pattern": "foo"}), None);
        assert!(gate.read_satisfied());
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("PLAN"));
        gate.on_tool_success(
            "codegraph_impact",
            &serde_json::json!({"symbol": "foo"}),
            None,
        );
        assert!(gate.codegraph_satisfied);
        assert!(gate.read_satisfied());
    }

    #[test]
    fn explorer_satisfies_exploration_and_read_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        let hint = gate
            .on_tool_success(
                "task",
                &serde_json::json!({
                    "subagent_type": "explorer",
                    "prompt": "find auth",
                    "description": "explore"
                }),
                None,
            )
            .unwrap();
        assert!(gate.exploration_satisfied);
        assert!(!gate.codegraph_satisfied);
        assert!(gate.read_satisfied());
        assert!(hint.contains("PLAN"));
        assert_eq!(gate.phase, CodeEditPhase::Plan);
    }

    #[test]
    fn tool_load_codegraph_does_not_satisfy_codegraph_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.exploration_satisfied = true;
        let hint = gate
            .on_tool_success(
                "tool_load",
                &serde_json::json!({
                    "tools": ["codegraph_context", "codegraph_impact"]
                }),
                None,
            )
            .unwrap();
        assert!(!gate.codegraph_satisfied);
        assert!(gate.read_satisfied());
        assert!(hint.contains("does NOT execute CodeGraph"));
        assert!(hint.contains("codegraph_context"));
    }

    #[test]
    fn tool_call_codegraph_context_satisfies_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        let args = serde_json::json!({
            "toolName": "codegraph_context",
            "arguments": {"task": "auth flow"}
        });
        let hint = gate.on_tool_success("tool_call", &args, None).unwrap();
        assert!(gate.codegraph_satisfied);
        assert!(gate.read_satisfied());
        assert!(hint.contains("PLAN"));
        assert_eq!(gate.phase, CodeEditPhase::Plan);
    }

    #[test]
    fn review_phase_blocks_edit_with_clear_hint() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        satisfy_read_gates(&mut gate);
        gate.review_cycle = 2;
        let err = gate.check_tool("edit", &serde_json::json!({"path": "src/foo.rs"}));
        assert!(err.is_some());
        let msg = err.unwrap();
        assert!(msg.contains("task(subagent_type=reviewer)"));
        assert!(msg.contains("retry cycle #2"));
    }

    #[test]
    fn resolve_effective_tool_name_parses_meta_call() {
        assert_eq!(
            resolve_effective_tool_name(
                "tool_call",
                &serde_json::json!({"toolName": "read", "arguments": {}})
            ),
            "read"
        );
        assert_eq!(
            resolve_effective_tool_name("grep", &serde_json::json!({"pattern": "x"})),
            "grep"
        );
    }

    #[test]
    fn read_phase_hides_write_tools_from_request_list() {
        let gate = WorkflowGate::with_strict(true);
        assert!(gate.hidden_request_tools().contains(&"edit"));
        assert!(gate.hidden_request_tools().contains(&"write"));
        assert!(!gate.hidden_request_tools().contains(&"bash"));
    }

    #[test]
    fn implement_phase_hides_bash_from_request_list() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Implement;
        assert!(gate.hidden_request_tools().contains(&"bash"));
    }

    #[test]
    fn read_phase_allows_read_only_git_diff_bash() {
        let mut gate = WorkflowGate::with_strict(true);
        let args = serde_json::json!({"command": "git diff"});
        assert!(gate.check_tool("bash", &args).is_none());
    }

    #[test]
    fn read_phase_blocks_mutating_git_commit_bash() {
        let mut gate = WorkflowGate::with_strict(true);
        let args = serde_json::json!({"command": "git commit -m \"test\""});
        let err = gate.check_tool("bash", &args).unwrap();
        assert!(err.contains("mutating commands are blocked"));
    }

    #[test]
    fn read_phase_allows_ambiguous_bash_pending_user_confirm() {
        let mut gate = WorkflowGate::with_strict(true);
        let args = serde_json::json!({"command": "some_custom_script.sh --dry-run"});
        assert!(gate.check_tool("bash", &args).is_none());
        let whitelist = WorkflowAmbiguousWhitelist::default();
        assert!(workflow_ambiguous_tool_requires_user_confirm(
            &gate, "bash", &args, &whitelist
        ));
        assert_eq!(
            classify_tool_workflow_kind("bash", &args),
            ToolWorkflowKind::Ambiguous
        );
    }

    #[test]
    fn ambiguous_tool_whitelist_skips_confirm_for_same_bash_command() {
        let gate = WorkflowGate::with_strict(true);
        let args = serde_json::json!({"command": "  some_custom_script.sh   --dry-run  "});
        let mut whitelist = WorkflowAmbiguousWhitelist::default();
        assert!(workflow_ambiguous_tool_requires_user_confirm(
            &gate, "bash", &args, &whitelist
        ));
        whitelist.add("bash", &args);
        assert!(!workflow_ambiguous_tool_requires_user_confirm(
            &gate, "bash", &args, &whitelist
        ));
        assert!(whitelist.is_whitelisted(
            "bash",
            &serde_json::json!({"command": "some_custom_script.sh --dry-run"})
        ));
        assert!(!whitelist.is_whitelisted(
            "bash",
            &serde_json::json!({"command": "other_script.sh"})
        ));
    }

    #[test]
    fn bash_whitelist_prefix_entry_matches_longer_grep_command() {
        let mut list = WorkflowAmbiguousWhitelist::default();
        list.bash_commands.insert("grep -rn".to_string());
        let cmd = r#"grep -rn "xlua" Assets.Lua/ 2>/dev/null | head -10"#;
        assert!(list.is_whitelisted("bash", &serde_json::json!({"command": cmd})));
    }

    #[test]
    fn read_plan_whitelist_skips_grep_bash_confirm() {
        let gate = WorkflowGate::with_strict(true);
        let mut list = WorkflowAmbiguousWhitelist::default();
        list.bash_commands.insert("grep -rn".to_string());
        let args = serde_json::json!({"command": r#"grep -rn "foo" Assets/"#});
        assert!(workflow_read_plan_whitelist_skips_tool_confirm(
            &gate, "bash", &args, &list
        ));
        assert!(workflow_read_plan_whitelist_offerable(
            &gate, "bash", &args, false
        ));
    }

    #[test]
    fn ambiguous_tool_whitelist_skips_confirm_for_non_bash_tool() {
        let gate = WorkflowGate::with_strict(true);
        let args = serde_json::json!({"foo": "bar"});
        let mut whitelist = WorkflowAmbiguousWhitelist::default();
        assert!(workflow_ambiguous_tool_requires_user_confirm(
            &gate,
            "lazy_unknown_tool",
            &args,
            &whitelist,
        ));
        whitelist.add("lazy_unknown_tool", &args);
        assert!(!workflow_ambiguous_tool_requires_user_confirm(
            &gate,
            "lazy_unknown_tool",
            &args,
            &whitelist,
        ));
    }

    #[test]
    fn classify_unknown_tool_as_ambiguous() {
        assert_eq!(
            classify_tool_workflow_kind(
                "lazy_unknown_tool",
                &serde_json::json!({"foo": "bar"}),
            ),
            ToolWorkflowKind::Ambiguous
        );
        assert_eq!(
            classify_tool_workflow_kind("grep", &serde_json::json!({"pattern": "x"})),
            ToolWorkflowKind::ReadOnly
        );
        assert_eq!(
            classify_tool_workflow_kind("web_fetch", &serde_json::json!({"url": "https://example.com"})),
            ToolWorkflowKind::ReadOnly
        );
    }

    #[test]
    fn implement_phase_blocks_bash_even_when_read_only() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Implement;
        let args = serde_json::json!({"command": "git diff"});
        assert!(gate.check_tool("bash", &args).is_some());
    }

    #[test]
    fn is_read_only_bash_command_recognizes_git_status_and_log() {
        assert!(is_read_only_bash_command("git status"));
        assert!(is_read_only_bash_command("git log --oneline -5"));
        assert!(!is_read_only_bash_command("git add ."));
        assert!(!is_read_only_bash_command("cargo build"));
    }

    #[test]
    fn is_rm_bash_command_detects_rm_and_compound_chains() {
        assert!(is_rm_bash_command("rm -rf Assets.Lua"));
        assert!(is_rm_bash_command("Remove-Item -Recurse -Force foo"));
        assert!(is_rm_bash_command("diff a b && rm -rf Assets.Lua"));
        assert!(!is_rm_bash_command("grep -r foo Assets.Lua"));
        assert!(!is_rm_bash_command("git status"));
    }

    #[test]
    fn bash_rm_requires_user_confirm_for_bash_args() {
        let args = serde_json::json!({"command": "rm -f foo.lua"});
        assert!(bash_rm_requires_user_confirm(&args));
        assert!(!bash_rm_requires_user_confirm(&serde_json::json!({"command": "ls"})));
    }

    #[test]
    fn is_read_only_bash_command_recognizes_grep_and_pipelines() {
        assert!(is_read_only_bash_command("grep -r WorkflowGate src-tauri"));
        assert!(is_read_only_bash_command("rg plan_confirmed --glob '*.rs'"));
        assert!(is_read_only_bash_command("grep -r foo . | head -20"));
        assert!(!is_read_only_bash_command("grep foo > /tmp/out.txt"));
        assert!(!is_read_only_bash_command("grep foo | rm -rf ."));
    }

    #[test]
    fn plan_phase_allows_grep_bash() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Plan;
        satisfy_read_gates(&mut gate);
        let args = serde_json::json!({"command": "grep -r plan_confirmed src-tauri"});
        assert!(gate.check_tool("bash", &args).is_none());
    }

    #[test]
    fn read_only_cargo_check_allowed_in_plan_phase() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Plan;
        let args = serde_json::json!({"command": "cargo check -p locus"});
        assert!(gate.check_tool("bash", &args).is_none());
    }

    #[test]
    fn non_strict_gate_does_not_hide_write_tools() {
        let gate = WorkflowGate::with_strict(false);
        assert!(gate.hidden_request_tools().is_empty());
    }

    #[test]
    fn prioritize_request_tools_puts_codegraph_first() {
        let gate = WorkflowGate::with_strict(true);
        let mut names = vec![
            "tool_load".to_string(),
            "tool_call".to_string(),
            "read".to_string(),
            "edit".to_string(),
            "grep".to_string(),
            "codegraph_impact".to_string(),
            "codegraph_context".to_string(),
        ];
        gate.prioritize_request_tools(&mut names);
        assert_eq!(names[0], "tool_load");
        assert_eq!(names[1], "tool_call");
        assert_eq!(names[2], "codegraph_context");
        assert_eq!(names[3], "codegraph_impact");
    }

    #[test]
    fn edit_blocked_after_exploration_directs_to_plan() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.on_tool_success("grep", &serde_json::json!({"pattern": "foo"}), None);
        assert!(gate.read_satisfied());
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        let err = gate
            .check_tool("edit", &serde_json::json!({"path": "src/foo.rs"}))
            .unwrap();
        assert!(err.contains("PLAN phase"));
    }

    #[test]
    fn status_reminder_allows_simple_path_without_codegraph() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.on_tool_success("read", &serde_json::json!({"path": "src/a.rs"}), None);
        let reminder = gate.status_reminder();
        assert!(reminder.contains("exploration_gate=true"));
        assert!(reminder.contains("codegraph_gate=false"));
        assert!(reminder.contains("modification plan"));
    }

    #[test]
    fn complex_implementer_blocked_without_codegraph() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        confirm_plan(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "Refactor auth across src/a.rs, src/b.rs, and src/c.rs",
            "description": "refactor"
        });
        let err = gate.check_tool("task", &args).unwrap();
        assert!(err.contains("complex edit"));
        assert!(err.contains("codegraph_gate=false"));
    }

    #[test]
    fn simple_implementer_allowed_without_codegraph_after_plan_confirm() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        confirm_plan(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "Fix typo in src/foo.rs line 10",
            "description": "fix"
        });
        assert!(gate.check_tool("task", &args).is_none());
    }

    #[test]
    fn implementer_blocked_in_plan_until_confirmed() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "Fix typo in src/foo.rs line 10",
            "description": "fix"
        });
        let err = gate.check_tool("task", &args).unwrap();
        assert!(err.contains("plan not confirmed"));
    }

    #[test]
    fn read_phase_blocks_edit() {
        let mut gate = WorkflowGate::with_strict(true);
        let err = gate.check_tool("edit", &serde_json::json!({"path": "src/foo.rs"}));
        assert!(err.is_some());
    }

    #[test]
    fn read_phase_allows_unity_execute() {
        let mut gate = WorkflowGate::with_strict(true);
        assert!(!gate.read_satisfied());
        let err = gate.check_tool(
            "unity_execute",
            &serde_json::json!({"code": "Debug.Log(\"ok\");"}),
        );
        assert!(err.is_none());
        assert!(is_unity_editor_workflow_tool("unity_execute"));
        assert!(!is_write_tool("unity_execute"));
    }

    #[test]
    fn review_phase_allows_unity_execute() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        satisfy_read_gates(&mut gate);
        let err = gate.check_tool(
            "unity_execute",
            &serde_json::json!({"code": "Debug.Log(\"ok\");"}),
        );
        assert!(err.is_none());
    }

    #[test]
    fn reviewer_allowed_in_read_without_exploration_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        assert!(!gate.read_satisfied());
        let args = serde_json::json!({
            "subagent_type": "reviewer",
            "prompt": "review gc hotspots in Application.lua",
            "description": "review"
        });
        assert!(gate.check_tool("task", &args).is_none());
        assert!(advance_to_review_if_allowed(&mut gate, &args));
        assert_eq!(gate.phase, CodeEditPhase::Review);
    }

    #[test]
    fn read_satisfied_allows_reviewer_for_read_only_review() {
        let mut gate = WorkflowGate::with_strict(true);
        satisfy_read_gates(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "reviewer",
            "prompt": "review gc",
            "description": "review"
        });
        assert!(gate.check_tool("task", &args).is_none());
        assert!(advance_to_review_if_allowed(&mut gate, &args));
        assert_eq!(gate.phase, CodeEditPhase::Review);
    }

    #[test]
    fn reviewer_allowed_for_complex_prompt_without_codegraph_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        assert!(!gate.codegraph_satisfied);
        let args = serde_json::json!({
            "subagent_type": "reviewer",
            "prompt": "refactor cross-module architecture and review blast radius",
            "description": "review"
        });
        assert!(gate.check_tool("task", &args).is_none());
    }

    #[test]
    fn plan_phase_allows_reviewer_before_plan_confirmation() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        assert!(!gate.plan_confirmed);
        let args = serde_json::json!({
            "subagent_type": "reviewer",
            "prompt": "review attached files",
            "description": "review"
        });
        assert!(gate.check_tool("task", &args).is_none());
        assert!(advance_to_review_if_allowed(&mut gate, &args));
        assert_eq!(gate.phase, CodeEditPhase::Review);
    }

    #[test]
    fn plan_phase_blocks_reviewer_after_plan_confirmation() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        confirm_plan(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "reviewer",
            "prompt": "review attached files",
            "description": "review"
        });
        let err = gate.check_tool("task", &args).unwrap();
        assert!(err.contains("Plan is confirmed"));
        assert!(!advance_to_review_if_allowed(&mut gate, &args));
        assert_eq!(gate.phase, CodeEditPhase::Plan);
    }

    #[test]
    fn read_satisfied_blocks_implementer_until_plan() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "do it",
            "description": "impl"
        });
        assert!(gate.check_tool("task", &args).is_some());
    }

    #[test]
    fn implement_phase_blocks_direct_write() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Implement;
        satisfy_read_gates(&mut gate);
        let err = gate.check_tool("write", &serde_json::json!({"path": "src/foo.rs"}));
        assert!(err.is_some());
    }

    #[test]
    fn knowledge_markdown_exempt() {
        let mut gate = WorkflowGate::with_strict(true);
        let err = gate.check_tool(
            "edit",
            &serde_json::json!({"path": "Locus/knowledge/design/note.md"}),
        );
        assert!(err.is_none());
    }

    #[test]
    fn knowledge_tools_exempt_from_workflow_in_any_phase() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        for tool in [
            "knowledge_create",
            "knowledge_edit",
            "knowledge_move",
            "knowledge_delete",
            "skill_create",
            "skill_reload",
            "skill_list",
        ] {
            assert!(
                gate.check_tool(tool, &serde_json::json!({})).is_none(),
                "{tool} should not be blocked"
            );
        }
    }

    #[test]
    fn knowledge_tools_not_hidden_during_implement_phase() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Implement;
        let hidden = gate.hidden_request_tools();
        assert!(!hidden.contains(&"knowledge_create"));
        assert!(!hidden.contains(&"skill_create"));
    }

    #[test]
    fn knowledge_read_does_not_satisfy_exploration_gate() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.on_tool_success("knowledge_read", &serde_json::json!({}), None);
        assert!(!gate.exploration_satisfied);
        assert!(!gate.read_satisfied());
    }

    #[test]
    fn subagent_done_advances_phases() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Implement;
        let msg = gate.on_subagent_done("implementer", false, None);
        assert_eq!(gate.phase, CodeEditPhase::Optimize);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("task(subagent_type=\"optimizer\")"));
        gate.on_subagent_done("optimizer", false, None);
        assert_eq!(gate.phase, CodeEditPhase::Review);
        gate.on_subagent_done("reviewer", false, Some("Overall verdict: PASS"));
        assert_eq!(gate.phase, CodeEditPhase::Complete);
        assert!(gate.take_completion_report_pending().is_some());
    }

    #[test]
    fn completion_report_pending_consumed_once_per_cycle() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        gate.on_subagent_done("reviewer", false, Some("Overall verdict: PASS"));
        let first = gate.take_completion_report_pending();
        assert!(first.is_some());
        assert_eq!(first.unwrap().review_cycle, 0);
        assert!(gate.take_completion_report_pending().is_none());
    }

    #[test]
    fn zero_change_plan_marks_completion_report_pending() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        let args = serde_json::json!({
            "question": "请确认保持现状",
            "options": [
                {
                    "label": "确认执行",
                    "description": "确认本轮不修改任何文件,工作区保持当前状态"
                },
                { "label": "取消", "description": "取消" },
                { "label": "修改", "description": "修改计划" }
            ]
        });
        gate.on_tool_success(
            "ask_user_question",
            &args,
            Some("User answered: 确认执行"),
        );
        assert_eq!(gate.phase, CodeEditPhase::Complete);
        let trigger = gate.take_completion_report_pending().expect("pending");
        assert!(trigger.zero_change);
        assert!(trigger.verdict.is_none());
    }

    #[test]
    fn reviewer_block_loops_to_read() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        satisfy_read_gates(&mut gate);
        let msg = gate.on_subagent_done("reviewer", false, Some("Verdict: BLOCK\nFix null check."));
        assert_eq!(gate.phase, CodeEditPhase::Read);
        assert!(!gate.read_satisfied());
        assert_eq!(gate.review_cycle, 1);
        assert!(msg.is_some());
    }

    #[test]
    fn implementer_blocked_in_read_after_review_block() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        satisfy_read_gates(&mut gate);
        gate.on_subagent_done("reviewer", false, Some("Verdict: BLOCK"));
        assert_eq!(gate.phase, CodeEditPhase::Read);
        assert_eq!(gate.review_cycle, 1);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "apply review fixes",
            "description": "fix"
        });
        let err = gate.check_tool("task", &args).unwrap();
        assert!(err.contains("retry cycle #1"));
        assert!(err.contains("cannot skip directly to implementer"));
        let reminder = gate.status_reminder();
        assert!(reminder.contains("review retry cycle"));
    }

    #[test]
    fn reviewer_pass_with_risks_completes() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        let msg = gate.on_subagent_done("reviewer", false, Some("PASS_WITH_RISKS: minor naming issues"));
        assert_eq!(gate.phase, CodeEditPhase::Complete);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("completion report"));
    }

    #[test]
    fn complete_phase_read_starts_new_cycle_with_hint() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Complete;
        let hint = gate.on_tool_success("read", &serde_json::json!({"path": "src/a.rs"}), None);
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        assert!(gate.read_satisfied());
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("PLAN"));
    }

    #[test]
    fn complete_phase_implementer_requires_read_first() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Complete;
        gate.reset_read_gates();
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "fix",
            "description": "fix"
        });
        assert!(gate.check_tool("task", &args).is_some());
    }

    #[test]
    fn complete_phase_implementer_after_read_advances_to_review() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Complete;
        gate.on_tool_success("grep", &serde_json::json!({"pattern": "x"}), None);
        gate.on_tool_success(
            "codegraph_context",
            &serde_json::json!({"task": "grep hits"}),
            None,
        );
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        assert!(gate.read_satisfied());
        confirm_plan(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "more",
            "description": "more"
        });
        assert!(gate.check_tool("task", &args).is_none());
        assert!(advance_to_implement_if_allowed(&mut gate, &args));
        assert_eq!(gate.phase, CodeEditPhase::Implement);
        gate.on_subagent_done("implementer", false, None);
        assert_eq!(gate.phase, CodeEditPhase::Optimize);
        gate.on_subagent_done("optimizer", false, None);
        assert_eq!(gate.phase, CodeEditPhase::Review);
    }

    #[test]
    fn implementer_done_emits_optimizer_hint() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Implement;
        gate.review_cycle = 1;
        let msg = gate.on_subagent_done("implementer", false, None).unwrap();
        assert_eq!(gate.phase, CodeEditPhase::Optimize);
        assert!(msg.contains("retry cycle #1"));
        assert!(msg.contains("optimizer"));
    }

    #[test]
    fn optimizer_done_emits_reviewer_hint() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Optimize;
        gate.review_cycle = 1;
        let msg = gate.on_subagent_done("optimizer", false, None).unwrap();
        assert_eq!(gate.phase, CodeEditPhase::Review);
        assert!(msg.contains("retry cycle #1"));
        assert!(msg.contains("reviewer"));
        assert!(msg.contains("do not stop"));
    }

    #[test]
    fn optimize_phase_blocks_direct_write() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Optimize;
        satisfy_read_gates(&mut gate);
        let err = gate.check_tool("write", &serde_json::json!({"path": "src/foo.rs"}));
        assert!(err.is_some());
        assert!(err.unwrap().contains("OPTIMIZE"));
    }

    #[test]
    fn optimize_phase_only_allows_optimizer_task() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Optimize;
        let reviewer_args = serde_json::json!({
            "subagent_type": "reviewer",
            "prompt": "review",
            "description": "review"
        });
        let err = gate.check_tool("task", &reviewer_args).unwrap();
        assert!(err.contains("optimizer"));
        let optimizer_args = serde_json::json!({
            "subagent_type": "optimizer",
            "prompt": "optimize",
            "description": "optimize"
        });
        assert!(gate.check_tool("task", &optimizer_args).is_none());
    }

    #[test]
    fn parse_review_verdict_cases() {
        assert_eq!(
            parse_review_verdict("Overall: PASS"),
            ReviewVerdict::Pass
        );
        assert_eq!(
            parse_review_verdict("PASS_WITH_RISKS"),
            ReviewVerdict::PassWithRisks
        );
        assert_eq!(parse_review_verdict("BLOCK: security issue"), ReviewVerdict::Block);
        assert_eq!(
            parse_review_verdict("结论：未通过，需要修复空指针"),
            ReviewVerdict::Block
        );
        assert_eq!(parse_review_verdict("looks fine"), ReviewVerdict::Unknown);
        assert_eq!(
            parse_review_verdict("结论末尾明确 \"**PASS**\""),
            ReviewVerdict::Pass
        );
        assert_eq!(
            parse_review_verdict("VERDICT: PASS\nAll fixes verified."),
            ReviewVerdict::Pass
        );
    }

    #[test]
    fn read_success_emits_implementer_hint_on_retry() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.review_cycle = 1;
        let hint = gate.on_tool_success("read", &serde_json::json!({"path": "a.rs"}), None);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("PLAN"));
        assert!(gate.read_satisfied());
        gate.on_tool_success(
            "codegraph_impact",
            &serde_json::json!({"symbol": "Foo"}),
            None,
        );
        assert!(gate.codegraph_satisfied);
    }

    #[test]
    fn retry_read_then_implementer_allowed_after_plan_confirm() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.phase = CodeEditPhase::Review;
        gate.on_subagent_done("reviewer", false, Some("BLOCK"));
        assert_eq!(gate.phase, CodeEditPhase::Read);
        gate.on_tool_success("grep", &serde_json::json!({"pattern": "foo"}), None);
        gate.on_tool_success(
            "codegraph_context",
            &serde_json::json!({"task": "fix block feedback"}),
            None,
        );
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        confirm_plan(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "fix",
            "description": "fix"
        });
        assert!(gate.check_tool("task", &args).is_none());
        assert!(advance_to_implement_if_allowed(&mut gate, &args));
        assert_eq!(gate.phase, CodeEditPhase::Implement);
    }

    #[test]
    fn ask_user_in_read_with_codegraph_only_does_not_confirm_plan() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.on_tool_success(
            "codegraph_impact",
            &serde_json::json!({"symbol": "WorkflowGate"}),
            None,
        );
        assert!(gate.codegraph_satisfied);
        assert!(!gate.exploration_satisfied);
        assert_eq!(gate.phase, CodeEditPhase::Read);
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({"question": "confirm plan", "options": []}),
                Some("User answered: 确认执行"),
            )
            .unwrap();
        assert!(!gate.plan_confirmed);
        assert_eq!(gate.phase, CodeEditPhase::Read);
        assert!(hint.contains("NOT recorded"));
        assert!(hint.contains("plan_confirmed remains false"));
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "implement plan",
            "description": "implement"
        });
        let err = gate.check_tool("task", &args).unwrap();
        assert!(err.contains("exploration_gate"));
    }

    #[test]
    fn read_partial_hint_when_codegraph_without_exploration() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.codegraph_satisfied = true;
        gate.exploration_satisfied = false;
        gate.phase = CodeEditPhase::Read;
        let partial = read_partial_progress_hint(&gate).unwrap();
        assert!(partial.contains("codegraph_gate=true, exploration_gate=false"));
        assert!(partial.contains("ask_user_question in READ phase does NOT record"));
    }

    #[test]
    fn ask_user_in_read_after_read_gate_advances_to_plan_and_confirms() {
        let mut gate = WorkflowGate::with_strict(true);
        gate.on_tool_success("read", &serde_json::json!({"path": "src/a.rs"}), None);
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({"question": "confirm", "options": []}),
                Some("User answered: 确认执行"),
            )
            .unwrap();
        assert!(gate.plan_confirmed);
        assert!(hint.contains("implementer"));
    }

    #[test]
    fn plan_confirm_via_ask_user_advances_to_implementer_ready() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({"question": "confirm plan", "options": []}),
                Some("User answered: 确认执行"),
            )
            .unwrap();
        assert!(gate.plan_confirmed);
        assert!(hint.contains("CodeGraph"));
        assert!(hint.contains("codegraph_gate"));
        assert!(hint.contains("implementer"));
    }

    #[test]
    fn plan_confirm_with_codegraph_hints_implementer_directly() {
        let mut gate = WorkflowGate::with_strict(true);
        satisfy_full_read_gates(&mut gate);
        gate.maybe_advance_read_to_plan();
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({"question": "confirm plan", "options": []}),
                Some("User answered: 确认执行"),
            )
            .unwrap();
        assert!(gate.plan_confirmed);
        assert!(hint.contains("Next required step: dispatch"));
        assert!(!hint.contains("Complex edits require CodeGraph first"));
    }

    #[test]
    fn plan_confirmed_status_reminder_without_codegraph() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        confirm_plan(&mut gate);
        let reminder = gate.status_reminder();
        assert!(reminder.contains("codegraph_gate=false"));
        assert!(reminder.contains("codegraph_context"));
        assert!(reminder.contains("PLAN phase"));
    }

    #[test]
    fn codegraph_in_plan_phase_satisfies_gate_then_implementer_allowed() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        confirm_plan(&mut gate);
        let args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "Refactor auth across src/a.rs, src/b.rs, and src/c.rs",
            "description": "refactor"
        });
        let err = gate.check_tool("task", &args).unwrap();
        assert!(err.contains("codegraph_gate=false"));
        assert!(err.contains("PLAN phase"));
        gate.on_tool_success(
            "codegraph_context",
            &serde_json::json!({"task": "auth refactor scope"}),
            None,
        );
        assert!(gate.codegraph_satisfied);
        assert!(gate.check_tool("task", &args).is_none());
    }

    #[test]
    fn prioritize_request_tools_in_plan_phase() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        assert!(!gate.codegraph_satisfied);
        let mut names = vec![
            "tool_load".to_string(),
            "tool_call".to_string(),
            "read".to_string(),
            "grep".to_string(),
            "codegraph_impact".to_string(),
            "codegraph_context".to_string(),
        ];
        gate.prioritize_request_tools(&mut names);
        assert_eq!(names[0], "tool_load");
        assert_eq!(names[1], "tool_call");
        assert_eq!(names[2], "codegraph_context");
        assert_eq!(names[3], "codegraph_impact");
    }

    #[test]
    fn plan_cancel_resets_to_read() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({}),
                Some("User answered: 取消"),
            )
            .unwrap();
        assert_eq!(gate.phase, CodeEditPhase::Read);
        assert!(!gate.read_satisfied());
        assert!(!gate.plan_confirmed);
    }

    #[test]
    fn plan_modify_keeps_plan_phase() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({}),
                Some("User answered: 增加单元测试覆盖"),
            )
            .unwrap();
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        assert!(!gate.plan_confirmed);
        assert!(hint.contains("Revise"));
    }

    #[test]
    fn plan_phase_text_stop_needs_ask_user_continuation() {
        let gate = WorkflowGate::with_strict(true);
        let mut gate = gate;
        enter_plan_phase(&mut gate);
        assert!(gate.needs_incomplete_workflow_continuation());
        let nudge = gate.take_incomplete_text_stop_nudge().unwrap();
        assert!(nudge.contains("ask_user_question"));
        assert_eq!(gate.workflow_text_stop_nudges, 1);
    }

    #[test]
    fn workflow_text_stop_nudges_cap_at_max() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        for _ in 0..MAX_WORKFLOW_TEXT_STOP_NUDGES {
            assert!(gate.take_incomplete_text_stop_nudge().is_some());
        }
        assert!(gate.take_incomplete_text_stop_nudge().is_none());
        assert!(!gate.needs_incomplete_workflow_continuation());
    }

    #[test]
    fn zero_change_plan_confirm_completes_without_implementer_nudge() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        let args = serde_json::json!({
            "question": "请确认保持现状",
            "options": [
                {
                    "label": "确认执行",
                    "description": "确认本轮不修改任何文件,工作区保持当前状态"
                },
                { "label": "取消", "description": "取消" },
                { "label": "修改", "description": "修改计划" }
            ]
        });
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &args,
                Some("User answered: 确认执行"),
            )
            .unwrap();
        assert!(gate.plan_confirmed);
        assert!(gate.plan_zero_change);
        assert_eq!(gate.phase, CodeEditPhase::Complete);
        assert!(hint.contains("zero-change"));
        assert!(hint.contains("do NOT dispatch"));
        assert!(hint.contains("completion report"));
        assert!(!gate.needs_incomplete_workflow_continuation());
        let task_args = serde_json::json!({
            "subagent_type": "implementer",
            "prompt": "noop",
            "description": "noop"
        });
        let err = gate.check_tool("task", &task_args).unwrap();
        assert!(err.contains("zero-change"));
    }

    #[test]
    fn zero_change_detected_from_answer_text() {
        assert!(looks_like_zero_change_text("确认零修改,保持现状"));
        assert!(is_zero_change_plan_confirmation(
            "User answered: 确认执行 — 零修改",
            &serde_json::json!({})
        ));
    }

    #[test]
    fn normal_plan_confirm_still_requires_implementer() {
        let mut gate = WorkflowGate::with_strict(true);
        enter_plan_phase(&mut gate);
        satisfy_full_read_gates(&mut gate);
        let hint = gate
            .on_tool_success(
                "ask_user_question",
                &serde_json::json!({
                    "question": "confirm plan",
                    "options": [
                        { "label": "确认执行", "description": "按计划修改 Coroutine.lua" }
                    ]
                }),
                Some("User answered: 确认执行"),
            )
            .unwrap();
        assert!(gate.plan_confirmed);
        assert!(!gate.plan_zero_change);
        assert_eq!(gate.phase, CodeEditPhase::Plan);
        assert!(hint.contains("implementer"));
        assert!(gate.needs_incomplete_workflow_continuation());
    }
}
