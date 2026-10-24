# PolarityPrompt — user guide

PolarityPrompt is a teleprompter that listens to you. You open a script written in Markdown,
it shows the text large on screen, and while you speak it scrolls to the words you are saying.
Everything runs on your own computer. No audio and no text ever leave it, and it works without
an internet connection.

There is nothing to compile and no extra software to install: the speech recognition model for
English is inside the app. Dutch, German, French, Spanish and Portuguese can be added with one
download each (see *Languages* below); that is the only time the app goes online.

## 1. Install

Download the file for your computer: [Mac](https://github.com/bosti-hub/polarityprompt/releases/latest/download/PolarityPrompt-Mac.dmg)
or [Windows](https://github.com/bosti-hub/polarityprompt/releases/latest/download/PolarityPrompt-Windows-Setup.exe)
(all versions are on the [Releases page](https://github.com/bosti-hub/polarityprompt/releases)).

### macOS (Apple silicon: M1, M2, M3, M4, M5)

1. Download `PolarityPrompt-Mac.dmg` and open it. Click **Agree** for the license.
2. Drag **PolarityPrompt** into **Applications**.
3. First start only: the app is not signed with an Apple developer certificate, so macOS will
   refuse to open it with a double-click. Instead, **right-click the app → Open → Open**.
   If macOS says the app "is damaged" or "cannot be checked for malicious software", open
   **System Settings → Privacy & Security**, scroll down, and click **Open Anyway** next to
   PolarityPrompt. This is needed once.
4. The first time you click **Start listening**, macOS asks for microphone access. Click
   **Allow**. If you clicked Don't Allow by mistake: **System Settings → Privacy & Security →
   Microphone**, switch PolarityPrompt on.

Requirements: macOS 12 or newer on an Apple silicon Mac. Intel Macs are not supported by this
download.

### Windows 10 / 11 (64-bit)

1. Download `PolarityPrompt-Windows-Setup.exe` and run it.
2. Windows SmartScreen may show "Windows protected your PC" because the installer is not
   signed with a certificate. Click **More info → Run anyway**.
3. The installer places the app in your user folder; no administrator password is needed.
   If Microsoft Edge WebView2 is missing (rare on Windows 10 and 11), the installer downloads it.
4. The first time you click **Start listening**, Windows may ask for microphone access. Allow it.
   If nothing is heard: **Settings → Privacy & security → Microphone**, make sure desktop apps
   may use the microphone.

## 2. Write a script

A script is a plain text file with the `.md` extension. You can write it in any text editor,
Obsidian, or Notes exported as Markdown.

```markdown
---
title: Episode 12
lang: nl
---

# Een goede kop koffie

Elke goede kop begint met **verse** bonen. Koffiebonen zijn de zaden van een rode
vrucht die aan kleine struiken groeit.

- hoe jij je koffie het liefst drinkt
- het leukste café waar je ooit bent geweest

Bij het branden worden de groene zaden bruin en ontstaat de smaak.
```

- `lang: en`, `nl`, `de`, `fr`, `es` or `pt` at the top tells the app which language you will speak. You can
  also change it in the app.
- `# Heading` lines are shown larger.
- Paragraphs are read as written. Leave an empty line between paragraphs.
- `- bullet` lists are **talking points**: the prompter stops there so you can improvise, and
  continues as soon as it hears you read the next paragraph.
- `**bold**` and `*italic*` work. A line with `---` draws a separator.
- `<!-- bookmark: Part 2 -->` on its own line above a paragraph adds a bookmark: a dot in the
  margin and a tick on the progress bar. Click it, or press `.` and `,`, to jump there for a retake.
- Text in `[square brackets]` is shown but not listened for: stage directions like `[pause]`,
  or names the app would never recognise.

## 3. Use the prompter

To try the app before writing anything, pick a language under **Controls → Language** and click
**Practice script** (also on the start screen). It opens a short text about making coffee, in that
language, that uses every feature above: headings, a bookmark, a talking-points list and stage
directions. Click **Start listening** and read it aloud.

The window has the text on one side and a narrow panel with three tabs on the other.

**Controls**
- **Open script…** opens a `.md` file. **Reopen** brings back the last one.
- **Language**: the language of this script (English, or one you downloaded under Settings →
  Languages). **Practice script** opens a short text in that language.
- **Input device** and **Start listening**: choose your microphone and switch listening on.
  The bar under the button shows the microphone level; the line below it says what the app is
  doing (listening, reconnecting, no signal). While listening, the box shows the last sentence
  it understood, which is handy to check that it hears you well.
- **Start / Pause / Hold / Restart**: start the slow automatic scroll (after a short countdown,
  adjustable in Settings), pause it, hold it in place, or go back to the top and reset the
  timer. Space bar pauses and resumes; the full list of keys is under Settings.
- **Mute** silences the microphone without stopping the session.
- The bar at the bottom of the text shows the reading time, your progress, and the bookmarks.
- Click anywhere in the text to bring that spot to the read line.
- **Speed**: how fast the text drifts when you are reading between two recognised positions.
  The left half of the slider covers the useful slow speeds.
- **Mirror horizontal / vertical**: for a teleprompter glass. Mirroring applies to the talent
  window when it is open, otherwise to the main window.
- **Talent window** opens a second, text-only window on your second screen (or the same one).
  Click it again to close it. While it is open, the screen list below the button moves it to
  another screen (numbered left to right; "this window" marks the screen of the main window),
  and **⌖ Bring here** puts it back as a normal window in front of the main window, for when it
  ended up out of sight.
- You can always scroll the text yourself with the mouse wheel or trackpad; the app takes it
  from there.

**Settings**: font, size, line height, text width and position, alignment, read line (the
orange line where you read) and how much the text above it is dimmed, colours, countdown,
automatic stop after long silence, panels left or right, what happens after the app has found
your position (keep drifting slowly, or hold until the next recognised sentence), and the list
of keyboard shortcuts.

**Advanced**: which speech models are loaded, the default language, and the sensitivity
of the voice tracking. The defaults are shown in brackets; **Reset to defaults** is one click.
The developer tools at the bottom are for tuning the scroll animation and can be ignored.

## 4. How the voice tracking works, in short

The app converts your speech to text and continuously compares the last few words it heard
with the script. When they match, the text glides so that those words sit on the read line.
It prefers to move forward, so reading on works smoothly; going back to repeat a sentence
needs a full sentence of evidence, and improvising simply makes the prompter wait.

Tips:
- Speak at your natural pace. A decent microphone helps, but the built-in laptop microphone
  works in a quiet room.
- Names and foreign words may not be recognised. That is fine, the surrounding words carry it.
- If the text stops following you, scroll to the right spot with the mouse; it picks up again.

## 5. Troubleshooting

| Problem | What to do |
|---|---|
| "Start listening" reports that the models are missing | Reinstall the app; the models are part of it. |
| Level bar stays empty | Check the microphone choice and the operating system's microphone permission (see Install). |
| The text jumps around | Advanced → Reset to defaults. If it keeps happening, raise "Evidence to move" a little. |
| Text is blurry when mirrored | Settings → try a different font size; mirroring is done by the graphics card and should stay sharp. |
| macOS refuses to open the app | Right-click → Open, or System Settings → Privacy & Security → Open Anyway. |
| Windows SmartScreen blocks the installer | More info → Run anyway. |

Your settings are saved automatically. macOS: `~/Library/Application Support/nl.polarityprompt.desktop/`.
Windows: `%APPDATA%\nl.polarityprompt.desktop\`.

## 6. Languages

English is included. Under **Settings → Languages** you can download Dutch, German, French,
Spanish and Portuguese (Brazilian; about 32–47 MB each). The download is checked before it is
installed and can be cancelled; afterwards the language works offline like English. **Remove**
frees the space again. Set the language of a script with `lang: nl` (or `de`, `fr`, `es`, `pt`)
at the top, or pick it under **Controls → Language**. If you pick a language that is not
installed yet, "Start listening" tells you to download it first.

Downloaded models are stored in the app's data folder (Windows:
`%LOCALAPPDATA%\nl.polarityprompt.desktop\models`, macOS:
`~/Library/Application Support/nl.polarityprompt.desktop/models`).

## 7. License and support

PolarityPrompt is free software under the GNU General Public License v3.0 or later: you may use
it for anything, share it and change it; versions you pass on must stay open source under the
same license. The license (`LICENSE.txt`) and the licenses of the components it includes
(`THIRD-PARTY-NOTICES.txt`) are installed next to the app.

Found a bug or have an idea? Open an issue on GitHub. If the app saves you time, you can support
its development via GitHub Sponsors (see the README).
