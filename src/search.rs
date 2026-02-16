use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineModel {
    pub name: String,
    pub description: Option<String>,
    pub url: String,
}

pub struct ModelSearch {
    client: reqwest::blocking::Client,
}

impl ModelSearch {
    pub fn new() -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .user_agent("ollama-cli/0.1.0")
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn search_online(&self, query: &str) -> anyhow::Result<Vec<OnlineModel>> {
        let url = "https://ollama.com/library";
        let response = self.client.get(url).send()?;

        let body = response.text()?;

        let mut models = Vec::new();
        let pattern = format!("{}/library/", "https://ollama.com");

        for line in body.lines() {
            if line.contains("/library/") && line.contains("<a ") {
                if let Some(name) = extract_model_name(line, &pattern) {
                    if query.is_empty() || name.to_lowercase().contains(&query.to_lowercase()) {
                        let model_url = format!("{}/library/{}", "https://ollama.com", name);
                        models.push(OnlineModel {
                            name: name.clone(),
                            description: None,
                            url: model_url,
                        });
                    }
                }
            }
        }

        let mut unique: std::collections::HashSet<String> = std::collections::HashSet::new();
        models.retain(|m| unique.insert(m.name.clone()));

        models.truncate(50);

        Ok(models)
    }

    pub fn get_popular_models(&self) -> anyhow::Result<Vec<OnlineModel>> {
        let url = "https://ollama.com/library?sort=popular";
        let response = self.client.get(url).send()?;

        let body = response.text()?;

        let mut models = Vec::new();
        let pattern = format!("{}/library/", "https://ollama.com");

        for line in body.lines() {
            if line.contains("/library/") && line.contains("<a ") {
                if let Some(name) = extract_model_name(line, &pattern) {
                    let model_url = format!("{}/library/{}", "https://ollama.com", name);
                    models.push(OnlineModel {
                        name: name.clone(),
                        description: None,
                        url: model_url,
                    });
                }
            }
        }

        let mut unique: std::collections::HashSet<String> = std::collections::HashSet::new();
        models.retain(|m| unique.insert(m.name.clone()));

        models.truncate(30);

        Ok(models)
    }
}

fn extract_model_name(line: &str, _base_url: &str) -> Option<String> {
    let href_start = line.find("href=\"")? + 6;
    let href_end = line[href_start..].find('"')? + href_start;
    let href = &line[href_start..href_end];

    if !href.starts_with("/library/") {
        return None;
    }

    let name = href.trim_start_matches("/library/");
    if name.is_empty() {
        return None;
    }

    if name.contains('/') || name.contains('?') {
        return None;
    }

    Some(name.to_string())
}

impl Default for ModelSearch {
    fn default() -> Self {
        Self::new()
    }
}
