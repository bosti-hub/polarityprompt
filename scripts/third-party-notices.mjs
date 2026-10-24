// Writes THIRD-PARTY-NOTICES.txt: every third-party component that ends up inside the
// installed app, with its license texts. Run after a build (the crate sources must be in
// the cargo registry) whenever dependencies change:
//
//   node scripts/third-party-notices.mjs
//
// Sources: `cargo metadata` for the Windows and macOS targets (normal dependencies of the
// desktop app only, no dev/build tools), ui/package-lock.json (runtime packages only), and
// licenses/ for the parts that are not crates (Vosk, its models, the MinGW runtime DLLs).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const TARGETS = ["x86_64-pc-windows-msvc", "aarch64-apple-darwin"];
const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense)([-._].*)?$/i;

// ---- Rust crates compiled into the app
const crates = new Map(); // "name version" -> { name, version, license, dir }
for (const target of TARGETS) {
  const out = execFileSync("cargo", ["metadata", "--format-version", "1", "--filter-platform", target], {
    cwd: root,
    maxBuffer: 256 * 1024 * 1024,
  });
  const meta = JSON.parse(out.toString());
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const start = meta.packages.find((p) => p.name === "polarityprompt").id;
  const seen = new Set();
  const stack = [start];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id).deps) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  for (const id of seen) {
    const p = byId.get(id);
    if (p.source == null) continue; // our own crates
    crates.set(`${p.name} ${p.version}`, {
      name: p.name,
      version: p.version,
      license: p.license ?? "see license file",
      authors: p.authors ?? [],
      dir: dirname(p.manifest_path),
    });
  }
}

// ---- npm packages shipped in the UI bundle (not marked dev in the lock file)
const lock = JSON.parse(readFileSync(join(root, "ui/package-lock.json"), "utf8"));
const npm = [];
for (const [path, p] of Object.entries(lock.packages)) {
  if (!path || p.dev || p.devOptional) continue;
  const name = path.replace(/^.*node_modules\//, "");
  npm.push({ name, version: p.version, license: p.license ?? "see license file", dir: join(root, "ui", path) });
}

function licenseFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => LICENSE_FILE.test(f))
    .sort()
    .map((f) => readFileSync(join(dir, f), "utf8").replace(/\r\n/g, "\n").trim());
}

// A package without license files gets the standard text of (one of) its declared
// licenses, with its authors as the copyright holders.
function standardText(c) {
  const holders = c.authors?.length ? c.authors.join(", ") : `the ${c.name} authors`;
  const template = (file) => readFileSync(join(root, "licenses", file), "utf8").replace("{{COPYRIGHT}}", holders);
  if (/\bMIT\b/.test(c.license)) return [template("MIT.template.txt")];
  if (/Apache-2\.0/.test(c.license)) return [readFileSync(join(root, "licenses/vosk-api.txt"), "utf8")];
  if (/BSD-3-Clause/.test(c.license)) return [template("BSD-3-Clause.template.txt")];
  if (/MPL-2\.0/.test(c.license)) return [readFileSync(join(root, "licenses/MPL-2.0.txt"), "utf8")];
  return [];
}

function withTexts(c) {
  const texts = licenseFiles(c.dir);
  return { ...c, texts: texts.length ? texts : standardText(c), standard: texts.length === 0 };
}

// ---- Components that are not crates or npm packages
const fixed = [
  {
    name: "Vosk speech recognition library (libvosk)",
    version: "0.3.44 (macOS) / 0.3.45 (Windows)",
    license: "Apache-2.0",
    texts: [readFileSync(join(root, "licenses/vosk-api.txt"), "utf8")],
  },
  {
    name: "Vosk model vosk-model-small-en-us-0.15 (Alpha Cephei Inc.; other languages are downloaded on request, also Apache-2.0)",
    version: "",
    license: "Apache-2.0",
    texts: [readFileSync(join(root, "licenses/vosk-api.txt"), "utf8")],
  },
  {
    name: "GCC runtime libraries libstdc++-6.dll and libgcc_s_seh-1.dll (Windows only)",
    version: "",
    license: "GPL-3.0-or-later WITH GCC-exception-3.1",
    texts: [
      "These libraries are part of GCC and are distributed under the GNU General Public License,\n" +
        "version 3 or later (see LICENSE), with the GCC Runtime Library Exception below.",
      readFileSync(join(root, "licenses/gcc-runtime-library-exception-3.1.txt"), "utf8"),
    ],
  },
  {
    name: "mingw-w64 winpthreads libwinpthread-1.dll (Windows only)",
    version: "",
    license: "MIT and BSD-3-Clause",
    texts: [readFileSync(join(root, "licenses/winpthreads.txt"), "utf8")],
  },
];

// ---- Assemble: a component list, then each distinct license text once with its users.
const components = [
  ...fixed,
  ...[...crates.values()].sort((a, b) => a.name.localeCompare(b.name)).map(withTexts),
  ...npm.sort((a, b) => a.name.localeCompare(b.name)).map(withTexts),
];

const texts = new Map(); // hash -> { text, users[] }
for (const c of components) {
  const label = c.version ? `${c.name} ${c.version}` : c.name;
  for (const t of c.texts) {
    const text = t.replace(/\r\n/g, "\n").trim();
    const hash = createHash("sha256").update(text.replace(/\s+/g, " ")).digest("hex");
    if (!texts.has(hash)) texts.set(hash, { text, users: [] });
    texts.get(hash).users.push(label);
  }
}

const rule = "=".repeat(78);
let out = `PolarityPrompt includes the third-party components listed below.
PolarityPrompt itself is Copyright (c) 2026 F. Bos - OnPolarity.com and licensed under the
GNU General Public License v3.0 or later (LICENSE).

Generated by scripts/third-party-notices.mjs. Do not edit by hand.

${rule}
COMPONENTS
${rule}

`;
for (const c of components) {
  const note = c.texts.length === 0 ? "   (no license text found)" : c.standard ? "   (standard license text; the package ships none)" : "";
  out += `${c.version ? `${c.name} ${c.version}` : c.name} — ${c.license}${note}\n`;
}
out += `\n${rule}\nLICENSE TEXTS\n${rule}\n`;
for (const { text, users } of texts.values()) {
  out += `\n${"-".repeat(78)}\nUsed by: ${users.join(", ")}\n${"-".repeat(78)}\n\n${text}\n`;
}

writeFileSync(join(root, "THIRD-PARTY-NOTICES.txt"), out);
console.log(`THIRD-PARTY-NOTICES.txt: ${components.length} components, ${texts.size} distinct license texts, ${Math.round(out.length / 1024)} KB`);
const standard = components.filter((c) => c.standard && c.texts.length).map((c) => c.name);
if (standard.length) console.log(`standard text added (package ships no license file): ${standard.join(", ")}`);
const none = components.filter((c) => c.texts.length === 0).map((c) => `${c.name} (${c.license})`);
if (none.length) {
  console.error(`no license text for: ${none.join(", ")}`);
  process.exitCode = 1;
}
