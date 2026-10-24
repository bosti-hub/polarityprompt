# Changelog

All notable changes to PolarityPrompt. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (0.x: things may still change between releases).

## [Unreleased]

## [0.1.0] — 2026-10-24

First public release.

### Added
- Voice-locked scrolling, fully offline (Vosk small models). English is bundled; Dutch, German,
  French, Spanish and Portuguese can be downloaded under Settings → Languages (checked with
  SHA-256, then offline).
  The whole script is aligned against what you say, word by word, so rephrasing, skipping
  ahead and improvising are handled.
- Markdown scripts: headings, paragraphs, talking-point lists (the text holds while you
  ad-lib), bookmarks (`<!-- bookmark -->`), stage directions in `[square brackets]`, front
  matter with `title` and `lang`.
- Operator window with controls, settings and an Advanced tab; talent window for a second
  screen with a screen picker and a "Bring here" button; horizontal and vertical mirroring.
- Cruise scrolling without voice, countdown, reading timer, progress bar, keyboard shortcuts,
  dimming above the read line, adjustable text width, font, colours and read line.
- A built-in practice script in all six languages, opened in the chosen language.
- Installers for Windows 10/11 (per-user, no administrator rights) and macOS 12+ on Apple
  silicon (unsigned).
