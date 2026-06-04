use std::time::Duration;

use reqwest::blocking::Client;

#[derive(Debug, Clone)]
pub struct HeadroomProxyClient {
    base_url: String,
    http: Client,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomProxyHealth {
    pub available: bool,
    pub status: String,
    pub error: Option<String>,
}

impl HeadroomProxyClient {
    pub fn from_settings() -> Self {
        Self::new(crate::headroom::settings::base_url())
    }

    pub fn new(base_url: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn health(&self) -> HeadroomProxyHealth {
        for path in ["/livez", "/health"] {
            match self.probe(path) {
                Ok(()) => {
                    return HeadroomProxyHealth {
                        available: true,
                        status: "healthy".to_string(),
                        error: None,
                    };
                }
                Err(error) => {
                    if path == "/health" {
                        return HeadroomProxyHealth {
                            available: false,
                            status: "unavailable".to_string(),
                            error: Some(error),
                        };
                    }
                }
            }
        }
        HeadroomProxyHealth {
            available: false,
            status: "unavailable".to_string(),
            error: Some("headroom proxy health probe failed".to_string()),
        }
    }

    fn probe(&self, path: &str) -> Result<(), String> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .get(&url)
            .send()
            .map_err(|error| format!("headroom proxy probe failed for {url}: {error}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "headroom proxy probe {} returned {}",
                url,
                response.status()
            ))
        }
    }
}
