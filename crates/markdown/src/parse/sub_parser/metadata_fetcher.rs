use std::time::Duration;
use scraper::{Html, Selector};
use tokio::time::timeout;
use reqwest::Client;

use crate::parse::sub_parser::link_metadata::{LinkMetadata, MetadataStatus};

pub struct MetadataFetcher {
    client: Client,
    timeout_duration: Duration,
}

impl MetadataFetcher {
    pub fn new() -> Self {
        Self::new_with_config(&crate::parse::config::MarkdownConfig::default())
    }

    pub fn new_with_config(config: &crate::parse::config::MarkdownConfig) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .user_agent(&config.link_card_user_agent)
                .build()
                .expect("Failed to create HTTP client"),
            timeout_duration: Duration::from_secs(config.link_card_timeout_seconds),
        }
    }

    pub async fn fetch_batch(&self, urls: Vec<String>) -> Vec<LinkMetadata> {
        let tasks: Vec<_> = urls
            .into_iter()
            .map(|url| self.fetch_single(url))
            .collect();

        futures::future::join_all(tasks).await
    }

    async fn fetch_single(&self, url: String) -> LinkMetadata {
        let result = timeout(self.timeout_duration, self.fetch_metadata(&url)).await;

        match result {
            Ok(Ok(metadata)) => metadata,
            Ok(Err(e)) => self.create_fallback_metadata(url, Some(e.to_string())),
            Err(_) => self.create_fallback_metadata(url, Some("Timeout".to_string())),
        }
    }

    async fn fetch_metadata(&self, url: &str) -> Result<LinkMetadata, Box<dyn std::error::Error + Send + Sync>> {
        let response = self.client.get(url).send().await?;
        
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()).into());
        }

        let html = response.text().await?;
        let metadata = self.parse_html_metadata(&html, url)?;
        
        Ok(LinkMetadata {
            url: url.to_string(),
            status: MetadataStatus::Success,
            title: metadata.title,
            description: metadata.description,
            image: metadata.image,
            error_message: None,
        })
    }

    fn parse_html_metadata(&self, html: &str, url: &str) -> Result<ParsedMetadata, Box<dyn std::error::Error + Send + Sync>> {
        use scraper::Html;
        
        let document = Html::parse_document(html);
        
        let title = self.extract_title(&document);
        let description = self.extract_description(&document);
        let image = self.extract_image(&document, url);

        Ok(ParsedMetadata {
            title,
            description,
            image,
        })
    }

    fn extract_title(&self, document: &Html) -> Option<String> {
        let og_title_selector = Selector::parse(r#"meta[property="og:title"]"#).ok()?;
        if let Some(element) = document.select(&og_title_selector).next() {
            if let Some(content) = element.value().attr("content") {
                return Some(content.to_string());
            }
        }

        let title_selector = Selector::parse("title").ok()?;
        document.select(&title_selector)
            .next()?
            .text()
            .collect::<String>()
            .trim()
            .to_string()
            .into()
    }

    fn extract_description(&self, document: &Html) -> Option<String> {
        let og_desc_selector = Selector::parse(r#"meta[property="og:description"]"#).ok()?;
        if let Some(element) = document.select(&og_desc_selector).next() {
            if let Some(content) = element.value().attr("content") {
                return Some(content.to_string());
            }
        }

        let meta_desc_selector = Selector::parse(r#"meta[name="description"]"#).ok()?;
        document.select(&meta_desc_selector)
            .next()?
            .value()
            .attr("content")?
            .to_string()
            .into()
    }

    fn extract_image(&self, document: &Html, base_url: &str) -> Option<String> {
        let og_image_selector = Selector::parse(r#"meta[property="og:image"]"#).ok()?;
        if let Some(element) = document.select(&og_image_selector).next() {
            if let Some(content) = element.value().attr("content") {
                return self.resolve_url(content, base_url);
            }
        }
        None
    }

    fn resolve_url(&self, url: &str, base_url: &str) -> Option<String> {
        if url.starts_with("http") {
            Some(url.to_string())
        } else if url.starts_with("/") {
            if let Ok(base) = reqwest::Url::parse(base_url) {
                if let Ok(resolved) = base.join(url) {
                    return Some(resolved.to_string());
                }
            }
            None
        } else {
            None
        }
    }

    fn create_fallback_metadata(&self, url: String, error_message: Option<String>) -> LinkMetadata {
        LinkMetadata {
            url: url.clone(),
            status: MetadataStatus::Fallback,
            title: Some(url),
            description: None,
            image: None,
            error_message,
        }
    }
}

#[derive(Debug)]
struct ParsedMetadata {
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
}