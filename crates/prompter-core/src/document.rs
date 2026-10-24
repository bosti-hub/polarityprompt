//! Markdown script → `Document` (blocks with kind, HTML, plain text, word count).
//!
//! See `docs/SCRIPT-FORMAT.md` for the conventions and `docs/ARCHITECTURE.md` 2.1
//! for the types. The UI renders `Block.html` as-is and never sees raw Markdown.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::anchors::{self, Anchor};
use crate::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "level", rename_all = "snake_case")]
pub enum BlockKind {
    Heading(u8),
    Paragraph,
    TalkingPoints,
    Rule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    #[serde(flatten)]
    pub kind: BlockKind,
    /// Inline HTML for this block (from pulldown-cmark). The UI renders it as-is.
    pub html: String,
    /// Plain text as spoken: used for matching and the pace model. Text in
    /// `[square brackets]` is shown but left out here (stage directions, names).
    pub plain: String,
    pub word_count: usize,
    /// Set by a `<!-- bookmark -->` or `<!-- bookmark: label -->` comment above the block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub path: PathBuf,
    pub title: Option<String>,
    /// Content hash; takes reference it so a take can be matched to a script version.
    pub hash: u64,
    pub language: Lang,
    pub blocks: Vec<Block>,
    pub anchors: Vec<Anchor>,
}

impl Document {
    /// The words to give the speech engine: the anchor keywords as spelled in the script.
    pub fn vocabulary(&self) -> Vec<String> {
        anchors::vocabulary(&self.blocks, &self.anchors)
    }

    /// Parse Markdown text. `default_lang` is used when the front-matter has no `lang:`.
    pub fn parse(path: impl Into<PathBuf>, text: &str, default_lang: Lang) -> Document {
        let path = path.into();
        let hash = {
            let mut h = DefaultHasher::new();
            text.hash(&mut h);
            h.finish()
        };

        let mut options = Options::empty();
        options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
        let parser = Parser::new_ext(text, options);

        let mut builder = Builder::new(default_lang);
        for event in parser {
            builder.push(event);
        }
        let (title, language, blocks, explicit) = builder.finish();
        let anchors = anchors::derive(&blocks, &explicit, language);

        Document {
            path,
            title,
            hash,
            language,
            blocks,
            anchors,
        }
    }
}

/// Explicit anchor authored as `<!-- anchor: w1 w2 w3 -->` above a block.
#[derive(Debug, Clone)]
pub struct ExplicitAnchor {
    pub block: BlockId,
    pub keywords: Vec<String>,
}

/// Groups the flat pulldown-cmark event stream into blocks.
struct Builder<'a> {
    default_lang: Lang,
    title: Option<String>,
    language: Option<Lang>,
    blocks: Vec<Block>,
    explicit: Vec<ExplicitAnchor>,
    /// Keywords from the most recent `<!-- anchor: -->` comment, waiting for the next block.
    pending_anchor: Option<Vec<String>>,
    /// Label from the most recent `<!-- bookmark -->` comment, waiting for the next block.
    pending_bookmark: Option<String>,
    /// Events collected for the block currently being built.
    current: Option<Current<'a>>,
    in_metadata: bool,
}

struct Current<'a> {
    kind: BlockKind,
    events: Vec<Event<'a>>,
    plain: String,
    /// Nesting depth of lists, so a list ends only when the outermost list closes.
    list_depth: u32,
}

impl<'a> Builder<'a> {
    fn new(default_lang: Lang) -> Self {
        Builder {
            default_lang,
            title: None,
            language: None,
            blocks: Vec::new(),
            explicit: Vec::new(),
            pending_anchor: None,
            pending_bookmark: None,
            current: None,
            in_metadata: false,
        }
    }

