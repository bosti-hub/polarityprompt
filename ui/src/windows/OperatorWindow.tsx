import { useCallback, useEffect, useState } from "react";
import { setSettings } from "../ipc";
import type { Settings, Snapshot } from "../ipc/types";
import ControlsPanel from "../panels/ControlsPanel";
import AdvancedPanel from "../panels/AdvancedPanel";
import FrameOverlay from "../panels/FrameOverlay";
import SettingsPanel from "../panels/SettingsPanel";
import DocumentView, { type ViewStats } from "../prompter/DocumentView";
import StartScreen from "../prompter/StartScreen";
import PresentationBar from "../panels/PresentationBar";
import { useShortcuts } from "../prompter/useShortcuts";
import { startPresentationStore, tickTimer } from "../store/presentation";
import { applySnapshot } from "../store/snapshot";
import { startLanguagesStore } from "../store/languages";
import { startTranscriptStore } from "../store/transcript";

type Tab = "controls" | "settings" | "advanced";

export default function OperatorWindow({ snapshot }: { snapshot: Snapshot }) {
  const [stats, setStats] = useState<ViewStats | null>(null);
  const [showOverlay, setShowOverlay] = useState(false);
  const [pixelSnap, setPixelSnap] = useState(false);
  const [tab, setTab] = useState<Tab>("controls");

  useEffect(() => {
    startTranscriptStore();
    startLanguagesStore();
    startPresentationStore();
  }, []);

  // The view reports stats once a second; that is the reading timer's clock.
  const onStats = useCallback((s: ViewStats) => {
    setStats(s);
    tickTimer(s.mode === "cruise" || s.mode.startsWith("glide") || s.mode === "hold");
  }, []);
  const toggleOverlay = useCallback(() => setShowOverlay((v) => !v), []);

  // Send the full settings object; Rust validates, persists and broadcasts.
  const update = useCallback(
    (patch: Partial<Settings>) => {
      const next = { ...snapshot.settings, ...patch };
      // Optimistic local apply keeps sliders responsive; Rust's snapshot follows.
      applySnapshot({ ...snapshot, settings: next });
      void setSettings(next);
    },
    [snapshot],
  );
  useShortcuts({ snapshot, update, stats, toggleOverlay });

  const tabs: { id: Tab; label: string }[] = [
    { id: "controls", label: "Controls" },
    { id: "settings", label: "Settings" },
    { id: "advanced", label: "Advanced" },
  ];

  return (
    <div className={`operator${snapshot.settings.sidebar_right ? " sidebar-right" : ""}`}>
      <aside className="sidebar">
        <nav className="tabs">
          {tabs.map((t) => (
            <button key={t.id} className={tab === t.id ? "active" : ""} onClick={() => setTab(t.id)}>
              {t.label}
            </button>
          ))}
        </nav>
        {tab === "controls" && <ControlsPanel snapshot={snapshot} update={update} />}
        {tab === "settings" && <SettingsPanel settings={snapshot.settings} update={update} />}
        {tab === "advanced" && (
          <AdvancedPanel
            document={snapshot.document}
            settings={snapshot.settings}
            update={update}
            stats={stats}
            showOverlay={showOverlay}
            setShowOverlay={setShowOverlay}
            pixelSnap={pixelSnap}
            setPixelSnap={setPixelSnap}
          />
        )}
        <footer className="sidebar-footer">
          PolarityPrompt {snapshot.app_version}
          {snapshot.status.listening ? " · listening" : ""}
        </footer>
      </aside>
      <main className="preview">
        {!snapshot.document && <StartScreen settings={snapshot.settings} />}
        <DocumentView
          document={snapshot.document}
          settings={snapshot.settings}
          mirror={!snapshot.talent_open}
          highlightBlock={snapshot.current_block}
          reportToHost
          onStats={onStats}
          pixelSnap={pixelSnap}
        />
        {snapshot.document && <PresentationBar stats={stats} />}
        {showOverlay && <FrameOverlay stats={stats} />}
      </main>
    </div>
  );
}
