pub mod correction;
pub mod models;
pub mod proposal;
pub mod retrieval;
pub mod store;

pub use models::*;
pub use correction::{
    build_correction_draft, collect_tool_rejection_feedbacks, correction_likely_valid,
    extract_session_correction_candidates, format_correction_memory_content, is_correction_message,
    CorrectionMemoryDraft,
};
pub use proposal::{
    apply_memory_entry, build_memory_entry_from_proposal_item, default_scope_for_category,
    evaluate_memory_proposal, evaluate_memory_proposal_from_session, extract_memory_candidates,
    linked_doc_path_for_category, slugify, sync_entry_to_markdown,
};
pub use retrieval::{
    build_relevant_memory_prefix, cosine_similarity, keyword_overlap_score, retrieve_entries,
};
pub use store::{
    current_unix_millis, new_entry_id, project_memory_db_path, user_memory_db_path,
    MemoryStoreState,
};
