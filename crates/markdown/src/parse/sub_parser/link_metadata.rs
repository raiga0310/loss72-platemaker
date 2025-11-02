use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetadataStatus {
    Success,
    Error,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkMetadata {
    pub url: String,
    pub status: MetadataStatus,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub error_message: Option<String>,
}
