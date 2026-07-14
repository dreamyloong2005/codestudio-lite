import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";

const options = parseOptions(process.argv.slice(2));
const version = requiredOption(options, "version");
const baseUrl = requiredOption(options, "base-url").replace(/\/+$/, "");
const parsedBaseUrl = new URL(baseUrl);
if (parsedBaseUrl.protocol !== "https:") {
  throw new Error("--base-url must use HTTPS.");
}

const outputPath = resolve(options.output ?? "dist-updater/latest.json");
const platforms = {};

if (options["windows-installer"]) {
  platforms["windows-x86_64"] = artifactEntry(
    options["windows-installer"],
    `${baseUrl}/releases/${encodeURIComponent(version)}`,
    "Windows"
  );
}

if (options["macos-dmg"]) {
  const macosEntry = artifactEntry(
    options["macos-dmg"],
    `${baseUrl}/releases/${encodeURIComponent(version)}`,
    "macOS"
  );
  platforms["darwin-aarch64"] = macosEntry;
  platforms["darwin-x86_64"] = macosEntry;
}

if (options["linux-appimage"]) {
  const linuxPlatform = requiredOption(options, "linux-platform");
  if (!/^linux-(?:x86_64|aarch64|armv7)$/.test(linuxPlatform)) {
    throw new Error("--linux-platform must be linux-x86_64, linux-aarch64, or linux-armv7.");
  }
  platforms[linuxPlatform] = artifactEntry(
    options["linux-appimage"],
    `${baseUrl}/releases/${encodeURIComponent(version)}`,
    "Linux"
  );
}

if (Object.keys(platforms).length === 0) {
  throw new Error("Provide --windows-installer, --macos-dmg, and/or --linux-appimage.");
}

const manifest = {
  version,
  notes: options.notes ?? `CodeStudio Lite ${version}`,
  pub_date: options["pub-date"] ?? new Date().toISOString(),
  platforms
};

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
console.log(`Updater manifest written to ${outputPath}`);

function artifactEntry(inputPath, objectBaseUrl, operatingSystem) {
  const artifactPath = resolve(inputPath);
  validateArtifactFilename(artifactPath, operatingSystem);
  const signaturePath = `${artifactPath}.sig`;
  if (!existsSync(artifactPath)) {
    throw new Error(`Updater artifact was not found: ${artifactPath}`);
  }
  if (!existsSync(signaturePath)) {
    throw new Error(`Updater signature was not found: ${signaturePath}`);
  }

  const signature = readFileSync(signaturePath, "utf8").trim();
  if (!signature) {
    throw new Error(`Updater signature is empty: ${signaturePath}`);
  }

  return {
    signature,
    url: `${objectBaseUrl}/${encodeURIComponent(basename(artifactPath))}`
  };
}

function validateArtifactFilename(path, operatingSystem) {
  const filename = basename(path);
  if (!/^[A-Za-z0-9][A-Za-z0-9.-]*$/.test(filename)) {
    throw new Error(`Updater artifact filename must use kebab-case without spaces or underscores: ${filename}`);
  }
  const requiredPrefix = `CodeStudio-Lite-${version}-${operatingSystem}-`;
  if (!filename.startsWith(requiredPrefix)) {
    throw new Error(`Updater artifact filename must include ${operatingSystem} between version and architecture: ${filename}`);
  }
}

function parseOptions(args) {
  const parsed = {};
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (!argument.startsWith("--")) {
      throw new Error(`Unexpected argument: ${argument}`);
    }
    const name = argument.slice(2);
    const value = args[index + 1];
    if (!value || value.startsWith("--")) {
      throw new Error(`Missing value for --${name}.`);
    }
    parsed[name] = value;
    index += 1;
  }
  return parsed;
}

function requiredOption(options, name) {
  const value = options[name]?.trim();
  if (!value) {
    throw new Error(`--${name} is required.`);
  }
  return value;
}
