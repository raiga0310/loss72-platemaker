#![deny(clippy::unwrap_used)]

use loss72_platemaker_core::{fs::File, log, model::Article};
use loss72_platemaker_structure::ArticleFile;
use parse::{ParseError, make_article_from_markdown};

mod frontmatter;
mod parse;

pub use parse::{builder::MarkdownParserBuilder, config::MarkdownConfig};

#[derive(Debug, thiserror::Error)]
pub enum MarkdownProcessError {
    #[error("Error during I/O: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Error during parsing Markdown:\n{0}")]
    ParseError(ParseError),
}

pub fn is_markdown_path(file: &File) -> bool {
    file.path().extension().is_some_and(|ext| ext == "md")
}

pub fn parse_markdown(file: &ArticleFile) -> Result<Article, MarkdownProcessError> {
    log!(step: "Parsing ./{}", file.relative_path.display());

    make_article_from_markdown(file).map_err(MarkdownProcessError::ParseError)
}

pub fn parse_markdown_with_link_cards(
    file: &ArticleFile,
    config: Option<MarkdownConfig>,
) -> Result<Article, MarkdownProcessError> {
    log!(step: "Parsing ./{} with link cards", file.relative_path.display());

    let content = file.file().read_to_string()?;
    let config = config.unwrap_or_default();

    let builder = MarkdownParserBuilder::with_config(config);
    let parsed = builder.parse(&content);

    let metadata = frontmatter::parse_toml_to_metadata(
        parsed
            .frontmatter()
            .ok_or(MarkdownProcessError::ParseError(ParseError::NoFrontmatter))?,
    )
    .map_err(MarkdownProcessError::ParseError)?;

    Ok(Article {
        id: file.id.clone(),
        metadata,
        content: parsed.html().to_string(),
    })
}
