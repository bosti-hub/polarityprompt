// The animator: one requestAnimationFrame loop, dt-based, critically damped spring
// in pixels. It owns motion; Rust (or in phase 0 the dev panel) owns intent.
// Pure TypeScript apart from requestAnimationFrame, so the spring math is testable.

import type { AfterGlide, GlideProfile } from "../ipc/types";

export type Mode =
  | { kind: "idle" }
  | { kind: "cruise"; speedPx: number }
  | { kind: "hold" }
  | { kind: "paused"; resume: Mode }
  | { kind: "glide"; target: number; profile: GlideProfile | "soft"; then: AfterGlide; seq: number };

interface SpringParams {
  /** Natural angular frequency (rad/s). Higher = stiffer, faster. */
  omega: number;
  /** Max speed in px/s, or Infinity. */
  vmax: number;
}

export const SPRINGS: Record<GlideProfile | "soft", SpringParams> = {
  // Word-level tracking sends many small glides; those use "soft" (chosen by the
  // view when the target is close) so the text flows instead of ticking.
  soft: { omega: 5, vmax: 900 },
  near: { omega: 14, vmax: Infinity },
  skip: { omega: 9, vmax: 2600 },
  far: { omega: 6, vmax: 1800 },
};

const ARRIVE_PX = 0.5;
const ARRIVE_V = 2; // px/s
const HOLD_DAMPING = 12; // 1/s; how fast we brake into a hold
const CRUISE_RAMP = 10; // 1/s; how fast cruise speed is reached

export interface FrameStats {
  dtMs: number;
  worstMs: number;
  fps: number;
}

export interface AnimatorState {
  y: number;
  v: number;
  mode: Mode;
}

/** One physics step. Returns the arrived seq when a glide lands this step. */
export function step(state: AnimatorState, dt: number, cruiseSpeedPx: number, minY: number, maxY: number): number | null {
  let arrived: number | null = null;
  const m = state.mode;
  switch (m.kind) {
    case "idle":
      state.v = 0;
      break;
    case "cruise": {
      const target = m.speedPx;
      state.v += (target - state.v) * Math.min(1, dt * CRUISE_RAMP);
      state.y += state.v * dt;
      break;
    }
    case "hold":
    case "paused": {
      state.v *= Math.exp(-HOLD_DAMPING * dt);
      if (Math.abs(state.v) < ARRIVE_V) state.v = 0;
      state.y += state.v * dt;
      break;
    }
    case "glide": {
      const p = SPRINGS[m.profile];
      const x = state.y - m.target;
      const a = -2 * p.omega * state.v - p.omega * p.omega * x;
      state.v += a * dt;
      if (Math.abs(state.v) > p.vmax) state.v = Math.sign(state.v) * p.vmax;
      state.y += state.v * dt;
      if (Math.abs(state.y - m.target) < ARRIVE_PX && Math.abs(state.v) < ARRIVE_V) {
        state.y = m.target;
        state.v = 0;
        arrived = m.seq;
        state.mode = m.then === "cruise" ? { kind: "cruise", speedPx: cruiseSpeedPx } : { kind: "hold" };
      }
      break;
    }
  }
  if (state.y < minY) {
    state.y = minY;
    if (state.v < 0) state.v = 0;
  }
  if (state.y > maxY) {
    state.y = maxY;
    if (state.v > 0) state.v = 0;
    if (state.mode.kind === "cruise") state.mode = { kind: "hold" };
  }
  return arrived;
}

export interface AnimatorHooks {
  render: (y: number) => void;
  onArrived: (seq: number) => void;
  onStats?: (s: FrameStats) => void;
  bounds: () => { minY: number; maxY: number };
}

export class Animator {
  state: AnimatorState = { y: 0, v: 0, mode: { kind: "idle" } };
  cruiseSpeedPx = 0;
  private raf = 0;
  private last = 0;
  private lastRendered = NaN;
  private worst = 0;
  private worstSince = 0;
  private frames = 0;
  private fpsSince = 0;
  private fps = 0;

  constructor(private hooks: AnimatorHooks) {}

  start() {
    if (this.raf) return;
    this.last = performance.now();
    this.worstSince = this.last;
    this.fpsSince = this.last;
    const tick = (now: number) => {
      this.raf = requestAnimationFrame(tick);
      const dtMs = now - this.last;
      this.last = now;
      // Cap dt so a hidden tab or a long stall does not launch the text across the screen.
      const dt = Math.min(dtMs, 50) / 1000;
      const { minY, maxY } = this.hooks.bounds();
      const arrived = step(this.state, dt, this.cruiseSpeedPx, minY, maxY);
      if (this.state.y !== this.lastRendered) {
        this.lastRendered = this.state.y;
        this.hooks.render(this.state.y);
      }
      if (arrived !== null) this.hooks.onArrived(arrived);
      this.frames += 1;
      if (dtMs > this.worst) this.worst = dtMs;
      if (now - this.fpsSince >= 500) {
        this.fps = (this.frames * 1000) / (now - this.fpsSince);
        this.frames = 0;
        this.fpsSince = now;
      }
      if (now - this.worstSince >= 1000) {
        this.hooks.onStats?.({ dtMs, worstMs: this.worst, fps: this.fps });
        this.worst = 0;
        this.worstSince = now;
      }
    };
    this.raf = requestAnimationFrame(tick);
  }

  stop() {
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  setMode(mode: Mode) {
    this.state.mode = mode;
  }

  /** Jump without animation. Only ever from an explicit user action (rule 4). */
  snap(y: number) {
    this.state.y = y;
    this.state.v = 0;
    this.lastRendered = NaN;
  }

  /** Manual scroll by the user: move now, cancel a running glide, keep cruising if cruising. */
  nudge(dy: number) {
    const { minY, maxY } = this.hooks.bounds();
    this.state.y = Math.min(maxY, Math.max(minY, this.state.y + dy));
    if (this.state.mode.kind === "glide") this.state.mode = { kind: "hold" };
    if (this.state.mode.kind !== "cruise") this.state.v = 0;
    this.lastRendered = NaN;
  }

  /** Keep the same document position when the layout changes under us. */
  rebase(y: number, glideTarget?: number) {
    this.state.y = y;
    this.lastRendered = NaN;
    if (this.state.mode.kind === "glide" && glideTarget !== undefined) {
      this.state.mode = { ...this.state.mode, target: glideTarget };
    }
  }
}
