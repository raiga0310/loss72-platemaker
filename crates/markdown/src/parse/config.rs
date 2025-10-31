use pulldown_cmark::Options;

/// MarkdownParserの設定
#[derive(Debug, Clone)]
pub struct MarkdownConfig {
    pub enable_link_cards: bool,
    pub link_card_timeout_seconds: u64,
    pub link_card_user_agent: String,
    pub parser_options: Options,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            enable_link_cards: true,
            link_card_timeout_seconds: 10,
            link_card_user_agent: "loss72-platemaker/1.0".to_string(),
            parser_options: Options::empty(),
        }
    }
}