use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub model: String,
    pub size: i64,
    pub digest: String,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub message: ChatMessage,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub name: String,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub status: Option<String>,
    pub digest: Option<String>,
}

pub struct OllamaClient {
    base_url: String,
    client: reqwest::Client,
}

impl OllamaClient {
    pub fn new(base_url: Option<String>) -> Self {
        let base_url = base_url.unwrap_or_else(|| "http://localhost:11434".to_string());
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn list_models(&self) -> anyhow::Result<ListResponse> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await?;
        let models: ListResponse = response.json().await?;
        Ok(models)
    }

    pub async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> anyhow::Result<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
        };
        let response = self.client.post(&url).json(&request).send().await?;
        let chat_response: ChatResponse = response.json().await?;
        Ok(chat_response)
    }

    pub async fn delete_model(&self, name: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/delete", self.base_url);
        let request = DeleteRequest { name: name.to_string() };
        self.client.delete(&url).json(&request).send().await?;
        Ok(())
    }

    pub async fn pull_model(&self, name: &str) -> anyhow::Result<PullResponse> {
        let url = format!("{}/api/pull", self.base_url);
        let request = PullRequest {
            name: name.to_string(),
            stream: false,
        };
        let response = self.client.post(&url).json(&request).send().await?;
        let pull_response: PullResponse = response.json().await?;
        Ok(pull_response)
    }
}
