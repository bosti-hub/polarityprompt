# PolarityPrompt

A teleprompter that listens to you. Open a script written in Markdown, read it aloud, and the
text scrolls along with your voice: it waits when you pause, keeps up when you speed up, and
finds its place again when you skip ahead or improvise.

**Everything runs on your own computer.** No account, no telemetry, and audio never leaves your
machine. The only time the app goes online is when you download an extra language.

## Features

- Voice-locked scrolling, fully offline (Vosk speech recognition). **English** is included;
  **Dutch, German, French, Spanish and Portuguese** are a one-time download inside the app.
- Large, mirrorable text for a beam-splitter prompter; a separate talent window for a second
  screen, with a screen picker.
- Talking points: bullet lists are ad-lib zones where the text holds until you continue.
- Bookmarks for retakes, stage directions in `[square brackets]` that are shown but not
  listened for, a countdown, reading timer and progress bar, keyboard shortcuts.
- Manual and automatic cruise scrolling when you would rather not use your voice.
- A built-in practice script in every language to try it in a minute.

## Download

Installers for **Windows 10/11 (64-bit)** and **macOS 12+ on Apple silicon** are on the
[Releases page](https://github.com/bosti-hub/polarityprompt/releases). English speech
recognition is inside the installer; other languages are a download inside the app.

The installers are not code-signed yet. Windows may show a SmartScreen warning and macOS asks
you to confirm the first start: [docs/USER-GUIDE.md](docs/USER-GUIDE.md) explains the one-time
steps.

## Getting started

1. Install and start PolarityPrompt.
2. Click **Practice script** on the start screen. For another language, pick it under
   **Controls → Language** first (and download it under Settings → Languages).
3. Click **Start listening** and read the text aloud.

Then write your own script: any `.md` file works. See the
[user guide](docs/USER-GUIDE.md) and the [script format](docs/SCRIPT-FORMAT.md).

## Support the project

PolarityPrompt is free and open source. If it saves you time, you can support its development
via [GitHub Sponsors](https://github.com/sponsors/bosti-hub).

## Building from source

See [CONTRIBUTING.md](CONTRIBUTING.md) for setting up a development machine (Windows or macOS),
running the app, the checks and making a release. The design is described in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/DECISIONS.md](docs/DECISIONS.md).

## License

Copyright © 2026 F. Bos - [OnPolarity.com](https://onpolarity.com).

PolarityPrompt is free software: you can redistribute it and/or modify it under the terms of the
[GNU General Public License](LICENSE), version 3 or (at your option) any later version.

It includes third-party components, among them the Vosk speech recognition library and models
(Apache 2.0); their licenses are listed in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt),
which is also installed next to the app.
