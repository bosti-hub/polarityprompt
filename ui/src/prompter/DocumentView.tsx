// Renders the document as one composited layer and runs the animator.
// Owns pixels and nothing else: it consumes ScrollIntents and Settings, and
// reports two things back to Rust, only on change: glide arrivals and the
// block under the read line.

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { animatorArrived, readLineBlock, readLinePos, scroll } from "../ipc";
import type { Block, Document, ScrollIntent, Settings } from "../ipc/types";
import { subscribeIntents } from "../store/intents";
import { usePresentation } from "../store/presentation";
import { Animator, type FrameStats, type Mode } from "./animator";
import { clampY, measureLayout, posToPx, pxToPos, type Layout } from "./layout";

export interface ViewStats extends FrameStats {
  layoutPasses: number;
  y: number;
  mode: string;
  block: number;
  /** 0..1 through the document. */
  progress: number;
  /** Progress positions of bookmarked blocks. */
  bookmarks: { block: number; at: number; label: string }[];
}

/** Nudge request from the keyboard (arrow keys): lines up/down. */
export const NUDGE_EVENT = "pp:nudge";

interface Props {
  document: Document | null;
  settings: Settings;
  /** Apply the mirror settings to this view (talent window / preview). */
  mirror: boolean;
  /** Block the speech matcher last confirmed; gets a subtle highlight. */
  highlightBlock?: number | null;
  /** Report arrivals and read-line block changes to Rust (one view per window does this). */
  reportToHost?: boolean;
  onStats?: (s: ViewStats) => void;
  /** Rounds the transform to device pixels (debug toggle). */
  pixelSnap?: boolean;
}

function blockClass(b: Block, highlighted: boolean): string {
  const base = (() => {
    switch (b.kind) {
      case "heading":
        return `block block-heading block-heading-${b.level ?? 1}`;
      case "paragraph":
        return "block block-paragraph";
      case "talking_points":
        return "block block-talking-points";
      case "rule":
        return "block block-rule";
    }
  })();
  return highlighted ? `${base} block-current` : base;
}

