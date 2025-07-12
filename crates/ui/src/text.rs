use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use common::display::DisplayProfile;
use gpui::{
    Action, AnyElement, AnyView, App, ElementId, FontWeight, HighlightStyle, InteractiveText,
    IntoElement, SharedString, StyledText, UnderlineStyle, Window,
};
use linkify::{LinkFinder, LinkKind};
use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use theme::ActiveTheme;

static NOSTR_URI_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"nostr:(npub|note|nprofile|nevent|naddr)[a-zA-Z0-9]+").unwrap());

static BECH32_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(npub|note|nprofile|nevent|naddr)[a-zA-Z0-9]+\b").unwrap());

#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = rich_text, no_json)]
pub struct OpenMention(String);

impl OpenMention {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Highlight {
    Highlight(HighlightStyle),
    Mention,
}

impl From<HighlightStyle> for Highlight {
    fn from(style: HighlightStyle) -> Self {
        Self::Highlight(style)
    }
}

type CustomRangeTooltipFn =
    Option<Arc<dyn Fn(usize, Range<usize>, &mut Window, &mut App) -> Option<AnyView>>>;

#[derive(Default)]
pub struct RichText {
    pub text: SharedString,
    pub highlights: Vec<(Range<usize>, Highlight)>,
    pub link_ranges: Vec<Range<usize>>,
    pub link_urls: Arc<[String]>,
    pub custom_ranges: Vec<Range<usize>>,
    custom_ranges_tooltip_fn: CustomRangeTooltipFn,
}

impl RichText {
    pub fn new(content: &str, profiles: &[Profile]) -> Self {
        let mut text = String::new();
        let mut highlights = Vec::new();
        let mut link_ranges = Vec::new();
        let mut link_urls = Vec::new();

        render_plain_text_mut(
            content,
            profiles,
            &mut text,
            &mut highlights,
            &mut link_ranges,
            &mut link_urls,
        );

        text.truncate(text.trim_end().len());

        RichText {
            text: SharedString::from(text),
            link_urls: link_urls.into(),
            link_ranges,
            highlights,
            custom_ranges: Vec::new(),
            custom_ranges_tooltip_fn: None,
        }
    }

    pub fn set_tooltip_builder_for_custom_ranges<F>(&mut self, f: F)
    where
        F: Fn(usize, Range<usize>, &mut Window, &mut App) -> Option<AnyView> + 'static,
    {
        self.custom_ranges_tooltip_fn = Some(Arc::new(f));
    }

