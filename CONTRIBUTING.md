# Contributing

Thanks for wanting to help. Bug reports and ideas are welcome as
[issues](https://github.com/bosti-hub/polarityprompt/issues); for code changes, open an issue
first so we can agree on the approach.

How the app is put together: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (layers, types, IPC),
[docs/DECISIONS.md](docs/DECISIONS.md) (why things are the way they are) and
[docs/ROADMAP.md](docs/ROADMAP.md). [CLAUDE.md](CLAUDE.md) holds the working rules for the
AI assistant that does much of the coding; they apply to humans too.

## Setting up a development machine

### Windows 10/11

Install the tools with winget (each command once; the Build Tools take a while and ask for
administrator permission), then open a new terminal:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

```powershell
winget install --id Rustlang.Rustup -e
```

```powershell
winget install --id OpenJS.NodeJS.22 -e
```

```powershell
winget install --id Git.Git -e
```

```powershell
cargo install tauri-cli --version "^2" --locked
```

Run the `scripts/*.sh` helpers from Git Bash (`bash scripts/fetch-models.sh`).

### macOS (Apple silicon)

```bash
xcode-select --install
```

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
brew install node
```

```bash
cargo install tauri-cli --version "^2" --locked
```

### Both platforms

```bash
npm ci --prefix ui
```

```bash
bash scripts/fetch-models.sh
```

`fetch-models.sh` downloads the small Vosk models for English (bundled with the app) and Dutch
(used by the tests) and the `libvosk` library into `models/` (about 150 MB, not committed).
`--all` adds German, French, Spanish and Portuguese, so the practice-script test covers every
language. In development builds the app also uses the models in `models/`; installed apps only
use their own and downloaded ones.

## Running

```bash
cargo tauri dev
```

Starts the UI dev server, compiles the Rust side and opens the operator window. Changes in `ui/`
reload instantly; Rust changes rebuild and restart the app.

The UI alone, in a browser with mocked IPC (for typography and scroll feel):

```bash
npm run dev --prefix ui
```

Then open http://localhost:1420/ .

## Checks

Every change must pass these (GitHub runs them on macOS and Windows for each push):

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
npm run typecheck --prefix ui && npm run lint --prefix ui
```

## Replaying a recording

The `replay` tool runs a recording through recognition and the full scroll logic and prints
every position estimate and scroll intent, ending with a summary. Without `--script` it prints
the transcript with word timings.

```bash
cargo run -p prompter-speech --bin replay -- fixtures/reading.wav nl --script examples/demo-nl.md
```

Keep your own test recordings in `fixtures/` (git-ignored; voice recordings are personal data
and are not committed). Convert phone or QuickTime recordings to WAV first:

```bash
ffmpeg -i fixtures/reading.m4a -ac 1 -ar 48000 -c:a pcm_s16le fixtures/reading.wav
```

On macOS `afconvert -f WAVE -d LEI16@48000 -c 1 in.m4a out.wav` works too, and
`scripts/synth-reading.sh` makes a synthetic read-through with the macOS Dutch voice.

## Licenses of dependencies

New dependencies need a permissive or GPL-compatible license; ask in the issue first.
After changing dependencies, regenerate the notices file that ships with the installers:

```bash
node scripts/third-party-notices.mjs
```

## Making a release

1. Update the version in `Cargo.toml`, `apps/desktop/tauri.conf.json` and `ui/package.json`,
   and move the "Unreleased" notes in [CHANGELOG.md](CHANGELOG.md) under the new version.
2. Tag and push; GitHub builds the Windows and macOS installers
   (`.github/workflows/release.yml`) and attaches them to a GitHub Release:

```bash
git tag v0.1.0 && git push origin main --tags
```

Local builds: `cargo tauri build` gives the macOS DMG in `target/release/bundle/dmg/`, and on
Windows `cargo tauri build --bundles nsis` gives the installer in `target/release/bundle/nsis/`.

## License of contributions

By contributing you agree that your contribution is licensed under the GPL-3.0-or-later, like
the rest of the project.