    fn push(&mut self, event: Event<'a>) {
        // Front-matter.
        match &event {
            Event::Start(Tag::MetadataBlock(_)) => {
                self.in_metadata = true;
                return;
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                self.in_metadata = false;
                return;
            }
            Event::Text(t) if self.in_metadata => {
                self.read_front_matter(t);
                return;
            }
            _ => {}
        }

        // Inside a block: collect events until the block ends.
        if let Some(cur) = self.current.as_mut() {
            match &event {
                Event::Start(Tag::List(_)) => cur.list_depth += 1,
                Event::End(TagEnd::List(_)) => {
                    cur.list_depth -= 1;
                    if cur.list_depth == 0 {
                        cur.events.push(event);
                        self.close_block();
                    } else {
                        cur.events.push(event);
                    }
                    return;
                }
                Event::End(TagEnd::Paragraph) if cur.kind == BlockKind::Paragraph => {
                    self.close_block();
                    return;
                }
                Event::End(TagEnd::Heading(_)) if matches!(cur.kind, BlockKind::Heading(_)) => {
                    self.close_block();
                    return;
                }
                Event::Text(t) | Event::Code(t) => cur.plain.push_str(t),
                Event::SoftBreak | Event::HardBreak => cur.plain.push(' '),
                Event::End(TagEnd::Item) => cur.plain.push(' '),
                _ => {}
            }
            // Inside a Paragraph/Heading the wrapping tag is not part of the inline HTML.
            match (&event, cur.kind) {
                (Event::Start(Tag::Paragraph), BlockKind::TalkingPoints)
                | (Event::End(TagEnd::Paragraph), BlockKind::TalkingPoints) => {
                    // Loose lists wrap items in <p>; keep them out so bullets stay compact.
                }
                _ => cur.events.push(event),
            }
            return;
        }

        // Between blocks.
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.open_block(BlockKind::Heading(heading_level(level)));
            }
            Event::Start(Tag::Paragraph) => self.open_block(BlockKind::Paragraph),
            Event::Start(Tag::List(_)) => {
                self.open_block(BlockKind::TalkingPoints);
                let cur = self.current.as_mut().expect("just opened");
                cur.list_depth = 1;
                cur.events.push(event);
            }
            Event::Rule => {
                self.open_block(BlockKind::Rule);
                self.close_block();
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                if let Some(words) = parse_anchor_comment(&h) {
                    self.pending_anchor = Some(words);
                }
                if let Some(label) = parse_bookmark_comment(&h) {
                    self.pending_bookmark = Some(label);
                }
            }
            _ => {}
        }
    }

    fn open_block(&mut self, kind: BlockKind) {
        self.current = Some(Current {
            kind,
            events: Vec::new(),
            plain: String::new(),
            list_depth: 0,
        });
    }

    fn close_block(&mut self) {
        let Some(cur) = self.current.take() else { return };
        let id = BlockId(self.blocks.len() as u32);
        let mut html_out = String::new();
        html::push_html(&mut html_out, cur.events.into_iter());
        let html_out = mark_skips(&html_out);
        let plain = strip_skips(&cur.plain).split_whitespace().collect::<Vec<_>>().join(" ");
        let word_count = plain.split_whitespace().count();
        let bookmark = self.pending_bookmark.take();
        if let Some(words) = self.pending_anchor.take() {
            // A comment above a Rule or list has nowhere to go; it just gets dropped.
            if matches!(cur.kind, BlockKind::Paragraph | BlockKind::Heading(_)) {
                self.explicit.push(ExplicitAnchor { block: id, keywords: words });
            }
        }
        self.blocks.push(Block {
            id,
            kind: cur.kind,
            html: html_out.trim_end().to_string(),
            plain,
            word_count,
            bookmark,
        });
    }

    fn read_front_matter(&mut self, text: &str) {
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else { continue };
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key.trim() {
                "title" if !value.is_empty() => self.title = Some(value.to_string()),
                "lang" => self.language = Lang::parse(value),
                _ => {}
            }
        }
    }

    fn finish(mut self) -> (Option<String>, Lang, Vec<Block>, Vec<ExplicitAnchor>) {
        self.close_block();
        let lang = self.language.unwrap_or(self.default_lang);
        (self.title, lang, self.blocks, self.explicit)
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    level as u8
}

/// `<!-- bookmark -->` or `<!-- bookmark: label -->` → label (may be empty).
fn parse_bookmark_comment(html: &str) -> Option<String> {
    let start = html.find("<!--")?;
    let end = html[start..].find("-->")? + start;
    let inner = html[start + 4..end].trim();
    let rest = inner.strip_prefix("bookmark")?;
    let label = rest.trim_start_matches(':').trim();
    Some(label.to_string())
}

/// Text in `[square brackets]` is shown but not spoken: remove it from the spoken text.
fn strip_skips(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => {
                depth -= 1;
                out.push(' ');
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Wrap `[square brackets]` in the rendered HTML so the UI can style them as cues.
/// Only bracket pairs outside HTML tags are touched.
fn mark_skips(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 32);
    let mut in_tag = false;
    let mut open = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(c);
            }
            '>' => {
                in_tag = false;
                out.push(c);
            }
            '[' if !in_tag && !open => {
                open = true;
                out.push_str("<span class=\"skip\">[");
            }
            ']' if !in_tag && open => {
                open = false;
                out.push_str("]</span>");
            }
            _ => out.push(c),
        }
    }
    if open {
        out.push_str("</span>");
    }
    out
}

