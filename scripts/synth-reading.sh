#!/usr/bin/env bash
# Synthesizes a "reading" of crates/prompter-core/testdata/zuurdesem.md with the macOS Dutch
# voice, for offline tuning with the replay tool. The bullet lists are replaced by improvised
# talk and one paragraph is skipped, so both behaviours can be checked. macOS only (`say`).
# Usage: scripts/synth-reading.sh [out.wav]   (default: fixtures/zuurdesem-reading.wav)
set -euo pipefail
OUT="${1:-fixtures/zuurdesem-reading.wav}"
VOICE="${VOICE:-Xander}"
RATE="${RATE:-160}"
mkdir -p "$(dirname "$OUT")"

read -r -d '' TEXT <<'TXT' || true
Waarom zuurdesem?
Smaak ontstaat niet uit gist maar uit zuur. Het verschil tussen twee broden is precies wat de bakker proeft, en dat verschil is geen toeval maar iets wat je kunt begrijpen en beïnvloeden.
Ik wil hier even iets vertellen wat niet in het script staat. Vorige week bakte ik een brood dat helemaal mislukte, en dat was eigenlijk best grappig om te zien. Een wijnmaker laat zijn druiven ook rustig gisten, en zo werkt het eigenlijk ook met deeg.
Daarom kijken we vandaag naar drie onderdelen die dat verschil vergroten of juist verpesten.
Onderdeel één: voorbereiding.
Het eerste onderdeel is voorbereiding. Wie de voorbereiding serieus neemt bepaalt de richting van het brood, en dat wordt vaker geproefd dan gezien.
Onderdeel twee: rijzen.
Het tweede onderdeel is rijzen, en dat is het meest verkeerd begrepen concept in dit hele recept. Rijzen is geen wachttijd en geen gok; het is de tijd waarin het aroma ontstaat.
Even iets over de fout van te vroeg bakken, en hoe dat in de winter verandert. Daar kan ik heel lang over praten maar dat doe ik nu niet.
Onderdeel drie: hitte.
Het derde onderdeel lijkt het eerste tegen te spreken. Hitte, op het juiste moment, is wat deeg omzet in korst.
Volgende week gaan we dieper in op hoe je dit toepast in een keuken die al jaren draait.
TXT

say -v "$VOICE" -r "$RATE" -o "$OUT" --data-format=LEI16@48000 "$TEXT"
echo "wrote $OUT ($(du -h "$OUT" | cut -f1)); skipped paragraph: 'Let op: het gaat hier niet om snelheid…' (block 6)"
echo "replay: cargo run -p prompter-speech --bin replay -- $OUT nl --script crates/prompter-core/testdata/zuurdesem.md"
