use std::ops::Range;
use std::sync::Arc;

use common::RenderedProfile;
use gpui::{
    AnyElement, App, ElementId, HighlightStyle, InteractiveText, IntoElement, SharedString,
    StyledText, UnderlineStyle, Window,
};
use gpui_component::ActiveTheme;
use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use person::PersonRegistry;
use regex::Regex;

use crate::RoomEvent;

static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:^|\s)(?:https?://)?(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}(?::\d+)?(?:/[^\s]*)?(?:\s|$)").unwrap()
});

static NOSTR_URI_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"nostr:(npub|note|nprofile|nevent|naddr)[a-zA-Z0-9]+").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Highlight {
    Link,
    Nostr,
}

#[derive(Default)]
pub struct RenderedText {
    pub text: SharedString,
    pub highlights: Vec<(Range<usize>, Highlight)>,
    pub link_ranges: Vec<Range<usize>>,
    pub link_urls: Arc<[String]>,
}

impl RenderedText {
    pub fn new(content: &str, cx: &App) -> Self {
        let mut text = String::new();
        let mut highlights = Vec::new();
        let mut link_ranges = Vec::new();
        let mut link_urls = Vec::new();

        render_plain_text_mut(
            content,
            &mut text,
            &mut highlights,
            &mut link_ranges,
            &mut link_urls,
            cx,
        );

        text.truncate(text.trim_end().len());

        RenderedText {
            text: SharedString::from(text),
            link_urls: link_urls.into(),
            link_ranges,
            highlights,
        }
    }

    pub fn element(&self, id: ElementId, window: &Window, cx: &App) -> AnyElement {
        let link_color = cx.theme().primary_foreground;

        InteractiveText::new(
            id,
            StyledText::new(self.text.clone()).with_default_highlights(
                &window.text_style(),
                self.highlights.iter().map(|(range, highlight)| {
                    (
                        range.clone(),
                        match highlight {
                            Highlight::Link => HighlightStyle {
                                color: Some(link_color),
                                underline: Some(UnderlineStyle::default()),
                                ..Default::default()
                            },
                            Highlight::Nostr => HighlightStyle {
                                color: Some(link_color),
                                ..Default::default()
                            },
                        },
                    )
                }),
            ),
        )
        .on_click(self.link_ranges.clone(), {
            let link_urls = self.link_urls.clone();
            move |ix, window, cx| {
                let token = link_urls[ix].as_str();

                if let Some(clean_url) = token.strip_prefix("nostr:") {
                    if let Ok(public_key) = PublicKey::parse(clean_url) {
                        window.dispatch_action(Box::new(RoomEvent::View(public_key)), cx);
                    }
                } else if is_url(token) {
                    let url = if token.starts_with("http") {
                        token.to_string()
                    } else {
                        format!("https://{token}")
                    };
                    cx.open_url(&url);
                } else {
                    log::warn!("Unrecognized token {token}")
                }
            }
        })
        .into_any_element()
    }
}

fn render_plain_text_mut(
    content: &str,
    text: &mut String,
    highlights: &mut Vec<(Range<usize>, Highlight)>,
    link_ranges: &mut Vec<Range<usize>>,
    link_urls: &mut Vec<String>,
    cx: &App,
) {
    // Copy the content directly
    text.push_str(content);

    // Collect all URLs
    let mut url_matches: Vec<(Range<usize>, String)> = Vec::new();

    for link in URL_REGEX.find_iter(content) {
        let range = link.start()..link.end();
        let url = link.as_str().to_string();

        url_matches.push((range, url));
    }

    // Collect all nostr entities with nostr: prefix
    let mut nostr_matches: Vec<(Range<usize>, String)> = Vec::new();

    for nostr_match in NOSTR_URI_REGEX.find_iter(content) {
        let range = nostr_match.start()..nostr_match.end();
        let nostr_uri = nostr_match.as_str().to_string();

        // Check if this nostr URI overlaps with any already processed URL
        if !url_matches
            .iter()
            .any(|(url_range, _)| url_range.start < range.end && range.start < url_range.end)
        {
            nostr_matches.push((range, nostr_uri));
        }
    }

    // Combine all matches for processing from end to start
    let mut all_matches = Vec::new();
    all_matches.extend(url_matches);
    all_matches.extend(nostr_matches);

    // Sort by position (end to start) to avoid changing positions when replacing text
    all_matches.sort_by(|(range_a, _), (range_b, _)| range_b.start.cmp(&range_a.start));

    // Process all matches
    for (range, entity) in all_matches {
        // Handle URL token
        if is_url(&entity) {
            highlights.push((range.clone(), Highlight::Link));
            link_ranges.push(range);
            link_urls.push(entity);
            continue;
        };

        if let Ok(nip21) = Nip21::parse(&entity) {
            match nip21 {
                Nip21::Pubkey(public_key) => {
                    render_pubkey(
                        public_key,
                        text,
                        &range,
                        highlights,
                        link_ranges,
                        link_urls,
                        cx,
                    );
                }
                Nip21::Profile(nip19_profile) => {
                    render_pubkey(
                        nip19_profile.public_key,
                        text,
                        &range,
                        highlights,
                        link_ranges,
                        link_urls,
                        cx,
                    );
                }
                Nip21::EventId(event_id) => {
                    render_bech32(
                        event_id.to_bech32().unwrap(),
                        text,
                        &range,
                        highlights,
                        link_ranges,
                        link_urls,
                    );
                }
                Nip21::Event(nip19_event) => {
                    render_bech32(
                        nip19_event.to_bech32().unwrap(),
                        text,
                        &range,
                        highlights,
                        link_ranges,
                        link_urls,
                    );
                }
                Nip21::Coordinate(nip19_coordinate) => {
                    render_bech32(
                        nip19_coordinate.to_bech32().unwrap(),
                        text,
                        &range,
                        highlights,
                        link_ranges,
                        link_urls,
                    );
                }
            }
        }
    }
}

