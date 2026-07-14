import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { basename, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const options = parseOptions(process.argv.slice(2));
const accountId = optionOrEnvironment(options, "account-id", "CLOUDFLARE_ACCOUNT_ID");
const bucket = optionOrEnvironment(options, "bucket", "R2_BUCKET");
const version = requiredOption(options, "version");
const manifestPath = resolve(requiredOption(options, "manifest"));
const dryRun = options["dry-run"] === true;
const endpoint = `https://${accountId}.r2.cloudflarestorage.com`;

const accessKeyId = dryRun ? process.env.R2_ACCESS_KEY_ID?.trim() ?? "dry-run" : requiredEnvironmentVariable("R2_ACCESS_KEY_ID");
const secretAccessKey = dryRun ? process.env.R2_SECRET_ACCESS_KEY?.trim() ?? "dry-run" : requiredEnvironmentVariable("R2_SECRET_ACCESS_KEY");
const awsEnvironment = {
  ...process.env,
  AWS_ACCESS_KEY_ID: accessKeyId,
  AWS_SECRET_ACCESS_KEY: secretAccessKey,
  AWS_DEFAULT_REGION: "auto"
};

const immutableUploads = [];
addArtifactUploads(immutableUploads, options["windows-installer"], `releases/${version}`, "Windows");
addArtifactUploads(immutableUploads, options["macos-dmg"], `releases/${version}`, "macOS");
if (options["linux-appimage"]) {
  const linuxPlatform = requiredOption(options, "linux-platform");
  if (!/^linux-(?:x86_64|aarch64|armv7)$/.test(linuxPlatform)) {
    throw new Error("--linux-platform must be linux-x86_64, linux-aarch64, or linux-armv7.");
  }
  addArtifactUploads(immutableUploads, options["linux-appimage"], `releases/${version}`, "Linux");
}

for (const upload of immutableUploads) {
  uploadImmutable(upload);
}

uploadMutable({
  path: manifestPath,
  key: "stable/latest.json",
  contentType: "application/json",
  cacheControl: "no-cache, max-age=0, must-revalidate"
});

console.log(dryRun ? "R2 updater publish dry run completed." : "R2 updater release published.");

function addArtifactUploads(uploads, inputPath, keyPrefix, operatingSystem) {
  if (!inputPath) {
    return;
  }
  const artifactPath = resolve(inputPath);
  validateArtifactFilename(artifactPath, operatingSystem);
  const signaturePath = `${artifactPath}.sig`;
  requireFile(artifactPath);
  requireFile(signaturePath);
  uploads.push({
    path: artifactPath,
    key: `${keyPrefix}/${basename(artifactPath)}`,
    contentType: "application/octet-stream",
    cacheControl: "public, max-age=31536000, immutable"
  });
  uploads.push({
    path: signaturePath,
    key: `${keyPrefix}/${basename(signaturePath)}`,
    contentType: "text/plain; charset=utf-8",
    cacheControl: "public, max-age=31536000, immutable"
  });
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

function uploadImmutable(upload) {
  const localSize = statSync(upload.path).size;
  const localSha256 = sha256File(upload.path);
  const remote = headObject(upload.key);
  if (remote !== null) {
    if (remote.size !== localSize || remote.sha256 !== localSha256) {
      throw new Error(`Refusing to overwrite immutable R2 object with different content: ${upload.key}`);
    }
    console.log(`Reusing existing immutable object: ${upload.key}`);
    return;
  }
  uploadObject({ ...upload, sha256: localSha256 });
  const verified = dryRun ? { size: localSize, sha256: localSha256 } : headObject(upload.key);
  if (verified?.size !== localSize || verified.sha256 !== localSha256) {
    throw new Error(`R2 upload verification failed for ${upload.key}.`);
  }
}

function uploadMutable(upload) {
  requireFile(upload.path);
  uploadObject(upload);
}

function uploadObject(upload) {
  const args = [
    "s3", "cp", upload.path, `s3://${bucket}/${upload.key}`,
    "--endpoint-url", endpoint,
    "--region", "auto",
    "--content-type", upload.contentType,
    "--cache-control", upload.cacheControl,
    "--only-show-errors"
  ];
  if (upload.sha256) {
    args.push("--metadata", `sha256=${upload.sha256}`);
  }
  if (dryRun) {
    console.log(`[dry-run] aws ${args.map(quoteArgument).join(" ")}`);
    return;
  }
  runAws(args);
  console.log(`Uploaded: ${upload.key}`);
}

function headObject(key) {
  if (dryRun) {
    return null;
  }
  const result = spawnSync(
    "aws",
    [
      "s3api", "head-object",
      "--bucket", bucket,
      "--key", key,
      "--endpoint-url", endpoint,
      "--region", "auto",
      "--output", "json"
    ],
    { encoding: "utf8", env: awsEnvironment }
  );
  if (result.status === 0) {
    const metadata = JSON.parse(result.stdout);
    return {
      size: metadata.ContentLength,
      sha256: metadata.Metadata?.sha256 ?? null
    };
  }
  if (/404|Not Found|NoSuchKey/i.test(result.stderr)) {
    return null;
  }
  throw new Error(result.stderr.trim() || `AWS CLI failed while checking ${key}.`);
}

function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function runAws(args) {
  const result = spawnSync("aws", args, { encoding: "utf8", env: awsEnvironment, stdio: "inherit" });
  if (result.error?.code === "ENOENT") {
    throw new Error("AWS CLI was not found. Install AWS CLI v2 before publishing to R2.");
  }
  if (result.status !== 0) {
    throw new Error(`AWS CLI exited with code ${result.status}.`);
  }
}

function requireFile(path) {
  if (!existsSync(path)) {
    throw new Error(`Required release file was not found: ${path}`);
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
    if (name === "dry-run") {
      parsed[name] = true;
      continue;
    }
    const value = args[index + 1];
    if (!value || value.startsWith("--")) {
      throw new Error(`Missing value for --${name}.`);
    }
    parsed[name] = value;
    index += 1;
  }
  return parsed;
}

function optionOrEnvironment(options, optionName, environmentName) {
  return options[optionName]?.trim() || requiredEnvironmentVariable(environmentName);
}

function requiredOption(options, name) {
  const value = options[name]?.trim();
  if (!value) {
    throw new Error(`--${name} is required.`);
  }
  return value;
}

function requiredEnvironmentVariable(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`${name} is required.`);
  }
  return value;
}

function quoteArgument(value) {
  return /\s/.test(value) ? JSON.stringify(value) : value;
}
