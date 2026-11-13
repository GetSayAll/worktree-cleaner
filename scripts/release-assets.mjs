// The two ends of a release: gathering what the build produced, and writing the
// manifest the updater reads.
//
// The updater does not look at a release page. It fetches one JSON file —
// `latest.json` — and picks the entry that matches the machine it is running on.
// Writing that file by hand is how a release ships with a broken updater, so it
// is written here instead, from the artifacts themselves: the signature in the
// manifest is the content of the `.sig` file that was produced next to the
// artifact, never a copy somebody pasted.
//
// Usage:
//   node scripts/release-assets.mjs collect --platform darwin --arch universal \
//        --version 0.1.0 --bundle target/universal-apple-darwin/release/bundle \
//        --out release
//   node scripts/release-assets.mjs manifest --dir release --version 0.1.0 \
//        --tag v0.1.0 --repo GetSayAll/worktree-cleaner --out release/latest.json

import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

/** The name every artifact carries, so two platforms never collide in a release. */
function artifactName(version, platform, arch, kind) {
  const slug = platform === "darwin" ? "macos" : "windows";
  return `Worktree-Cleaner_${version}_${slug}_${arch}${kind}`;
}

/**
 * Every file under `dir`, recursively.
 *
 * The bundle directory holds the installer, the updater artifact and the
 * signatures in separate subdirectories, and their names differ between
 * platforms and Tauri versions, so the files are classified by what they are
 * rather than by where they are.
 */
export function listFiles(dir) {
  if (!existsSync(dir)) return [];
  const found = [];
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const full = join(current, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile()) found.push(full);
    }
  };
  walk(dir);
  return found;
}

/**
 * What a file is, from its name.
 *
 * `updater` is the archive the updater downloads; `installer` is what a person
 * downloads. The macOS updater archive is a universal binary, so it is published
 * under both Apple-silicon and Intel keys — the running process asks for the key
 * of its own architecture, and one universal file answers both.
 */
export function classify(fileName) {
  if (fileName.endsWith(".app.tar.gz")) return { suffix: ".app.tar.gz", updater: true };
  if (fileName.endsWith(".dmg")) return { suffix: ".dmg", updater: false };
  if (fileName.endsWith("-setup.exe")) return { suffix: "-setup.exe", updater: true };
  return null;
}

/** The platform and architecture a collected artifact name carries. */
export function parseArtifactName(name) {
  const match = name.match(/_(macos|windows)_([a-z0-9_]+)(?:\.app\.tar\.gz|\.dmg|-setup\.exe)$/);
  if (!match) return null;
  return {
    platform: match[1] === "macos" ? "darwin" : "windows",
    arch: match[2]
  };
}

/**
 * The platform keys an artifact answers for.
 *
 * The updater asks for `darwin-aarch64` on an Apple-silicon Mac and
 * `darwin-x86_64` on an Intel one; a universal macOS build answers both.
 */
export function platformKeys(platform, arch) {
  if (platform === "darwin") {
    return arch === "universal" ? ["darwin-aarch64", "darwin-x86_64"] : [`darwin-${arch}`];
  }
  return [`windows-${arch}`];
}

/**
 * Copies the build output into `out`, with names that cannot collide.
 *
 * The signature travels with its artifact: the manifest is only trustworthy
 * because the `.sig` file is the one the build produced, not one typed out.
 */
export function collectArtifacts({ bundleDir, outDir, version, platform, arch }) {
  const files = listFiles(bundleDir);
  const copied = [];
  for (const file of files) {
    const name = basename(file);
    if (name.endsWith(".sig")) continue; // copied with its artifact below
    const kind = classify(name);
    if (!kind) continue;
    const target = join(outDir, artifactName(version, platform, arch, kind.suffix));
    mkdirSync(outDir, { recursive: true });
    cpSync(file, target);
    copied.push(target);
    const signature = `${file}.sig`;
    if (existsSync(signature)) {
      const signatureTarget = `${target}.sig`;
      cpSync(signature, signatureTarget);
      copied.push(signatureTarget);
    }
  }
  return copied;
}

/**
 * Writes `latest.json` from the artifacts in `dir`.
 *
 * Every entry carries the signature that was produced next to the artifact and
 * the URL the updater will download it from. Nothing is invented here: an
 * artifact without a signature is skipped rather than published unsigned.
 */
export function buildManifest({ dir, version, tag, repo, notes = "", pubDate }) {
  const files = listFiles(dir);
  const platforms = {};
  for (const file of files) {
    const name = basename(file);
    if (name.endsWith(".sig")) continue;
    const parsed = parseArtifactName(name);
    if (!parsed) continue;
    const signatureFile = `${file}.sig`;
    if (!existsSync(signatureFile)) continue;
    const signature = readFileSync(signatureFile, "utf8").trim();
    const url = `https://github.com/${repo}/releases/download/${tag}/${name}`;
    for (const key of platformKeys(parsed.platform, parsed.arch)) {
      platforms[key] = { signature, url };
    }
  }
  if (Object.keys(platforms).length === 0) {
    throw new Error(`no signed artifacts found in ${dir}`);
  }
  return {
    version,
    notes,
    pub_date: pubDate ?? new Date().toISOString(),
    platforms
  };
}

/** The checksum file that sits next to the assets in a release. */
export function writeChecksums(dir) {
  const files = listFiles(dir)
    .filter((file) => !basename(file).endsWith(".sig") && basename(file) !== "SHA256SUMS")
    .sort();
  const lines = files.map((file) => {
    const digest = createHash("sha256").update(readFileSync(file)).digest("hex");
    return `${digest}  ${basename(file)}`;
  });
  writeFileSync(join(dir, "SHA256SUMS"), `${lines.join("\n")}\n`);
  return lines.length;
}

/** Reads the command line into an options object. */
function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith("--")) continue;
    const key = token.slice(2);
    const next = argv[index + 1];
    if (next === undefined || next.startsWith("--")) {
      options[key] = true;
    } else {
      options[key] = next;
      index += 1;
    }
  }
  return options;
}

function main(argv) {
  const options = parseArgs(argv);
  const command = argv[0];

  if (command === "collect") {
    const outDir = resolve(String(options.out ?? "release"));
    const copied = collectArtifacts({
      bundleDir: resolve(String(options.bundle ?? "target/release/bundle")),
      outDir,
      version: String(options.version ?? "0.0.0"),
      platform: String(options.platform ?? "darwin"),
      arch: String(options.arch ?? "universal")
    });
    if (copied.length === 0) {
      throw new Error(`nothing to collect in ${options.bundle}`);
    }
    console.log(`collected ${copied.length} files into ${outDir}`);
    return;
  }

  if (command === "manifest") {
    const dir = resolve(String(options.dir ?? "release"));
    const manifest = buildManifest({
      dir,
      version: String(options.version ?? "0.0.0"),
      tag: String(options.tag ?? `v${options.version}`),
      repo: String(options.repo ?? "GetSayAll/worktree-cleaner"),
      notes: typeof options.notes === "string" ? options.notes : ""
    });
    const out = resolve(String(options.out ?? join(dir, "latest.json")));
    mkdirSync(dirnameOf(out), { recursive: true });
    writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
    writeChecksums(dir);
    console.log(`wrote ${out} with ${Object.keys(manifest.platforms).length} platforms`);
    return;
  }

  throw new Error(`unknown command: ${command ?? "(none)"}`);
}

function dirnameOf(path) {
  const index = path.lastIndexOf("/");
  return index <= 0 ? "." : path.slice(0, index);
}

const invokedDirectly =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
