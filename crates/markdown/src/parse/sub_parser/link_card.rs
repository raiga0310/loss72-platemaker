use pulldown_cmark::{Event, TagEnd};
use std::sync::Arc;
use tokio::runtime::Handle;

use super::SubParser;
use crate::parse::{
    control::*,
    sub_parser::{
        link_metadata::{LinkMetadata, MetadataStatus},
        metadata_fetcher::MetadataFetcher,
    },
};

struct LinkCardParserInfo<'p> {
    url: String,
    title: Option<String>,
    events: Vec<Event<'p>>,
}

#[derive(Default)]
pub struct LinkCardParser<'p> {
    building_link: Option<LinkCardParserInfo<'p>>,
    metadata_fetcher: Option<Arc<MetadataFetcher>>,
}

impl LinkCardParser<'_> {
    pub fn with_metadata_fetcher(fetcher: Arc<MetadataFetcher>) -> Self {
        Self {
            building_link: None,
            metadata_fetcher: Some(fetcher),
        }
    }

    fn generate_card_html(
        &self,
        link: &LinkCardParserInfo,
        metadata: Option<&LinkMetadata>,
    ) -> String {
        let (title, description, image, css_class) = match metadata {
            Some(meta) => {
                let title = meta
                    .title
                    .as_ref()
                    .or(link.title.as_ref())
                    .unwrap_or(&link.url);

                (
                    title,
                    meta.description.as_deref().unwrap_or(""),
                    meta.image.as_deref(),
                    match meta.status {
                        MetadataStatus::Success => "link-card",
                        MetadataStatus::Fallback => "link-card link-card-fallback",
                        MetadataStatus::Error => "link-card link-card-error",
                    },
                )
            }
            None => {
                let title = link.title.as_ref().unwrap_or(&link.url);
                (title, "", None, "link-card link-card-simple")
            }
        };

        let image_html = image
            .map(|img| {
                format!(
                    r#"<div class="link-card-img"><img src="{}" alt="サイト画像" class="link-card-img-element"></div>"#,
                    img
                )
            })
            .unwrap_or_else(|| {
                r#"<div class="link-card-img"><div class="link-card-img-placeholder">画像なし</div></div>"#.to_string()
            });

        format!(
            r#"<a href="{}" target="_blank" rel="noopener" style="text-decoration: none; color: inherit;">
<div class="{}">
  <div class="link-card-container">
    {}
    <div class="link-card-statement">
      <div class="link-card-header">
        <div class="link-card-title">{}</div>
        <div class="link-card-url">{}</div>
      </div>
      <div class="link-card-description">{}</div>
    </div>
  </div>
</div>
</a>"#,
            link.url, css_class, image_html, title, link.url, description
        )
    }
}

impl<'p> SubParser<'p> for LinkCardParser<'p> {
    type Output = ();

    fn finalize(&mut self) -> Option<Vec<Event<'p>>> {
        None
    }

    fn receive_event(&mut self, event: &Event<'p>) -> EventProcessControl<'p> {
        if self.building_link.is_some() {
            return self.process_building_link_event(event);
        }

        match event {
            Event::Html(html) => {
                if let Some(url) = self.extract_url_from_text(html) {
                    if self.should_create_card(&url) {
                        // URLをリンクカードに変換して即座に返す
                        let link_info = LinkCardParserInfo {
                            url: url.clone(),
                            title: Some(url.clone()),
                            events: vec![],
                        };
                        return self.convert_link_to_html(link_info);
                    }
                }
                use_next()
            }
            _ => use_next(),
        }
    }

    fn compose_output(self) -> Self::Output {}
}

impl<'p> LinkCardParser<'p> {
    fn process_building_link_event(&mut self, event: &Event<'p>) -> EventProcessControl<'p> {
        let should_end_link = matches!(event, Event::End(TagEnd::Link));

        if let Some(ref mut building_link) = self.building_link {
            if let Event::Text(text) = event {
                if building_link.title.is_none() {
                    building_link.title = Some(text.to_string());
                }
            }
            building_link.events.push(event.clone());
        }

        if should_end_link {
            if let Some(link) = self.building_link.take() {
                return self.convert_link_to_html(link);
            }
        }

        discard()
    }

    fn convert_link_to_html(&self, link: LinkCardParserInfo<'p>) -> EventProcessControl<'p> {
        if let Some(fetcher) = &self.metadata_fetcher {
            if let Ok(handle) = Handle::try_current() {
                let metadata_result =
                    handle.block_on(fetcher.fetch_single_metadata(link.url.clone()));

                let card_html = self.generate_card_html(&link, Some(&metadata_result));
                return use_html(card_html.into());
            } else {
                // tokio runtimeがない場合は、新しいランタイムを作成
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                if let Ok(rt) = rt {
                    let metadata_result =
                        rt.block_on(fetcher.fetch_single_metadata(link.url.clone()));

                    let card_html = self.generate_card_html(&link, Some(&metadata_result));
                    return use_html(card_html.into());
                }
            }
        }

        // フォールバック: メタデータなしでHTML生成
        let card_html = self.generate_card_html(&link, None);
        use_html(card_html.into())
    }

    fn should_create_card(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }

    fn extract_url_from_text(&self, text: &str) -> Option<String> {
        // テキストが `https\://` または `http\://` で始まり、単体のURLの場合のみ抽出
        let trimmed = text.trim();
        if (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
            && !trimmed.contains(' ')
            && trimmed.len() > 8
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::control::EventProcessControl;
    use pulldown_cmark::CowStr;

    #[test]
    fn test_should_create_card() {
        let parser = LinkCardParser::default();

        assert!(parser.should_create_card("https://example.com"));
        assert!(parser.should_create_card("http://example.com"));
        assert!(!parser.should_create_card("/internal/link"));
        assert!(!parser.should_create_card("mailto:test@example.com"));
    }

    #[test]
    fn test_mixed_text_passthrough() {
        let mut parser = LinkCardParser::default();

        let text_event = Event::Text(CowStr::Borrowed("Visit https://example.com for more info"));
        let control = parser.receive_event(&text_event);
        assert!(matches!(control, EventProcessControl::Continue(_)));

        let text_event = Event::Text(CowStr::Borrowed("Just some text"));
        let control = parser.receive_event(&text_event);
        assert!(matches!(control, EventProcessControl::Continue(_)));
    }
}
