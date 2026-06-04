const TOOL_CONFIRM_FEEDBACK_PREFIX = "feedback:";

export function encodeToolConfirmFeedback(text: string): string {
  return `${TOOL_CONFIRM_FEEDBACK_PREFIX}${text.trim()}`;
}

export function encodeToolConfirmAllow(addToWorkflowWhitelist: boolean): string {
  return addToWorkflowWhitelist ? "allow:whitelist" : "allow";
}

