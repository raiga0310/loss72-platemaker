use std::sync::Arc;
use pulldown_cmark::Options;

use super::{
    config::MarkdownConfig,
    full_service::{MarkdownParser, MarkdownParseResult},
    sub_parser::{SubParsers, metadata_fetcher::MetadataFetcher},
};

/// MarkdownParserを構築するためのビルダー
pub struct MarkdownParserBuilder {
    config: MarkdownConfig,
    metadata_fetcher: Option<Arc<MetadataFetcher>>,
}

impl MarkdownParserBuilder {
    /// 新しいビルダーを作成
    pub fn new() -> Self {
        Self {
            config: MarkdownConfig::default(),
            metadata_fetcher: None,
        }
    }

    pub fn with_config(config: MarkdownConfig) -> Self {
        let metadata_fetcher = if config.enable_link_cards {
            Some(Arc::new(MetadataFetcher::new_with_config(&config)))
        } else {
            None
        };

        Self {
            config,
            metadata_fetcher,
        }
    }

    pub fn enable_link_cards(mut self) -> Self {
        self.config.enable_link_cards = true;
        if self.metadata_fetcher.is_none() {
            self.metadata_fetcher = Some(Arc::new(MetadataFetcher::new_with_config(&self.config)));
        }
        self
    }

    pub fn disable_link_cards(mut self) -> Self {
        self.config.enable_link_cards = false;
        self.metadata_fetcher = None;
        self
    }

    pub fn with_metadata_fetcher(mut self, fetcher: Arc<MetadataFetcher>) -> Self {
        self.config.enable_link_cards = true;
        self.metadata_fetcher = Some(fetcher);
        self
    }

    pub fn with_parser_options(mut self, options: Options) -> Self {
        self.config.parser_options = options;
        self
    }

    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.config.link_card_timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_user_agent<S: Into<String>>(mut self, user_agent: S) -> Self {
        self.config.link_card_user_agent = user_agent.into();
        self
    }

    pub fn build<'p>(self, content: &'p str) -> MarkdownParser<'p> {
        let sub_parsers = SubParsers::with_metadata_fetcher(self.metadata_fetcher);
        MarkdownParser::new_with_sub_parsers(content, self.config.parser_options, sub_parsers)
    }

    pub fn parse<'p>(self, content: &'p str) -> MarkdownParseResult {
        self.build(content).run()
    }
}

impl Default for MarkdownParserBuilder {
    fn default() -> Self {
        Self::new()
    }
}