export default function DocumentView({ document, settings, mirror, highlightBlock, reportToHost, onStats, pixelSnap }: Props) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const articleRef = useRef<HTMLElement>(null);
  const layoutRef = useRef<Layout | null>(null);
  const animRef = useRef<Animator | null>(null);
  const passesRef = useRef(0);
  const statsRef = useRef(onStats);
  const pixelSnapRef = useRef(pixelSnap ?? false);
  const reportRef = useRef(reportToHost ?? false);
  const readLineBlockRef = useRef<number>(-1);
  const docRef = useRef<Document | null>(document);
  useEffect(() => {
    docRef.current = document;
  }, [document]);
  const lastPosReportRef = useRef<number>(0);
  const docKeyRef = useRef<string>("");
  useEffect(() => {
    statsRef.current = onStats;
    pixelSnapRef.current = pixelSnap ?? false;
    reportRef.current = reportToHost ?? false;
  }, [onStats, pixelSnap, reportToHost]);
  const [emptyHint, setEmptyHint] = useState(false);
  const { countdown } = usePresentation();

  const linePx = settings.font_size * settings.line_height;
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;

  // Animator lives for the lifetime of the view.
  useEffect(() => {
    const anim = new Animator({
      render: (y) => {
        const el = articleRef.current;
        if (!el) return;
        const yy = pixelSnapRef.current ? Math.round(y * dpr) / dpr : y;
        el.style.transform = `translate3d(0, ${-yy}px, 0)`;
        // Which block is under the read line? Pure arithmetic on the cached layout.
        const lay = layoutRef.current;
        if (lay && reportRef.current) {
          const pos = pxToPos(lay, y);
          if (pos.block !== readLineBlockRef.current) {
            readLineBlockRef.current = pos.block;
            void readLineBlock(pos.block);
          }
          // Position report for late-joining windows: at most twice a second.
          const now = performance.now();
          if (now - lastPosReportRef.current > 500) {
            lastPosReportRef.current = now;
            void readLinePos(pos);
          }
        }
      },
      onArrived: (seq) => {
        if (reportRef.current) void animatorArrived(seq);
      },
      onStats: (s) => {
        const lay = layoutRef.current;
        const st = anim.state;
        const span = lay ? Math.max(1, lay.maxY - lay.minY) : 1;
        const doc = docRef.current;
        const bookmarks =
          lay && doc
            ? doc.blocks
                .filter((b) => b.bookmark !== undefined)
                .map((b) => {
                  const box = lay.blocks.find((x) => x.id === b.id);
                  return { block: b.id, at: box ? (box.top - lay.readLinePx - lay.minY) / span : 0, label: b.bookmark ?? "" };
                })
            : [];
        statsRef.current?.({
          ...s,
          layoutPasses: passesRef.current,
          y: st.y,
          mode: st.mode.kind === "glide" ? `glide/${st.mode.profile}` : st.mode.kind,
          block: lay ? pxToPos(lay, st.y).block : -1,
          progress: lay ? Math.min(1, Math.max(0, (st.y - lay.minY) / span)) : 0,
          bookmarks,
        });
      },
      bounds: () => {
        const lay = layoutRef.current;
        return lay ? { minY: lay.minY, maxY: lay.maxY } : { minY: 0, maxY: 0 };
      },
    });
    animRef.current = anim;
    const lay = layoutRef.current;
    if (lay) anim.snap(lay.minY);
    anim.start();
    if (import.meta.env.DEV && reportRef.current) {
      // Debug handle for the browser console / automated checks (dev builds only).
      (window as unknown as { __ppAnimator?: Animator }).__ppAnimator = anim;
    }
    return () => anim.stop();
  }, [dpr]);

  // Cruise speed in px follows the settings.
  useEffect(() => {
    const anim = animRef.current;
    if (!anim) return;
    anim.cruiseSpeedPx = settings.base_speed * linePx;
    if (anim.state.mode.kind === "cruise") anim.setMode({ kind: "cruise", speedPx: anim.cruiseSpeedPx });
  }, [settings.base_speed, linePx]);

  // Measure once per layout change: document, typography, viewport size.
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    const article = articleRef.current;
    if (!viewport || !article) return;

    const docKey = document ? `${document.path}#${document.hash}` : "";
    const newDocument = docKey !== docKeyRef.current;
    docKeyRef.current = docKey;

    const remeasure = () => {
      const anim = animRef.current;
      const old = layoutRef.current;
      const keep = old && anim && !newDocument ? pxToPos(old, anim.state.y) : null;
      const glideKeep =
        old && anim && !newDocument && anim.state.mode.kind === "glide" ? pxToPos(old, anim.state.mode.target) : null;
      const next = measureLayout(article, viewport, settings.read_line, linePx);
      layoutRef.current = next;
      passesRef.current += 1;
      if (anim) {
        const y = keep ? posToPx(next, keep) : next.minY;
        anim.rebase(clampY(next, y), glideKeep ? posToPx(next, glideKeep) : undefined);
        if (newDocument) anim.setMode({ kind: "idle" });
      }
      setEmptyHint(next.blocks.length === 0);
    };

    remeasure();
    const fontsReady = typeof window !== "undefined" && "fonts" in window.document ? window.document.fonts.ready : null;
    fontsReady?.then(remeasure).catch(() => {});
    const ro = new ResizeObserver(remeasure);
    ro.observe(viewport);
    ro.observe(article);
    return () => ro.disconnect();
  }, [document, settings.font_family, settings.font_size, settings.line_height, settings.margin_left, settings.margin_right, settings.read_line, linePx]);

  // Manual scroll: wheel/trackpad moves the text directly (an explicit user action),
  // and the host is told where we ended up so the matcher follows.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || !reportToHost) return;
    let timer = 0;
    const onWheel = (e: WheelEvent) => {
      const anim = animRef.current;
      const lay = layoutRef.current;
      if (!anim || !lay) return;
      e.preventDefault();
      const dy = e.deltaMode === 1 ? e.deltaY * lay.linePx : e.deltaMode === 2 ? e.deltaY * lay.viewportHeight : e.deltaY;
      anim.nudge(settings.mirror_v ? -dy : dy);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const l = layoutRef.current;
        const a = animRef.current;
        if (l && a) void scroll({ type: "reposition", pos: pxToPos(l, a.state.y) });
      }, 250);
    };
    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      viewport.removeEventListener("wheel", onWheel);
      window.clearTimeout(timer);
    };
  }, [reportToHost, settings.mirror_v]);

  // Arrow keys (from the shortcuts hook) nudge by whole lines.
  useEffect(() => {
    if (!reportToHost) return;
    const onNudge = (e: Event) => {
      const lines = (e as CustomEvent<number>).detail;
      const anim = animRef.current;
      const lay = layoutRef.current;
      if (!anim || !lay) return;
      anim.nudge(lines * lay.linePx);
      void scroll({ type: "reposition", pos: pxToPos(lay, anim.state.y) });
    };
    window.addEventListener(NUDGE_EVENT, onNudge);
    return () => window.removeEventListener(NUDGE_EVENT, onNudge);
  }, [reportToHost]);

  // Click a spot in the text to bring it to the read line; click a bookmark dot to jump and hold.
  const onArticleClick = (e: React.MouseEvent<HTMLElement>) => {
    if (!reportToHost) return;
    const anim = animRef.current;
    const lay = layoutRef.current;
    const viewport = viewportRef.current;
    if (!anim || !lay || !viewport) return;
    const dot = (e.target as HTMLElement).closest("[data-bookmark-block]") as HTMLElement | null;
    if (dot) {
      e.preventDefault();
      const block = Number(dot.dataset.bookmarkBlock);
      void scroll({ type: "jump_hold", pos: { block, frac: 0 } });
      return;
    }
    const rect = viewport.getBoundingClientRect();
    let rel = e.clientY - rect.top;
    if (settings.mirror_v && mirror) rel = rect.height - rel;
    const target = clampY(lay, anim.state.y + rel - lay.readLinePx);
    anim.snap(target);
    if (anim.state.mode.kind === "glide") anim.setMode({ kind: "hold" });
    void scroll({ type: "reposition", pos: pxToPos(lay, target) });
  };

  // Intents → animator modes. The only mutation path for motion.
  useEffect(() => {
    return subscribeIntents((intent: ScrollIntent) => {
      const anim = animRef.current;
      const lay = layoutRef.current;
      if (!anim || !lay) return;
      switch (intent.type) {
        case "cruise":
          anim.cruiseSpeedPx = intent.speed * lay.linePx;
          anim.setMode({ kind: "cruise", speedPx: anim.cruiseSpeedPx });
          break;
        case "hold":
          anim.setMode({ kind: "hold" });
          break;
        case "pause": {
          const m = anim.state.mode;
          if (m.kind === "paused") anim.setMode(m.resume);
          else anim.setMode({ kind: "paused", resume: m });
          break;
        }
        case "glide_to": {
          const target = posToPx(lay, intent.pos);
          const close = Math.abs(target - anim.state.y) < 3 * lay.linePx;
          const mode: Mode = {
            kind: "glide",
            target,
            profile: intent.profile === "near" && close ? "soft" : intent.profile,
            then: intent.then,
            seq: intent.seq,
          };
          anim.setMode(mode);
          break;
        }
        case "snap":
          anim.snap(posToPx(lay, intent.pos));
          if (anim.state.mode.kind === "glide") anim.setMode({ kind: "hold" });
          break;
      }
    });
  }, []);

  const mirrorTransform = mirror
    ? `scale(${settings.mirror_h ? -1 : 1}, ${settings.mirror_v ? -1 : 1})`
    : "none";

  const viewportStyle: CSSProperties = {
    background: settings.color_bg,
    color: settings.color_fg,
    fontFamily: `"${settings.font_family}", system-ui, sans-serif`,
    fontSize: `${settings.font_size}px`,
    lineHeight: settings.line_height,
    transform: mirrorTransform,
  };
  const articleStyle: CSSProperties = {
    paddingLeft: `${settings.margin_left * 100}%`,
    paddingRight: `${settings.margin_right * 100}%`,
    textAlign: settings.align === "center" ? "center" : "left",
  };
  const readLineStyle: CSSProperties = { top: `${settings.read_line * 100}%` };
  const dimStyle: CSSProperties = {
    height: `${settings.read_line * 100}%`,
    background: `linear-gradient(to bottom, ${settings.color_bg} 0%, transparent 100%)`,
    opacity: settings.dim_above,
  };

  return (
    <div className="prompter-viewport" ref={viewportRef} style={viewportStyle}>
      <article className="prompter-article" ref={articleRef} style={articleStyle} onClick={onArticleClick}>
        {document?.blocks.map((b) =>
          b.kind === "rule" ? (
            <hr key={b.id} className={blockClass(b, false)} data-block-id={b.id} />
          ) : (
            <div key={b.id} className={blockClass(b, highlightBlock === b.id)} data-block-id={b.id}>
              {b.bookmark !== undefined && (
                <span className="bookmark-dot" data-bookmark-block={b.id} title={b.bookmark || "Bookmark"} role="button" />
              )}
              <span dangerouslySetInnerHTML={{ __html: b.html }} />
            </div>
          ),
        )}
      </article>
      {settings.dim_above > 0 && <div className="dim-above" style={dimStyle} aria-hidden="true" />}
      <div className="read-line" style={readLineStyle} aria-hidden="true" />
      {countdown > 0 && (
        <div className="countdown" aria-live="assertive">
          {countdown}
        </div>
      )}
      {emptyHint && document && <div className="prompter-empty">This script has no text.</div>}
    </div>
  );
}
