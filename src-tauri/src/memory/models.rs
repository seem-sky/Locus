use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    User,
    Feedback,
    Topic,
    Reference,
}

impl MemoryCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Feedback => "feedback",
            Self::Topic => "topic",
            Self::Reference => "reference",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "user" => Some(Self::User),
            "feedback" => Some(Self::Feedback),
            "topic" => Some(Self::Topic),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::User, Self::Feedback, Self::Topic, Self::Reference]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Project,
    User,
}

impl MemoryScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::User => "user",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "project" => Some(Self::Project),
            "user" => Some(Self::User),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub category: MemoryCategory,
    pub scope: MemoryScope,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub pinned: bool,
    pub pin_weight: i32,
    pub access_count: u32,
    pub last_accessed_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_doc_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryPatch {
    pub category: Option<MemoryCategory>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub pinned: Option<bool>,
    pub pin_weight: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryListFilter {
    pub category: Option<MemoryCategory>,
    pub scope: Option<MemoryScope>,
    pub tags: Option<Vec<String>>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRetrieveHit {
    pub entry: MemoryEntry,
    pub score: f32,
    pub keyword_score: f32,
    pub semantic_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRetrieveOptions {
    pub query: String,
    pub limit: Option<usize>,
    pub token_budget: Option<usize>,
    pub scopes: Option<Vec<MemoryScope>>,
}

pub const DEFAULT_PIN_WEIGHT: i32 = 100;
pub const DEFAULT_RETRIEVE_LIMIT: usize = 12;
pub const DEFAULT_TOKEN_BUDGET: usize = 800;
