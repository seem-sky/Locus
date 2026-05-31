use uuid::Uuid;

pub fn current_unix_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn new_entry_id() -> String {
    format!("mem_{}", Uuid::new_v4())
}

pub fn project_memory_db_path(working_dir: &str) -> std::path::PathBuf {
    std::path::Path::new(working_dir)
        .join("Locus/memory")
        .join("entries.db")
}

pub fn user_memory_db_path(app_storage_dir: &std::path::Path) -> std::path::PathBuf {
    app_storage_dir.join("memory").join("entries.db")
}

pub use crate::agentmemory::AgentMemoryState as MemoryStoreState;
