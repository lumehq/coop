use gpui::{
    AnyElement, AnyView, App, ElementId, FontWeight, HighlightStyle, InteractiveText, IntoElement,
    SharedString, StyledText, UnderlineStyle, Window,
};
use std::{ops::Range, sync::Arc};

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

#[derive(Clone, Default)]
pub struct RichText {
    pub text: SharedString,
    pub highlights: Vec<(Range<usize>, Highlight)>,
    pub link_ranges: Vec<Range<usize>>,
    pub link_urls: Arc<[String]>,
    pub custom_ranges: Vec<Range<usize>>,
    custom_ranges_tooltip_fn: CustomRangeTooltipFn,
}

impl RichText {
    pub fn new(content: String) -> Self {
        let mut text = String::new();
        let mut highlights = Vec::new();
        let mut link_ranges = Vec::new();
        let mut link_urls = Vec::new();

        render_plain_text_mut(
            &content,
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

    pub fn set_tooltip_builder_for_custom_ranges(
        &mut self,
        f: impl Fn(usize, Range<usize>, &mut Window, &mut App) -> Option<AnyView> + 'static,
    ) {
        self.custom_ranges_tooltip_fn = Some(Arc::new(f));
    }

    pub fn element(&self, id: ElementId, window: &mut Window, _cx: &App) -> AnyElement {
        InteractiveText::new(
            id,
            StyledText::new(self.text.clone()).with_default_highlights(
                &window.text_style(),
                self.highlights.iter().map(|(range, highlight)| {
                    (
                        range.clone(),
                        match highlight {
                            Highlight::Highlight(highlight) => *highlight,
                            Highlight::Mention => HighlightStyle {
                                font_weight: Some(FontWeight::BOLD),
                                ..Default::default()
                            },
                        },
                    )
                }),
            ),
        )
        .on_click(self.link_ranges.clone(), {
            let link_urls = self.link_urls.clone();
            move |ix, _, cx| {
                let url = &link_urls[ix];
                if url.starts_with("http") {
                    cx.open_url(url);
                }
                // Handle mention URLs
                else if url.starts_with("mention:") {
                    // Handle mention clicks
                    // For example: cx.emit_custom_event(MentionClicked(url.strip_prefix("mention:").unwrap().to_string()));
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
    text: &mut String,
    highlights: &mut Vec<(Range<usize>, Highlight)>,
    link_ranges: &mut Vec<Range<usize>>,
    link_urls: &mut Vec<String>,
) {
    // Copy the content directly
    text.push_str(content);

    // Process links with linkify
    let mut finder = linkify::LinkFinder::new();
    finder.kinds(&[linkify::LinkKind::Url]);

    for link in finder.links(content) {
        let start = link.start();
        let end = link.end();
        let range = start..end;

        link_ranges.push(range.clone());
        link_urls.push(link.as_str().to_string());

        highlights.push((
            range,
            Highlight::Highlight(HighlightStyle {
                underline: Some(UnderlineStyle {
                    thickness: 1.0.into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        ));
    }

    // Process mentions (npub and nprofile)
    let mention_pattern = regex::Regex::new(r"\b(npub|nprofile)[a-zA-Z0-9]+\b").unwrap();

    for mention_match in mention_pattern.find_iter(content) {
        let start = mention_match.start();
        let end = mention_match.end();
        let range = start..end;
        let mention_text = mention_match.as_str();

        // Avoid duplicating highlights if this range was already processed as a link
        if !link_ranges
            .iter()
            .any(|r| r.start < range.end && range.start < r.end)
        {
            // All mentions are treated the same way
            highlights.push((range.clone(), Highlight::Mention));

            // Make mentions clickable
            link_ranges.push(range);
            link_urls.push(format!("mention:{}", mention_text));
        }
    }
}