/// Check if a string is a URL
fn is_url(s: &str) -> bool {
    URL_REGEX.is_match(s)
}

/// Format a bech32 entity with ellipsis and last 4 characters
fn format_shortened_entity(entity: &str) -> String {
    let prefix_end = entity.find('1').unwrap_or(0);

    if prefix_end > 0 && entity.len() > prefix_end + 5 {
        let prefix = &entity[0..=prefix_end]; // Include the '1'
        let suffix = &entity[entity.len() - 4..]; // Last 4 chars

        format!("{prefix}...{suffix}")
    } else {
        entity.to_string()
    }
}

fn render_pubkey(
    public_key: PublicKey,
    text: &mut String,
    range: &Range<usize>,
    highlights: &mut Vec<(Range<usize>, Highlight)>,
    link_ranges: &mut Vec<Range<usize>>,
    link_urls: &mut Vec<String>,
    cx: &App,
) {
    let persons = PersonRegistry::global(cx);
    let profile = persons.read(cx).get_person(&public_key, cx);
    let display_name = format!("@{}", profile.display_name());

    text.replace_range(range.clone(), &display_name);

    let new_length = display_name.len();
    let length_diff = new_length as isize - (range.end - range.start) as isize;
    let new_range = range.start..(range.start + new_length);

    highlights.push((new_range.clone(), Highlight::Nostr));
    link_ranges.push(new_range);
    link_urls.push(format!("nostr:{}", profile.public_key().to_hex()));

    if length_diff != 0 {
        adjust_ranges(highlights, link_ranges, range.end, length_diff);
    }
}

fn render_bech32(
    bech32: String,
    text: &mut String,
    range: &Range<usize>,
    highlights: &mut Vec<(Range<usize>, Highlight)>,
    link_ranges: &mut Vec<Range<usize>>,
    link_urls: &mut Vec<String>,
) {
    let njump_url = format!("https://njump.me/{bech32}");
    let shortened_entity = format_shortened_entity(&bech32);
    let display_text = format!("https://njump.me/{shortened_entity}");

    text.replace_range(range.clone(), &display_text);

    let new_length = display_text.len();
    let length_diff = new_length as isize - (range.end - range.start) as isize;
    let new_range = range.start..(range.start + new_length);

    highlights.push((new_range.clone(), Highlight::Link));
    link_ranges.push(new_range);
    link_urls.push(njump_url);

    if length_diff != 0 {
        adjust_ranges(highlights, link_ranges, range.end, length_diff);
    }
}

// Helper function to adjust ranges when text length changes
fn adjust_ranges(
    highlights: &mut [(Range<usize>, Highlight)],
    link_ranges: &mut [Range<usize>],
    position: usize,
    length_diff: isize,
) {
    // Adjust highlight ranges
    for (range, _) in highlights.iter_mut() {
        if range.start > position {
            range.start = (range.start as isize + length_diff) as usize;
            range.end = (range.end as isize + length_diff) as usize;
        }
    }

    // Adjust link ranges
    for range in link_ranges.iter_mut() {
        if range.start > position {
            range.start = (range.start as isize + length_diff) as usize;
            range.end = (range.end as isize + length_diff) as usize;
        }
    }
}
