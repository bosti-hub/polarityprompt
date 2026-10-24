// Layout cache: block offsets measured once per layout change, never per frame.
// DocPos <-> px mapping lives here and only here (docs/ARCHITECTURE.md section 5).
//
// Coordinates: `y` is how far the document has scrolled up, in px. y = 0 puts the
// top of the article at the top of the viewport. The read line sits at
// readLinePx from the top of the viewport; a DocPos "is on the read line" when
// y = blockTop + frac * blockHeight - readLinePx.

import type { DocPos } from "../ipc/types";

export interface BlockBox {
  id: number;
  top: number;
  height: number;
}

export interface Layout {
  blocks: BlockBox[];
  articleHeight: number;
  viewportHeight: number;
  readLinePx: number;
  /** px per "line" at the current font settings; cruise speed is lines/sec. */
  linePx: number;
  minY: number;
  maxY: number;
}

export function measureLayout(article: HTMLElement, viewport: HTMLElement, readLine: number, linePx: number): Layout {
  const blocks: BlockBox[] = [];
  for (const el of Array.from(article.children)) {
    const h = el as HTMLElement;
    const id = Number(h.dataset.blockId);
    if (Number.isNaN(id)) continue;
    blocks.push({ id, top: h.offsetTop, height: h.offsetHeight });
  }
  const viewportHeight = viewport.clientHeight;
  const readLinePx = viewportHeight * readLine;
  const articleHeight = article.offsetHeight;
  return {
    blocks,
    articleHeight,
    viewportHeight,
    readLinePx,
    linePx,
    minY: -readLinePx,
    maxY: Math.max(-readLinePx, articleHeight - readLinePx),
  };
}

export function posToPx(layout: Layout, pos: DocPos): number {
  const b = layout.blocks.find((x) => x.id === pos.block) ?? layout.blocks[layout.blocks.length - 1];
  if (!b) return layout.minY;
  const frac = Math.min(1, Math.max(0, pos.frac));
  return clampY(layout, b.top + frac * b.height - layout.readLinePx);
}

export function pxToPos(layout: Layout, y: number): DocPos {
  const line = y + layout.readLinePx;
  const first = layout.blocks[0];
  if (!first) return { block: 0, frac: 0 };
  if (line <= first.top) return { block: first.id, frac: 0 };
  for (const b of layout.blocks) {
    if (line < b.top + b.height) {
      return { block: b.id, frac: b.height > 0 ? (line - b.top) / b.height : 0 };
    }
  }
  const last = layout.blocks[layout.blocks.length - 1]!;
  return { block: last.id, frac: 1 };
}

export function clampY(layout: Layout, y: number): number {
  return Math.min(layout.maxY, Math.max(layout.minY, y));
}
