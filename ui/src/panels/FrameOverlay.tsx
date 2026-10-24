import type { ViewStats } from "../prompter/DocumentView";

export default function FrameOverlay({ stats }: { stats: ViewStats | null }) {
  if (!stats) return <div className="frame-overlay">waiting for frames…</div>;
  const warn = stats.worstMs > 20;
  return (
    <div className={`frame-overlay${warn ? " warn" : ""}`}>
      <div>{stats.fps.toFixed(0)} fps · last {stats.dtMs.toFixed(1)} ms · worst {stats.worstMs.toFixed(1)} ms</div>
      <div>
        layout passes {stats.layoutPasses} · y {stats.y.toFixed(1)} · block {stats.block} · {stats.mode}
      </div>
    </div>
  );
}
