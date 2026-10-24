//! Anchors: keyword sets attached to blocks. See `docs/ARCHITECTURE.md` 2.2.
//!
//! Phase 0 derives them and ships them to the UI; nothing acts on them yet.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::document::{Block, BlockId, BlockKind, ExplicitAnchor};
use crate::stopwords::is_stopword;
use crate::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnchorId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub id: AnchorId,
    pub block: BlockId,
    /// Reading order.
    pub ordinal: usize,
    /// Normalized keywords (lowercase, no punctuation), 2–4 per anchor.
    pub keywords: Vec<String>,
    /// Where each keyword first occurs in the block, as a fraction of its words (0..1).
    /// Lets a hit glide to the spoken words instead of the top of the block.
    pub positions: Vec<f32>,
    /// 2 when the anchor has two or more keywords, else 1.
    pub min_hits: usize,
    /// Authored via `<!-- anchor: -->` rather than auto-derived.
    pub explicit: bool,
}

/// Lowercase, letters and digits only, accents folded ("één" → "een") so the
/// script and the recognizer's spelling agree.
pub fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .map(fold_accent)
        .collect()
}

/// A word as the recognizer spells it: lowercase, accents kept, apostrophes and hyphens
/// inside the word kept ("genießen", "l'arôme", "lisez-le"). The speech models know these
/// spellings, not the folded keywords ("geniesen", "larome", "lisezle") (D24).
pub fn spoken_form(raw: &str) -> String {
    let word: String = raw
        .chars()
        .map(|c| if c == '’' { '\'' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
        .flat_map(char::to_lowercase)
        .collect();
    word.trim_matches(|c| c == '\'' || c == '-').to_string()
}

/// Every spelling of each keyword as it occurs in the script, in `spoken_form`. A keyword
/// that does not occur in the text (an explicit anchor) is its own spelling.
pub fn keyword_spellings(blocks: &[Block], anchors: &[Anchor]) -> HashMap<String, Vec<String>> {
    let mut spellings: HashMap<String, Vec<String>> = HashMap::new();
    for a in anchors {
        for k in &a.keywords {
            spellings.entry(k.clone()).or_default();
        }
    }
    for raw in blocks.iter().flat_map(|b| b.plain.split_whitespace()) {
        if let Some(list) = spellings.get_mut(&normalize(raw)) {
            let spoken = spoken_form(raw);
            if !list.contains(&spoken) {
                list.push(spoken);
            }
        }
    }
    for (k, list) in spellings.iter_mut() {
        if list.is_empty() {
            list.push(k.clone());
        }
    }
    spellings
}

/// The words to check with the speech engine: all spellings of all keywords, sorted.
pub fn vocabulary(blocks: &[Block], anchors: &[Anchor]) -> Vec<String> {
    let mut words: Vec<String> = keyword_spellings(blocks, anchors).into_values().flatten().collect();
    words.sort();
    words.dedup();
    words
}

fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'ç' => 'c',
        'ñ' => 'n',
        'ß' => 's',
        other => other,
    }
}

const MIN_WORD_LEN: usize = 4;
const AUTO_KEYWORDS: usize = 4;

pub fn derive(blocks: &[Block], explicit: &[ExplicitAnchor], lang: Lang) -> Vec<Anchor> {
    // Document frequency of each candidate word, so rare words rank first.
    let mut df: HashMap<String, usize> = HashMap::new();
    let per_block: Vec<Vec<String>> = blocks
        .iter()
        .map(|b| {
            if !is_anchorable(b.kind) {
                return Vec::new();
            }
            let mut words: Vec<String> = b
                .plain
                .split_whitespace()
                .map(normalize)
                .filter(|w| w.chars().count() >= MIN_WORD_LEN && !is_stopword(lang, w))
                .collect();
            words.sort();
            words.dedup();
            for w in &words {
                *df.entry(w.clone()).or_insert(0) += 1;
            }
            words
        })
        .collect();

    let mut anchors = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if !is_anchorable(block.kind) {
            continue;
        }
        let (keywords, is_explicit) = match explicit.iter().find(|e| e.block == block.id) {
            Some(e) => (e.keywords.clone(), true),
            None => {
                // Rank by rarity in the document, then by first appearance in the block
                // so ties stay stable.
                let mut ranked: Vec<(&String, usize, usize)> = per_block[i]
                    .iter()
                    .map(|w| {
                        let first = block
                            .plain
                            .split_whitespace()
                            .position(|raw| normalize(raw) == *w)
                            .unwrap_or(usize::MAX);
                        (w, df.get(w).copied().unwrap_or(1), first)
                    })
                    .collect();
                ranked.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
                // Words unique to this block are the only reliable evidence; words that
                // also occur elsewhere ("mechanisme") fire on the wrong block or on a
                // hallucinated word. Without a unique word the block gets no anchor.
                let mut picked: Vec<String> = ranked.iter().filter(|r| r.1 == 1).take(AUTO_KEYWORDS).map(|r| r.0.clone()).collect();
                if picked.is_empty() {
                    continue;
                }
                if picked.len() < 2 {
                    picked.extend(ranked.iter().filter(|r| r.1 > 1).take(2 - picked.len()).map(|r| r.0.clone()));
                }
                (picked, false)
            }
        };
        if keywords.is_empty() {
            continue;
        }
        let positions = keywords.iter().map(|k| word_position(&block.plain, k)).collect();
        let min_hits = if keywords.len() >= 2 { 2 } else { 1 };
        anchors.push(Anchor {
            id: AnchorId(anchors.len() as u32),
            block: block.id,
            ordinal: anchors.len(),
            keywords,
            positions,
            min_hits,
            explicit: is_explicit,
        });
    }
    anchors
}

