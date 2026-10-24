// Shown in the document area while no script is open.

import { openDocumentDialog, openPracticeScript, reopenLastDocument } from "../ipc";
import { LANGUAGES, type Settings } from "../ipc/types";

export default function StartScreen({ settings }: { settings: Settings }) {
  const last = settings.last_document?.split(/[\\/]/).pop();
  const languageName = LANGUAGES.find((l) => l.lang === settings.default_language)?.name ?? settings.default_language;
  return (
    <div className="start-screen" style={{ background: settings.color_bg, color: settings.color_fg }}>
      <svg className="logo" viewBox="0 0 120 120" width="140" height="140" aria-hidden="true">
        <circle cx="60" cy="60" r="44" fill="#ec7942" />
        <path d="M 60 16 A 44 44 0 0 0 60 104 Z" fill="#121214" opacity="0.25" />
        <circle cx="60" cy="60" r="8" fill="#121214" />
      </svg>
      <h1>
        Polarity<span>Prompt</span>
      </h1>
      <p>Open a Markdown script to begin.</p>
      <div className="start-actions">
        <button onClick={() => void openDocumentDialog()}>Open script…</button>
        {last && (
          <button onClick={() => void reopenLastDocument()} title={settings.last_document ?? undefined}>
            Reopen {last}
          </button>
        )}
      </div>
      <p className="start-practice-label">Or try it with a short practice script:</p>
      <div className="start-actions">
        <button onClick={() => void openPracticeScript(settings.default_language)}>Practice script · {languageName}</button>
      </div>
      <p className="start-practice-hint">Change the language under Controls → Language.</p>
    </div>
  );
}
