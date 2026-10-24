// Copies the committed source (HEAD) into a new folder, without the files that must stay
// private, as the starting point of the public repository.
// It does not run git in the new folder; that is a separate, deliberate step.
//
//   node scripts/make-public-snapshot.mjs ../polarityprompt-public

import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Never public. Recordings of Floris's voice (personal data), the macOS voice output, and the
// polarity script (Floris's own content; removed from the tree, listed here as a safety net);
// KICKOFF.md is the private first-day setup (CONTRIBUTING.md replaces it).
const EXCLUDE = [
  "examples/realistische_opname.m4a",
  "examples/reguliere_opname.m4a",
  "examples/voorbeeld-fragment-synth.wav",
  "examples/voorbeeld-script.md",
  "KICKOFF.md",
  "docs/DEVLOG.md",
  "docs/PUBLISHING.md",
];

// Private files that the new folder still needs for daily work. They are copied from this
// checkout but are git-ignored there (.gitignore), so they never end up in a public commit.
const PRIVATE_LOCAL = ["docs/DEVLOG.md", "docs/PUBLISHING.md", "fixtures"];

const target = process.argv[2];
if (!target) {
  console.error("usage: node scripts/make-public-snapshot.mjs <new-folder>");
  process.exit(2);
}
const dest = resolve(target);
if (existsSync(dest) && readdirSync(dest).length > 0) {
  console.error(`${dest} exists and is not empty; choose a new folder.`);
  process.exit(1);
}
const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: root }).toString().trim();
if (dirty) console.warn("note: uncommitted changes are not included; the snapshot is HEAD.\n");

const files = execFileSync("git", ["ls-files", "-z"], { cwd: root }).toString().split("\0").filter(Boolean);
const missing = EXCLUDE.filter((f) => !files.includes(f));
if (missing.length) console.warn(`note: already absent: ${missing.join(", ")}`);
const keep = files.filter((f) => !EXCLUDE.includes(f));

mkdirSync(dest, { recursive: true });
for (const file of keep) {
  // Content exactly as stored in git at HEAD (LF line endings, binary files untouched).
  const content = execFileSync("git", ["show", `HEAD:${file}`], { cwd: root, maxBuffer: 256 * 1024 * 1024 });
  mkdirSync(dirname(join(dest, file)), { recursive: true });
  writeFileSync(join(dest, file), content);
}

for (const p of PRIVATE_LOCAL) {
  if (existsSync(join(root, p))) cpSync(join(root, p), join(dest, p), { recursive: true });
}

const head = execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd: root }).toString().trim();
console.log(`Copied ${keep.length} files from ${head} to ${dest}`);
console.log(`Left out: ${EXCLUDE.join(", ")}`);
console.log(`Copied as private, git-ignored files: ${PRIVATE_LOCAL.join(", ")}`);
console.log("Next (only after review): git init -b main, first commit, create the public repository.");