    pub fn element(&self, id: ElementId, window: &mut Window, cx: &App) -> AnyElement {
        let link_color = cx.theme().text_accent;

        InteractiveText::new(
            id,
            StyledText::new(self.text.clone()).with_default_highlights(
                &window.text_style(),
                self.highlights.iter().map(|(range, highlight)| {
                    (
                        range.clone(),
                        match highlight {
                            Highlight::Highlight(highlight) => {
                                // Check if this is a link highlight by seeing if it has an underline
                                if highlight.underline.is_some() {
                                    // It's a link, so apply the link color
                                    let mut link_style = *highlight;
                                    link_style.color = Some(link_color);
                                    link_style
                                } else {
                                    *highlight
                                }
                            }
                            Highlight::Mention => HighlightStyle {
                                color: Some(link_color),
                                font_weight: Some(FontWeight::MEDIUM),
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
                let url = &link_urls[ix];

                if url.starts_with("http") {
                    cx.open_url(url);
                } else if url.starts_with("mention:") {
                    window.dispatch_action(Box::new(OpenMention(url.replace("mention:", ""))), cx);
                }
            }
        })
        .tooltip({
            let link_ranges = self.link_ranges.clone();
            let link_urls = self.link_urls.clone();
            let custom_tooltip_ranges = self.custom_ranges.clone();
            let custom_tooltip_fn = self.custom_ranges_tooltip_fn.clone();
            move |idx, window, cx| {
                for (ix, range) in link_ranges.iter().enumerate() {
                    if range.contains(&idx) {
                        let url = &link_urls[ix];
                        if url.starts_with("http") {
                            // return Some(LinkPreview::new(url, cx));
                        }
                        // You can add custom tooltip handling for mentions here
                    }
                }
                for range in &custom_tooltip_ranges {
                    if range.contains(&idx) {
                        if let Some(f) = &custom_tooltip_fn {
                            return f(idx, range.clone(), window, cx);
                        }
                    }
                }
                None
            }
        })
        .into_any_element()
    }
}

pub fn render_plain_text_mut(
    content: &str,
    profiles: &[Profile],
    text: &mut String,
    highlights: &mut Vec<(Range<usize>, Highlight)>,
    link_ranges: &mut Vec<Range<usize>>,
    link_urls: &mut Vec<String>,
) {
    // Copy the content directly
    text.push_str(content);

    // Create a profile lookup using PublicKey directly
    let profile_lookup: HashMap<PublicKey, Profile> = profiles
        .iter()
        .map(|profile| (profile.public_key(), profile.clone()))
        .collect();

    // Process regular URLs using linkify
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);

    // Collect all URLs
    let mut url_matches: Vec<(Range<usize>, String)> = Vec::new();

    for link in finder.links(content) {
        let start = link.start();
        let end = link.end();
        let range = start..end;
        let url = link.as_str().to_string();

        url_matches.push((range, url));
    }

    // Process nostr entities with nostr: prefix
    let mut nostr_matches: Vec<(Range<usize>, String)> = Vec::new();

    for nostr_match in NOSTR_URI_REGEX.find_iter(content) {
        let start = nostr_match.start();
        let end = nostr_match.end();
        let range = start..end;
        let nostr_uri = nostr_match.as_str().to_string();

        // Check if this nostr URI overlaps with any already processed URL
        if !url_matches
            .iter()
            .any(|(url_range, _)| url_range.start < range.end && range.start < url_range.end)
        {
            nostr_matches.push((range, nostr_uri));
        }
    }

    // Process raw bech32 entities (without nostr: prefix)
    let mut bech32_matches: Vec<(Range<usize>, String)> = Vec::new();

    for bech32_match in BECH32_REGEX.find_iter(content) {
        let start = bech32_match.start();
        let end = bech32_match.end();
        let range = start..end;
        let bech32_entity = bech32_match.as_str().to_string();

        // Check if this entity overlaps with any already processed matches
        let overlaps_with_url = url_matches
            .iter()
            .any(|(url_range, _)| url_range.start < range.end && range.start < url_range.end);

        let overlaps_with_nostr = nostr_matches
            .iter()
            .any(|(nostr_range, _)| nostr_range.start < range.end && range.start < nostr_range.end);

        if !overlaps_with_url && !overlaps_with_nostr {
            bech32_matches.push((range, bech32_entity));
        }
    }

    // Combine all matches for processing from end to start
    let mut all_matches = Vec::new();
    all_matches.extend(url_matches);
    all_matches.extend(nostr_matches);
    all_matches.extend(bech32_matches);

    // Sort by position (end to start) to avoid changing positions when replacing text
    all_matches.sort_by(|(range_a, _), (range_b, _)| range_b.start.cmp(&range_a.start));

    // Process all matches
    for (range, entity) in all_matches {
        if entity.starts_with("http") {
            // Regular URL
            highlights.push((
                range.clone(),
                Highlight::Highlight(HighlightStyle {
                    underline: Some(UnderlineStyle {
                        thickness: 1.0.into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            ));

            link_ranges.push(range);
            link_urls.push(entity);
        } else {
            let entity_without_prefix = if entity.starts_with("nostr:") {
                entity.strip_prefix("nostr:").unwrap_or(&entity)
            } else {
                &entity
            };

            // Try to find a matching profile if this is npub or nprofile
            let profile_match = if entity_without_prefix.starts_with("npub") {
                PublicKey::from_bech32(entity_without_prefix)
                    .ok()
                    .and_then(|pubkey| profile_lookup.get(&pubkey).cloned())
            } else if entity_without_prefix.starts_with("nprofile") {
                Nip19Profile::from_bech32(entity_without_prefix)
                    .ok()
                    .and_then(|profile| profile_lookup.get(&profile.public_key).cloned())
            } else {
                None
            };

            if let Some(profile) = profile_match {
                // Profile found - create a mention
                let display_name = format!("@{}", profile.display_name());

                // Replace mention with profile name
                text.replace_range(range.clone(), &display_name);

                // Adjust ranges
                let new_length = display_name.len();
                let length_diff = new_length as isize - (range.end - range.start) as isize;

                // New range for the replacement
                let new_range = range.start..(range.start + new_length);

                // Add highlight for the profile name
                highlights.push((new_range.clone(), Highlight::Mention));

                // Make it clickable
                link_ranges.push(new_range);
                link_urls.push(format!("mention:{}", profile.public_key().to_hex()));

                // Adjust subsequent ranges if needed
                if length_diff != 0 {
                    adjust_ranges(highlights, link_ranges, range.end, length_diff);
                }
            } else {
                // No profile match or not a profile entity - create njump.me link
                let njump_url = format!("https://njump.me/{entity_without_prefix}");

                // Create a shortened display format for the URL
                let shortened_entity = format_shortened_entity(entity_without_prefix);
                let display_text = format!("https://njump.me/{shortened_entity}");

                // Replace the original entity with the shortened display version
                text.replace_range(range.clone(), &display_text);

                // Adjust the ranges
                let new_length = display_text.len();
                let length_diff = new_length as isize - (range.end - range.start) as isize;

                // New range for the replacement
                let new_range = range.start..(range.start + new_length);

                // Add underline highlight
                highlights.push((
                    new_range.clone(),
                    Highlight::Highlight(HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: 1.0.into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                ));

                // Make it clickable
                link_ranges.push(new_range);
                link_urls.push(njump_url);

                // Adjust subsequent ranges if needed
                if length_diff != 0 {
                    adjust_ranges(highlights, link_ranges, range.end, length_diff);
                }
            }
        }
    }
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
