// Builds public/third-party-notices.txt - the licence texts of every library that ships inside
// the app (Rust crates for Windows and Android, npm packages of the interface, Android libraries).
// Runs by itself before every build (`npm run build`, also inside `tauri build` and the Android
// build) and in the pre-commit hook when a lock file changes; by hand: `npm run notices`.

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "public", "third-party-notices.txt");
// Every platform the app is built for.
const TARGETS = ["x86_64-pc-windows-msvc", "aarch64-linux-android", "x86_64-linux-android"];
const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense)([-._].*)?$/i;

function licenseFiles(dir, deep) {
  if (!existsSync(dir)) return [];
  const found = [];
  for (const name of readdirSync(dir).sort()) {
    const path = join(dir, name);
    const stat = statSync(path);
    if (stat.isFile() && LICENSE_FILE.test(name)) found.push(path);
    // Vendored C sources (OpenSSL, SQLCipher) keep their licence one folder down.
    else if (deep && stat.isDirectory()) found.push(...licenseFiles(path, false));
  }
  return found;
}

const read = (path) => readFileSync(path, "utf8").replace(/\r\n/g, "\n").trim();

// ---------- Rust ----------
const crates = new Map();
for (const target of TARGETS) {
  const meta = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--locked", "--filter-platform", target, "--manifest-path", join(ROOT, "src-tauri", "Cargo.toml")],
      { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
    ),
  );
  const packages = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const isMacro = (p) => p.targets.some((t) => t.kind.includes("proc-macro"));
  // Only what is compiled into the app: normal dependencies, no macros, no build tools - except
  // `*-src` crates, whose C code (OpenSSL, SQLCipher) a build script compiles into the app.
  const seen = new Set();
  const queue = [meta.resolve.root];
  while (queue.length) {
    const id = queue.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      const p = packages.get(dep.pkg);
      if (!p || isMacro(p)) continue;
      if (dep.dep_kinds.some((k) => k.kind === null)) queue.push(dep.pkg);
      else if (p.name.endsWith("-src") && dep.dep_kinds.some((k) => k.kind === "build")) seen.add(dep.pkg);
    }
  }
  seen.delete(meta.resolve.root);
  for (const id of seen) {
    const p = packages.get(id);
    const dir = dirname(p.manifest_path);
    const files = licenseFiles(dir, p.name.endsWith("-src") || p.name === "libsqlite3-sys");
    if (p.license_file) files.push(join(dir, p.license_file));
    crates.set(`${p.name} ${p.version}`, {
      license: p.license ?? "see the files",
      files: [...new Set(files)],
      home: p.repository ?? "",
      authors: p.authors ?? [],
    });
  }
}

// ---------- npm (only packages that ship in the interface) ----------
const lock = JSON.parse(readFileSync(join(ROOT, "package-lock.json"), "utf8"));
const npm = new Map();
for (const [path, p] of Object.entries(lock.packages)) {
  if (!path.startsWith("node_modules/") || p.dev || p.devOptional || p.optional) continue;
  const name = path.slice("node_modules/".length);
  if (!existsSync(join(ROOT, path))) {
    console.error(`missing ${path} - run npm install first (the licence texts would be incomplete)`);
    process.exit(1);
  }
  npm.set(`${name} ${p.version}`, { license: p.license ?? "see the files", files: licenseFiles(join(ROOT, path), false), home: "", authors: [] });
}

// ---------- Android libraries (APK only, from Maven) ----------
const gradle = read(join(ROOT, "src-tauri", "gen", "android", "app", "build.gradle.kts"));
const android = [...gradle.matchAll(/implementation\("([^"]+)"\)/g)].map((m) => m[1]);

// ---------- one copy of every distinct text (per file: Apache-2.0 once, each MIT notice kept) ----------
const groups = new Map();
const missing = [];
for (const [who, info] of [...crates, ...npm].sort(([a], [b]) => a.localeCompare(b))) {
  if (info.files.length === 0) {
    const by = info.authors.length ? ` - Copyright (c) ${info.authors.join(", ")}` : "";
    missing.push(`${who} - ${info.license}${by}${info.home ? ` - ${info.home}` : ""}`);
    continue;
  }
  for (const file of info.files) {
    const text = read(file);
    const key = text.replace(/\s+/g, " ");
    const group = groups.get(key) ?? { text, who: [], licenses: new Set() };
    if (!group.who.includes(who)) group.who.push(who);
    group.licenses.add(info.license);
    groups.set(key, group);
  }
}

const line = "=".repeat(78);
const parts = [
  "Tablica Ofert - third-party notices",
  "",
  "Tablica Ofert is MIT-licensed. It is built with the libraries below; each keeps its own licence.",
  `Rust crates: ${crates.size}, npm packages: ${npm.size}. Generated by scripts/third-party-notices.mjs`,
  "from Cargo.lock and package-lock.json. SQLite (inside SQLCipher) is in the public domain.",
  "",
];
for (const g of groups.values()) {
  parts.push(line, `Used by: ${g.who.join(", ")}`, `Licence: ${[...g.licenses].join("; ")}`, line, "", g.text, "");
}
if (android.length) {
  parts.push(
    line,
    "Android only (APK): " + android.join(", ") + " and their AndroidX / Kotlin dependencies",
    "Licence: Apache-2.0 (text above) - https://developer.android.com/jetpack/androidx",
    line,
    "",
  );
}
if (missing.length) {
  parts.push(line, "No licence file in the package - the licence named here applies, full text at the address", line, "", ...missing, "");
}
// Written only when something changed - the build does not touch the file for nothing.
const text = parts.join("\n");
const same = existsSync(OUT) && readFileSync(OUT, "utf8") === text;
if (!same) writeFileSync(OUT, text, "utf8");
console.log(`${same ? "unchanged" : "updated"} ${OUT}: ${groups.size} texts, ${crates.size} crates, ${npm.size} npm packages, ${missing.length} without a file`);
