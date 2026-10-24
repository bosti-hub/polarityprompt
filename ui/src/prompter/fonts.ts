// Detects whether a font family is installed by measuring text width against
// generic fallbacks. Cheap (one canvas, a few ms for ~30 families), no Rust needed.

let canvas: CanvasRenderingContext2D | null = null;
const cache = new Map<string, boolean>();

function width(font: string): number {
  if (!canvas) canvas = document.createElement("canvas").getContext("2d");
  if (!canvas) return 0;
  canvas.font = font;
  return canvas.measureText("mmmmmmmmmmlliWWQ@1234").width;
}

export function isFontAvailable(family: string): boolean {
  const hit = cache.get(family);
  if (hit !== undefined) return hit;
  const q = `"${family.replace(/"/g, "")}"`;
  const ok =
    width(`72px ${q}, monospace`) !== width("72px monospace") ||
    width(`72px ${q}, serif`) !== width("72px serif") ||
    width(`72px ${q}, sans-serif`) !== width("72px sans-serif");
  cache.set(family, ok);
  return ok;
}

export function availableFonts(candidates: string[]): string[] {
  return candidates.filter(isFontAvailable);
}
