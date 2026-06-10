use super::{CompletionReportTrigger, ReviewVerdict};
use crate::session::models::{ChatMessage, MessageRole};
use crate::session::store::SessionStore;

const REPORT_HEADING: &str = "## 完成报告";
const MAX_SECTION_CHARS: usize = 12_000;

#[derive(Debug, Clone, Default)]
pub struct WorkflowCompletionContext {
    pub user_request: String,
    pub plan_text: String,
    pub implementer_output: String,
    pub optimizer_output: String,
    pub reviewer_output: String,
    pub review_cycle: u32,
    pub review_verdict: Option<ReviewVerdict>,
    pub zero_change: bool,
}

pub fn collect_workflow_completion_context(
    store: &SessionStore,
    parent_session_id: &str,
    trigger: CompletionReportTrigger,
) -> Result<WorkflowCompletionContext, String> {
    let parent_messages = store.get_messages(parent_session_id)?;
    let user_request = first_user_message_content(&parent_messages);
    let plan_text = extract_plan_text(&parent_messages);

    let mut ctx = WorkflowCompletionContext {
        user_request,
        plan_text,
        review_cycle: trigger.review_cycle,
        review_verdict: trigger.verdict,
        zero_change: trigger.zero_change,
        ..Default::default()
    };

    if trigger.zero_change {
        return Ok(ctx);
    }

    let children = store.list_child_sessions(parent_session_id)?;
    for (session_id, agent_id, _title) in children {
        let Some(agent_id) = agent_id else {
            continue;
        };
        let messages = store.get_messages(&session_id)?;
        let output = last_assistant_content(&messages).unwrap_or_default();
        match agent_id.as_str() {
            "implementer" => ctx.implementer_output = truncate_section(&output),
            "optimizer" => ctx.optimizer_output = truncate_section(&output),
            "reviewer" => ctx.reviewer_output = truncate_section(&output),
            _ => {}
        }
    }

    Ok(ctx)
}

pub fn build_completion_report_user_prompt(ctx: &WorkflowCompletionContext) -> String {
    let verdict_label = ctx
        .review_verdict
        .map(review_verdict_label)
        .unwrap_or("N/A");

    format!(
        "Workflow cycle #{}\nZero-change plan: {}\nReview verdict: {}\n\n\
         ## User request\n{}\n\n\
         ## Modification plan\n{}\n\n\
         ## Implementer output\n{}\n\n\
         ## Optimizer output\n{}\n\n\
         ## Reviewer output\n{}\n",
        ctx.review_cycle,
        ctx.zero_change,
        verdict_label,
        or_placeholder(&ctx.user_request),
        or_placeholder(&ctx.plan_text),
        or_placeholder(&ctx.implementer_output),
        or_placeholder(&ctx.optimizer_output),
        or_placeholder(&ctx.reviewer_output),
    )
}

pub fn is_workflow_completion_report(content: &str) -> bool {
    content.trim_start().starts_with(REPORT_HEADING)
}

fn review_verdict_label(verdict: ReviewVerdict) -> &'static str {
    match verdict {
        ReviewVerdict::Pass => "PASS",
        ReviewVerdict::PassWithRisks => "PASS_WITH_RISKS",
        ReviewVerdict::Block => "BLOCK",
        ReviewVerdict::Unknown => "UNKNOWN",
    }
}

fn first_user_message_content(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.content.trim().to_string())
        .unwrap_or_default()
}

fn extract_plan_text(messages: &[ChatMessage]) -> String {
    let plan_markers = [
        "modification plan",
        "修改计划",
        "rollback strategy",
        "impact assessment",
        "file list",
    ];
    let mut best = String::new();
    for message in messages {
        if message.role != MessageRole::Assistant {
            continue;
        }
        if is_workflow_completion_report(&message.content) {
            continue;
        }
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        let lower = content.to_lowercase();
        let looks_like_plan = plan_markers.iter().any(|marker| lower.contains(marker));
        if looks_like_plan || content.len() > best.len() {
            best = content.to_string();
        }
    }
    truncate_section(&best)
}

