// Thin bar at the bottom of the preview: reading time and progress with bookmark ticks.

import { scroll } from "../ipc";
import type { ViewStats } from "../prompter/DocumentView";
import { usePresentation } from "../store/presentation";

function fmt(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function PresentationBar({ stats }: { stats: ViewStats | null }) {
  const { elapsed } = usePresentation();
  const progress = stats?.progress ?? 0;
  return (
    <div className="presentation-bar" aria-hidden="true">
      <span className="time">{fmt(elapsed)}</span>
      <div className="track">
        <div className="fill" style={{ width: `${progress * 100}%` }} />
        {stats?.bookmarks.map((b) => (
          <button
            key={b.block}
            className="tick"
            style={{ left: `${Math.min(100, Math.max(0, b.at * 100))}%` }}
            title={b.label || `Bookmark (block ${b.block})`}
            onClick={() => void scroll({ type: "jump_hold", pos: { block: b.block, frac: 0 } })}
          />
        ))}
      </div>
      <span className="pct">{Math.round(progress * 100)}%</span>
    </div>
  );
}
