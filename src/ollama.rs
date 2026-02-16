use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};

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

    pub async fn delete_model(&self, name: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/delete", self.base_url);
        let request = DeleteRequest {
            name: name.to_string(),
        };
        self.client.delete(&url).json(&request).send().await?;
        Ok(())
    }

    pub fn chat_streaming<F>(model: String, messages: Vec<ChatMessage>, callback: F) -> std::thread::JoinHandle<anyhow::Result<String>>
    where
        F: Fn(String) + Send + 'static,
    {
        let base_url = "http://localhost:11434".to_string();
        
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::new();
            let url = format!("{}/api/chat", base_url);
            
            let request = ChatRequest {
                model,
                messages,
                stream: true,
            };
            
            let response = client.post(&url).json(&request).send()?;
            
            let reader = BufReader::new(response);
            let mut content = String::new();
            
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                
                if let Ok(resp) = serde_json::from_str::<ChatResponse>(&line) {
                    content.push_str(&resp.message.content);
                    callback(content.clone());
                    
                    if resp.done {
                        break;
                    }
                }
            }
            
            Ok(content)
        })
    }
}