fn last_assistant_content(messages: &[ChatMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(|message| message.content.trim().to_string())
        .filter(|content| !content.is_empty())
}

fn truncate_section(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_SECTION_CHARS {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(MAX_SECTION_CHARS)
        .chain(std::iter::once('…'))
        .collect()
}

fn or_placeholder(value: &str) -> String {
    if value.trim().is_empty() {
        "(none provided)".to_string()
    } else {
        value.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::SessionStore;
    use tempfile::tempdir;

    fn add_assistant(store: &SessionStore, session_id: &str, content: &str) {
        store
            .add_message(session_id, MessageRole::Assistant, content)
            .expect("add assistant");
    }

    fn add_user(store: &SessionStore, session_id: &str, content: &str) {
        store
            .add_message(session_id, MessageRole::User, content)
            .expect("add user");
    }

    #[test]
    fn collect_context_aggregates_parent_and_child_sessions() {
        let dir = tempdir().expect("tempdir");
        let store = SessionStore::new(dir.path()).expect("store");
        let parent_id = store
            .create_session("Parent", None, None, "chat", Some("dev"))
            .expect("parent");
        add_user(&store, &parent_id, "Fix login timeout");
        add_assistant(
            &store,
            &parent_id,
            "Modification plan: file list\n- src/auth.rs\nimpact assessment\nrollback strategy",
        );

        let implementer_id = store
            .create_session(
                "sub:implement",
                Some(&parent_id),
                None,
                "chat",
                Some("implementer"),
            )
            .expect("implementer");
        add_assistant(&store, &implementer_id, "Implemented retry logic");

        let optimizer_id = store
            .create_session(
                "sub:optimize",
                Some(&parent_id),
                None,
                "chat",
                Some("optimizer"),
            )
            .expect("optimizer");
        add_assistant(&store, &optimizer_id, "Reduced allocations");

        let reviewer_id = store
            .create_session(
                "sub:review",
                Some(&parent_id),
                None,
                "chat",
                Some("reviewer"),
            )
            .expect("reviewer");
        add_assistant(&store, &reviewer_id, "Overall verdict: PASS");

        let ctx = collect_workflow_completion_context(
            &store,
            &parent_id,
            CompletionReportTrigger {
                review_cycle: 0,
                verdict: Some(ReviewVerdict::Pass),
                zero_change: false,
            },
        )
        .expect("context");

        assert_eq!(ctx.user_request, "Fix login timeout");
        assert!(ctx.plan_text.contains("Modification plan"));
        assert_eq!(ctx.implementer_output, "Implemented retry logic");
        assert_eq!(ctx.optimizer_output, "Reduced allocations");
        assert_eq!(ctx.reviewer_output, "Overall verdict: PASS");
    }

    #[test]
    fn zero_change_context_skips_child_sessions() {
        let dir = tempdir().expect("tempdir");
        let store = SessionStore::new(dir.path()).expect("store");
        let parent_id = store
            .create_session("Parent", None, None, "chat", Some("dev"))
            .expect("parent");
        add_user(&store, &parent_id, "Investigate only");

        let ctx = collect_workflow_completion_context(
            &store,
            &parent_id,
            CompletionReportTrigger {
                review_cycle: 0,
                verdict: None,
                zero_change: true,
            },
        )
        .expect("context");

        assert!(ctx.zero_change);
        assert!(ctx.implementer_output.is_empty());
    }

    #[test]
    fn build_prompt_includes_verdict_and_sections() {
        let prompt = build_completion_report_user_prompt(&WorkflowCompletionContext {
            user_request: "Add cache".to_string(),
            plan_text: "Plan details".to_string(),
            implementer_output: "Done".to_string(),
            optimizer_output: "Tuned".to_string(),
            reviewer_output: "PASS".to_string(),
            review_cycle: 1,
            review_verdict: Some(ReviewVerdict::PassWithRisks),
            zero_change: false,
        });
        assert!(prompt.contains("Workflow cycle #1"));
        assert!(prompt.contains("PASS_WITH_RISKS"));
        assert!(prompt.contains("Add cache"));
    }
}
