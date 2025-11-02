pub mod builder;
pub mod config;
mod control;
mod full_service;
mod sub_parser;

use loss72_platemaker_core::model::Article;
use loss72_platemaker_structure::ArticleFile;

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error(
        "The file is at the invalid location. Expected markdown files to be placed at `./$year/$month/$day[-$num]_$slug.md`."
    )]
    InvalidStructure,

    #[error("The file is at the path where cannot be represented in UTF-8.")]
    InvalidPath,

    #[error(
        "No TOML frontmatter was found. Write TOML frontmatter wrapped with `+++` at the top of the markdown content."
    )]
    NoFrontmatter,

    #[error("The frontmatter could not be parsed or not valid metadata:\n{0}")]
    InvalidToml(String),
}

pub fn make_article_from_markdown(file: &ArticleFile) -> ParseResult<Article> {
    let config = crate::MarkdownConfig::default();
    let result = crate::parse_markdown_with_link_cards(file, Some(config))
        .map_err(|e| ParseError::InvalidToml(e.to_string()))?;
    Ok(result)
}
