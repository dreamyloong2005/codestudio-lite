import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const rootDir = resolve(import.meta.dirname, "..");
const publicConfig = JSON.parse(
  readFileSync(resolve(rootDir, "updater.config.json"), "utf8").replace(/^\uFEFF/, "")
);
const outputFlagIndex = process.argv.indexOf("--output");
const outputPath = resolve(
  rootDir,
  outputFlagIndex >= 0 && process.argv[outputFlagIndex + 1]
    ? process.argv[outputFlagIndex + 1]
    : "src-tauri/tauri.updater.generated.conf.json"
);

const baseUrl = configuredValue("CODESTUDIO_UPDATE_BASE_URL", publicConfig.baseUrl).replace(/\/+$/, "");
const publicKey = configuredValue("TAURI_UPDATER_PUBKEY", publicConfig.pubkey);
const parsedBaseUrl = new URL(baseUrl);
if (parsedBaseUrl.protocol !== "https:") {
  throw new Error("CODESTUDIO_UPDATE_BASE_URL must use HTTPS.");
}

const config = {
  bundle: {
    createUpdaterArtifacts: true
  },
  plugins: {
    updater: {
      endpoints: [`${baseUrl}/stable/latest.json`],
      pubkey: publicKey
    }
  }
};

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
console.log(`Updater config written to ${outputPath}`);

function configuredValue(environmentName, fallback) {
  const value = process.env[environmentName]?.trim() || String(fallback ?? "").trim();
  if (!value) {
    throw new Error(`${environmentName} is required. Set it in the environment or updater.config.json.`);
  }
  return value;
}