/// `<!-- anchor: w1 w2 w3 -->` → `["w1", "w2", "w3"]` (normalized).
fn parse_anchor_comment(html: &str) -> Option<Vec<String>> {
    let start = html.find("<!--")?;
    let end = html[start..].find("-->")? + start;
    let inner = html[start + 4..end].trim();
    let rest = inner.strip_prefix("anchor:")?;
    let words: Vec<String> = rest
        .split_whitespace()
        .map(anchors::normalize)
        .filter(|w| !w.is_empty())
        .collect();
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../testdata/zuurdesem.md");

    fn kinds(doc: &Document) -> Vec<BlockKind> {
        doc.blocks.iter().map(|b| b.kind).collect()
    }

    #[test]
    fn reads_front_matter() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::En);
        assert_eq!(doc.language, Lang::Nl);
        assert_eq!(doc.title.as_deref(), Some("Testtekst — Waarom zuurdesem?"));
    }

    #[test]
    fn falls_back_to_default_language() {
        let doc = Document::parse("x.md", "# Hi\n\nHello there.", Lang::En);
        assert_eq!(doc.language, Lang::En);
        assert_eq!(doc.title, None);
    }

    #[test]
    fn groups_blocks_by_kind() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::Nl);
        use BlockKind::*;
        assert_eq!(
            kinds(&doc),
            vec![
                Heading(1),
                Paragraph,
                TalkingPoints,
                Paragraph,
                Heading(2),
                Paragraph,
                Paragraph,
                Heading(2),
                Paragraph,
                TalkingPoints,
                Heading(2),
                Paragraph,
                Rule,
                Paragraph,
            ]
        );
        for (i, b) in doc.blocks.iter().enumerate() {
            assert_eq!(b.id, BlockId(i as u32));
        }
    }

    #[test]
    fn paragraph_html_is_inline_with_emphasis() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::Nl);
        let p = &doc.blocks[1];
        assert!(p.html.contains("<strong>zuur</strong>"), "{}", p.html);
        assert!(!p.html.starts_with("<p>"), "wrapper must be stripped: {}", p.html);
        assert!(p.plain.starts_with("Smaak ontstaat niet uit gist maar uit zuur."));
        assert_eq!(p.word_count, p.plain.split_whitespace().count());
        assert!(p.word_count > 30);
    }

    #[test]
    fn heading_html_has_no_wrapper() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::Nl);
        assert_eq!(doc.blocks[0].html, "Waarom zuurdesem?");
        assert_eq!(doc.blocks[0].plain, "Waarom zuurdesem?");
    }

    #[test]
    fn list_becomes_one_talking_points_block() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::Nl);
        let tp = &doc.blocks[2];
        assert!(tp.html.starts_with("<ul>"), "{}", tp.html);
        assert_eq!(tp.html.matches("<li>").count(), 3);
        assert!(tp.plain.contains("vergelijking met de wijnmaker"));
    }

    #[test]
    fn rule_is_empty() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::Nl);
        let rule = &doc.blocks[12];
        assert_eq!(rule.kind, BlockKind::Rule);
        assert_eq!(rule.word_count, 0);
    }

    #[test]
    fn explicit_anchor_comment_attaches_to_next_block() {
        let doc = Document::parse("x.md", EXAMPLE, Lang::Nl);
        let a = doc.anchors.iter().find(|a| a.block == BlockId(1)).unwrap();
        assert!(a.explicit);
        assert_eq!(a.keywords, vec!["smaak", "zuur", "verschil"]);
        let b = doc.anchors.iter().find(|a| a.block == BlockId(3)).unwrap();
        assert!(b.explicit);
        assert_eq!(b.keywords, vec!["onderdelen", "vergroten", "verpesten"]);
    }

    #[test]
    fn bookmarks_attach_to_the_next_block() {
        let doc = Document::parse("x.md", "<!-- bookmark: Intro -->\n# Hi\n\nText.\n\n<!-- bookmark -->\nMore text here.", Lang::En);
        assert_eq!(doc.blocks[0].bookmark.as_deref(), Some("Intro"));
        assert_eq!(doc.blocks[1].bookmark, None);
        assert_eq!(doc.blocks[2].bookmark.as_deref(), Some(""));
    }

    #[test]
    fn bracketed_text_is_shown_but_not_spoken() {
        let doc = Document::parse("x.md", "Say hello to [name here] and smile [pause] now.", Lang::En);
        let b = &doc.blocks[0];
        assert_eq!(b.plain, "Say hello to and smile now.");
        assert_eq!(b.word_count, 6);
        assert!(b.html.contains("<span class=\"skip\">[name here]</span>"), "{}", b.html);
        assert!(b.html.contains("<span class=\"skip\">[pause]</span>"), "{}", b.html);
    }

    #[test]
    fn hash_changes_with_content() {
        let a = Document::parse("x.md", "Hello", Lang::Nl);
        let b = Document::parse("x.md", "Hello!", Lang::Nl);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn block_kind_serializes_flat() {
        let doc = Document::parse("x.md", "## Two\n\nText", Lang::Nl);
        let json = serde_json::to_string(&doc.blocks[0]).unwrap();
        assert!(json.contains("\"kind\":\"heading\""), "{json}");
        assert!(json.contains("\"level\":2"), "{json}");
        let json = serde_json::to_string(&doc.blocks[1]).unwrap();
        assert!(json.contains("\"kind\":\"paragraph\""), "{json}");
    }
}
