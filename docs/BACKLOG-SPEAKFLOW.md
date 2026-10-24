# Feature backlog, compared with Speakflow (2026-09-18)

Source: speakflow.com/docs. Speakflow is a browser/cloud teleprompter; we are local and offline,
which is our main difference and stays that way. What follows is what they offer that we do not,
grouped by effort. "Easy" = an afternoon, "medium" = a day or two, "big" = a phase of its own.

## Already covered
Voice-driven scrolling (their "Flow"), fixed-speed scrolling ("Auto"), manual scrolling by mouse
or trackpad, mirroring, font/size/line height/margins/colours, read line, device picker, a second
synced display (our talent window = their Remote for one extra screen), Dutch + English.

## Easy — done 2026-09-18
- **Timer and progress bar.** Elapsed reading time and a thin progress track along the text area,
  with bookmark ticks. All the data is already in the UI.
- **Bookmarks.** `[bookmark]` or a marker in the Markdown (`<!-- bookmark -->` fits our format),
  shown as dots in the margin; `.` and `,` jump to next/previous; click a dot to go there.
- **Keyboard shortcuts panel.** Space start/stop, R restart, arrows one line, `[` `]` script
  width, `+` `-` speed, `1`–`9` speed presets, F/A/M mode. Show them in Settings; customisable later.
- **Click a word to jump there.** We know the block under the click; word-level is a small
  addition since blocks are plain HTML.
- **Dim text above the read line.** A gradient so what you have read fades; keeps the eye on the
  line. CSS only.
- **Script width and horizontal position.** We have left/right margins; a single "width" slider
  plus "position" is friendlier. Cosmetic rework of existing settings.
- **Countdown before start.** 3-2-1 overlay when pressing Start. Small.
- **Skip markers in the script.** Text in `[square brackets]` is shown but ignored by the voice
  tracker (stage directions, names it will never recognise). A few lines in the parser/aligner.
- **Text alignment.** Left/centre. One setting.
- **"Ignore microphone" / mute** toggle without stopping the session.
- **Stop after long silence** (they stop Flow after 30 s). We already hold; an optional auto-stop
  after N minutes of silence is trivial.

## Medium
- **Voice commands.** "Go next", "go previous", "go top", "go current" (restart paragraph). We
  already have the transcript; a small command matcher on the bus, with a settable trigger word,
  in Dutch and English. Must not fire on normal speech (require the trigger word).
- **Scroll mode switch in one place.** Voice / Auto / Manual as an explicit mode selector instead
  of the implicit combination of Start listening + Start scroll. Mostly UI, some controller work.
- **Overlay window.** A borderless, always-on-top, semi-transparent talent window to put over
  Zoom/Meet/Teams. Tauri supports always-on-top, transparency and window decorations; a "Overlay"
  variant of the talent window with an opacity slider. Screen-share invisibility (their desktop app
  claims it) is a macOS-specific window flag; worth a try, not guaranteed.
- **Recent scripts list and folders.** We reopen the last one; a list of recent files with the
  title from the front-matter is easy; folders are just the file system, so a "scripts folder"
  setting with a list is enough.
- **Estimated duration / word count** per script and per block, from the pace model (words per
  minute measured live). Shown next to the title.
- **Per-window settings.** Font size and mirror per window (operator vs talent), which the
  architecture already reserves.

## Big or not for us
- **Video recording with the script** and cloud storage: out of scope; OBS integration (Phase 2)
  covers recording.
- **Remote mode across phones/tablets**: would need a local web server + phones on the same Wi-Fi.
  Feasible offline (Tauri host serves the talent view over LAN) but a phase of its own.
- **Writing assistant (AI rephrase/simplify)**: the `Advisor` seam exists for a local model;
  Phase 3 material.
- **Teams, workspaces, public links, version history, accounts**: cloud features; we have files
  and git-less simplicity. Version history could be "keep the last N copies of a script" later.
- **Import from documents**: they don't have it either; Markdown is our import format.

## Suggested first batch (one session)
Timer + progress bar, bookmarks, dim-above-read-line, click-to-jump, skip markers, countdown,
shortcuts panel. All local, all small, and together they make the presentation screen feel
complete.
