// Settings → Languages: bundled and downloadable speech models.

import { useState } from "react";
import { cancelDownload, downloadLanguage, removeLanguage } from "../ipc";
import type { DownloadDto, LanguageInfo } from "../ipc/types";
import { refreshLanguages, useLanguages } from "../store/languages";

function progressText(p: DownloadDto): string {
  switch (p.state) {
    case "downloading":
      return `${Math.round(p.received / 1_000_000)} of ${Math.round(p.total / 1_000_000)} MB`;
    case "verifying":
      return "checking…";
    case "installing":
      return "installing…";
    default:
      return "";
  }
}

export default function LanguagesSection() {
  const { languages, progress } = useLanguages();
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(String(e));
    }
    refreshLanguages();
  };

  const row = (l: LanguageInfo) => {
    const p = progress[l.lang];
    const busy = l.downloading || (p !== undefined && ["downloading", "verifying", "installing"].includes(p.state));
    return (
      <div key={l.lang} className="language-row">
        <span className="language-name">{l.name}</span>
        {busy && p ? (
          <>
            <progress max={p.total || 1} value={p.state === "downloading" ? p.received : p.total} />
            <span className="language-status">{progressText(p)}</span>
            {p.state === "downloading" && <button onClick={() => void run(() => cancelDownload(l.lang))}>Cancel</button>}
          </>
        ) : l.bundled ? (
          <span className="language-status">included</span>
        ) : l.installed ? (
          <>
            <span className="language-status">installed</span>
            <button onClick={() => void run(() => removeLanguage(l.lang))}>Remove</button>
          </>
        ) : (
          <button onClick={() => void run(() => downloadLanguage(l.lang))}>Download ({l.download_mb} MB)</button>
        )}
        {p?.state === "failed" && !busy && <p className="error">{p.error}</p>}
      </div>
    );
  };

  return (
    <>
      <h2>Languages</h2>
      <p className="hint">
        Speech models for following your voice. English is included; other languages are downloaded once
        (from GitHub, or the Vosk website as a fallback) and then work offline.
      </p>
      {languages.map(row)}
      {error && <p className="error">{error}</p>}
    </>
  );
}
