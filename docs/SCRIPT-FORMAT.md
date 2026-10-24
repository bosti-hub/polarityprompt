# Script format

Scripts are plain Markdown files. Anything you can write in Obsidian works; the conventions
below add prompter meaning without breaking normal Markdown rendering.

## Front-matter (optional)

```yaml
---
title: Episode 12 — Brood bakken met zuurdesem
lang: nl          # en | nl | de | fr | es | pt ; overrides the app default for this script
---
```

## Blocks and how the prompter treats them

| Markdown | Block kind | Prompter behaviour |
|---|---|---|
| `# Heading`, `## Heading` | Heading | Rendered large; gets an anchor (auto) |
| Paragraph | Paragraph | Read verbatim; gets an anchor (auto) |
| `- bullet` / `1. item` list | TalkingPoints | Ad-lib zone: shown as bullets, scroll **holds**, resumes at the next block's anchor |
| `---` | Rule | Visual separator, no anchor |
| `**bold**`, `*italic*` | inline | Rendered as emphasis |
| Blank line | — | Paragraph break; extra blank lines are preserved as extra space |

## Bookmarks

Put `<!-- bookmark -->` or `<!-- bookmark: Chapter 2 -->` on its own line above a block. The
prompter shows a dot in the margin and a tick on the progress bar; click either, or press `.` /
`,`, to jump there and hold. Useful for retakes.

## Cues that are not spoken

Text in `[square brackets]` is shown dimmed and italic but ignored by the voice tracking and the
word count: stage directions (`[pause]`, `[look at camera]`), names the speech model will never
recognise, or placeholders (`[guest name]`).

## Anchors

By default the app derives 2–4 keywords per paragraph/heading automatically: distinctive content
words (names, terms, numbers), never stopwords. An anchor fires when at least `min_hits` of its
keywords are heard within a ~6 s window, in any order and in any sentence structure.

To choose the keywords yourself, put an HTML comment right above the block:

```markdown
<!-- anchor: zuurdesem bacteriën temperatuur -->
Wat veel mensen niet weten is dat zuurdesem pas goed werkt zodra de bacteriën de juiste
temperatuur hebben.
```

Rules of thumb for good anchors: pick words you would say in *any* phrasing of that paragraph,
prefer nouns and proper names over verbs, avoid words that also appear in neighbouring paragraphs.
Automatic anchors only use words that occur nowhere else in the script; a paragraph without such a
word gets no anchor, and the prompter simply cruises through it. Word forms are matched loosely
("onderdeel" also hits "onderdelen").
Words unknown to the speech model (typically foreign names or English terms in a Dutch script) are
ignored and flagged in the operator window; pick a different word for that block.

## Example

```markdown
---
title: Intro
lang: nl
---

# Waarom zuurdesem?

<!-- anchor: smaak zuur verschil -->
Smaak ontstaat niet uit gist maar uit zuur: het verschil tussen twee broden is precies wat
de bakker proeft.

- voorbeeld uit de praktijk
- de vergelijking met de wijnmaker
- korte anekdote over het brood van vorige week

Daarom kijken we vandaag naar drie onderdelen die dat verschil vergroten of juist verpesten.
```

The bullet block is where you talk freely; the prompter stops there and picks up at
"Daarom kijken we vandaag…" as soon as it hears its keywords.