fn is_anchorable(kind: BlockKind) -> bool {
    matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_))
}

/// Fraction (0..1) of the block's words before the first occurrence of `keyword`.
fn word_position(plain: &str, keyword: &str) -> f32 {
    let words: Vec<&str> = plain.split_whitespace().collect();
    if words.is_empty() {
        return 0.0;
    }
    match words.iter().position(|w| normalize(w) == keyword) {
        Some(i) => i as f32 / words.len() as f32,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn normalize_strips_punctuation_and_case() {
        assert_eq!(normalize("Korst."), "korst");
        assert_eq!(normalize("**Geduld**"), "geduld");
        assert_eq!(normalize("Één"), "een");
        assert_eq!(normalize("beïnvloeden"), "beinvloeden");
    }

    #[test]
    fn spoken_form_keeps_accents_apostrophes_and_hyphens() {
        assert_eq!(spoken_form("Genießen,"), "genießen");
        assert_eq!(spoken_form("l’arôme"), "l'arôme");
        assert_eq!(spoken_form("(lisez-le)."), "lisez-le");
        assert_eq!(spoken_form("'quoted'"), "quoted");
    }

    #[test]
    fn vocabulary_uses_the_spelling_in_the_script() {
        let doc = Document::parse("x.md", "Wir genießen frischen Kaffee.\n\nLisez-le avec l’arôme du café.", Lang::De);
        let words = doc.vocabulary();
        assert!(words.contains(&"genießen".to_string()), "{words:?}");
        assert!(!words.contains(&"geniesen".to_string()), "{words:?}");
        let spellings = keyword_spellings(&doc.blocks, &doc.anchors);
        assert_eq!(spellings["geniesen"], vec!["genießen"]);
        assert_eq!(spellings["lisezle"], vec!["lisez-le"]);
        assert_eq!(spellings["larome"], vec!["l'arôme"]);
    }

    #[test]
    fn talking_points_and_rules_get_no_anchor() {
        let doc = Document::parse("x.md", "- a\n- b\n\n---\n\nParagraph with useful content words.", Lang::En);
        assert_eq!(doc.anchors.len(), 1);
        assert_eq!(doc.anchors[0].block, BlockId(2));
    }

    #[test]
    fn auto_anchor_prefers_rare_long_content_words() {
        let text = "Het eerste onderdeel is voorbereiding. Wie de voorbereiding neemt bepaalt de richting.";
        let doc = Document::parse("x.md", text, Lang::Nl);
        let a = &doc.anchors[0];
        assert!(!a.explicit);
        assert!(a.keywords.len() >= 3);
        assert!(a.keywords.contains(&"voorbereiding".to_string()), "{:?}", a.keywords);
        assert!(!a.keywords.iter().any(|w| w == "eerste" || w == "het"), "{:?}", a.keywords);
        assert_eq!(a.min_hits, 2);
    }

    #[test]
    fn blocks_without_a_unique_word_get_no_anchor() {
        let text = "# Onderdeel één: voorbereiding\n\nHet eerste onderdeel is voorbereiding, en voorbereiding bepaalt de richting.\n\n# Onderdeel twee\n\nHet tweede onderdeel werkt via rijzen.";
        let doc = Document::parse("x.md", text, Lang::Nl);
        let blocks: Vec<u32> = doc.anchors.iter().map(|a| a.block.0).collect();
        // Heading 1 shares both its words with the paragraph → no anchor. Paragraph 1 has "bepaalt"/"richting".
        assert_eq!(blocks, vec![1, 3], "{:?}", doc.anchors);
        assert!(doc.anchors[0].keywords.contains(&"bepaalt".to_string()));
    }

    #[test]
    fn ordinals_follow_reading_order() {
        let doc = Document::parse("x.md", include_str!("../testdata/zuurdesem.md"), Lang::Nl);
        for (i, a) in doc.anchors.iter().enumerate() {
            assert_eq!(a.ordinal, i);
            assert_eq!(a.id, AnchorId(i as u32));
        }
        assert!(doc.anchors.windows(2).all(|w| w[0].block.0 < w[1].block.0));
    }

    #[test]
    fn short_anchor_needs_one_hit() {
        let doc = Document::parse("x.md", "<!-- anchor: korst -->\nDe korst.", Lang::Nl);
        assert_eq!(doc.anchors[0].keywords, vec!["korst"]);
        assert_eq!(doc.anchors[0].min_hits, 1);
        assert_eq!(doc.anchors[0].positions, vec![0.5]);
    }

    #[test]
    fn two_keyword_anchor_needs_both() {
        let doc = Document::parse("x.md", "<!-- anchor: korst kleur -->\nDe kleur van de korst.", Lang::Nl);
        assert_eq!(doc.anchors[0].min_hits, 2);
        assert_eq!(doc.anchors[0].positions, vec![0.8, 0.2]);
    }
}
