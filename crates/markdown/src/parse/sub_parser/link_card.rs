use pulldown_cmark::{Event, Tag, TagEnd};
use tokio::runtime::Handle;
use std::sync::Arc;

use crate::parse::{control::*, sub_parser::{metadata_fetcher::MetadataFetcher, link_metadata::{LinkMetadata, MetadataStatus}}};
use super::SubParser;

struct LinkCardParserInfo<'p> {
    url: String,
    title: Option<String>,
    events: Vec<Event<'p>>,
}

#[derive(Default)]
pub struct LinkCardParser<'p> {
    pending_links: Vec<LinkCardParserInfo<'p>>,
    building_link: Option<LinkCardParserInfo<'p>>,
    metadata_fetcher: Option<Arc<MetadataFetcher>>,
}

impl<'p> LinkCardParser<'p> {
    pub fn with_metadata_fetcher(fetcher: Arc<MetadataFetcher>) -> Self {
        Self {
            pending_links: Vec::new(),
            building_link: None,
            metadata_fetcher: Some(fetcher),
        }
    }

    fn generate_card_html(&self, link: &LinkCardParserInfo, metadata: Option<&LinkMetadata>) -> String {
        let (title, description, image, css_class) = match metadata {
            Some(meta) => {
                let title = meta.title.as_ref()
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
                    }
                )
            },
            None => {
                let title = link.title.as_ref().unwrap_or(&link.url);
                (title, "", None, "link-card link-card-simple")
            },
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
        if let Some(link) = self.building_link.take() {
            self.pending_links.push(link);
        }

        if self.pending_links.is_empty() {
            return None;
        }

        let mut events = vec![];
        
        if let Some(fetcher) = &self.metadata_fetcher {
            if let Ok(handle) = Handle::try_current() {
                let urls: Vec<String> = self.pending_links.iter().map(|link| link.url.clone()).collect();
                let metadata_results = handle.block_on(fetcher.fetch_batch(urls));

                for (link, metadata) in self.pending_links.iter().zip(metadata_results.iter()) {
                    let card_html = self.generate_card_html(link, Some(metadata));
                    events.push(Event::Html(card_html.into()));
                }
            } else {
                for link in &self.pending_links {
                    let card_html = self.generate_card_html(link, None);
                    events.push(Event::Html(card_html.into()));
                }
            }
        } else {
            for link in &self.pending_links {
                let card_html = self.generate_card_html(link, None);
                events.push(Event::Html(card_html.into()));
            }
        }

        Some(events)
    }

    fn receive_event(&mut self, event: &Event<'p>) -> EventProcessControl<'p> {
        if self.building_link.is_some() {
            self.process_building_link_event(event);
            return discard();
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
    fn process_building_link_event(&mut self, event: &Event<'p>) {
        let should_end_link = matches!(event, Event::End(TagEnd::Link));
        
        if let Some(ref mut building_link) = self.building_link {
            match event {
                Event::Text(text) => {
                    if building_link.title.is_none() {
                        building_link.title = Some(text.to_string());
                    }
                }
                _ => {}
            }
            building_link.events.push(event.clone());
        }
        
        if should_end_link {
            if let Some(link) = self.building_link.take() {
                self.pending_links.push(link);
            }
        }
    }

    fn should_create_card(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        
        // 外部リンクの開始イベント
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
        
        // リンク開始
        let _ = parser.receive_event(&Event::Start(Tag::Link {
            link_type: pulldown_cmark::LinkType::Inline,
            dest_url: CowStr::Borrowed("https://example.com"),
            title: CowStr::Borrowed(""),
            id: CowStr::Borrowed(""),
        }));
        
        // リンクテキスト
        let _ = parser.receive_event(&Event::Text(CowStr::Borrowed("Example Site")));
        
        // リンク終了
        let _ = parser.receive_event(&Event::End(TagEnd::Link));
        
        // 最終処理
        let events = parser.finalize();
        assert!(events.is_some());
        
        let events = events.unwrap();
        assert_eq!(events.len(), 1);
        
        if let Event::Html(html) = &events[0] {
            assert!(html.contains("https://example.com"));
            assert!(html.contains("Example Site"));
            assert!(html.contains("link-card"));
        } else {
            panic!("Expected HTML event");
        }
    }
}