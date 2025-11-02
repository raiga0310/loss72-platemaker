use pulldown_cmark::{Event, Tag, TagEnd};
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
            .map(|img| format!(r#"<img src="{}" alt="" class="link-card-image">"#, img))
            .unwrap_or_default();

        format!(
            r#"<div class="{}">
                <a href="{}" target="_blank" rel="noopener">
                    {}
                    <div class="link-card-content">
                        <h3>{}</h3>
                        <p>{}</p>
                        <span class="link-url">{}</span>
                    </div>
                </a>
            </div>"#,
            css_class, link.url, image_html, title, description, link.url
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
            Event::Start(Tag::Link { dest_url: url, .. }) => {
                if self.should_create_card(url) {
                    self.building_link = Some(LinkCardParserInfo {
                        url: url.to_string(),
                        title: None,
                        events: vec![],
                    });
                    discard()
                } else {
                    use_next()
                }
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
                // リンクが完成したらすぐにHTMLに変換して返す
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
            }
        }

        let card_html = self.generate_card_html(&link, None);
        use_html(card_html.into())
    }

    fn should_create_card(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::control::{BreakingEventProcess, EventProcessControl};
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
    fn test_external_link_processing() {
        let mut parser = LinkCardParser::default();

        let link_event = Event::Start(Tag::Link {
            link_type: pulldown_cmark::LinkType::Inline,
            dest_url: CowStr::Borrowed("https://example.com"),
            title: CowStr::Borrowed(""),
            id: CowStr::Borrowed(""),
        });

        let control = parser.receive_event(&link_event);
        assert!(matches!(control, EventProcessControl::Break(_)));
        assert!(parser.building_link.is_some());
    }

    #[test]
    fn test_internal_link_passthrough() {
        let mut parser = LinkCardParser::default();

        let link_event = Event::Start(Tag::Link {
            link_type: pulldown_cmark::LinkType::Inline,
            dest_url: CowStr::Borrowed("/internal"),
            title: CowStr::Borrowed(""),
            id: CowStr::Borrowed(""),
        });

        let control = parser.receive_event(&link_event);
        assert!(matches!(control, EventProcessControl::Continue(_)));
        assert!(parser.building_link.is_none());
    }

    #[test]
    fn test_full_link_processing() {
        let mut parser = LinkCardParser::default();

        let _ = parser.receive_event(&Event::Start(Tag::Link {
            link_type: pulldown_cmark::LinkType::Inline,
            dest_url: CowStr::Borrowed("https://example.com"),
            title: CowStr::Borrowed(""),
            id: CowStr::Borrowed(""),
        }));

        let _ = parser.receive_event(&Event::Text(CowStr::Borrowed("Example Site")));

        let control = parser.receive_event(&Event::End(TagEnd::Link));

        if let EventProcessControl::Break(BreakingEventProcess::UseThisInstead(Event::Html(html))) =
            control
        {
            assert!(html.contains("https://example.com"));
            assert!(html.contains("Example Site"));
            assert!(html.contains("link-card"));
        } else {
            panic!("Expected HTML event");
        }

        // finalizeは何も返さない
        let events = parser.finalize();
        assert!(events.is_none());
    }
}
