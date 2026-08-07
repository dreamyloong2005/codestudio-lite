import { join } from "node:path";

export const MACOS_TARGETS = Object.freeze({
  "aarch64-apple-darwin": Object.freeze({
    architecture: "arm64",
    tauriArchitecture: "aarch64",
    executableArchitecture: "arm64"
  }),
  "x86_64-apple-darwin": Object.freeze({
    architecture: "x64",
    tauriArchitecture: "x64",
    executableArchitecture: "x86_64"
  })
});

export function targetMetadata(target) {
  const metadata = MACOS_TARGETS[target];
  if (!metadata) {
    throw new Error(`Unsupported macOS target: ${target}`);
  }
  return metadata;
}

export function macosArtifactPaths(bundleRoot, version, target) {
  const metadata = targetMetadata(target);
  const canonicalBase = `CodeStudio-Lite-${version}-macOS-${metadata.architecture}`;
  const rawDmg = `CodeStudio Lite_${version}_${metadata.tauriArchitecture}.dmg`;
  const rawArchive = "CodeStudio Lite.app.tar.gz";

  return {
    raw: {
      dmg: join(bundleRoot, "dmg", rawDmg),
      archive: join(bundleRoot, "macos", rawArchive),
      archiveSignature: join(bundleRoot, "macos", `${rawArchive}.sig`)
    },
    canonical: {
      dmg: join(bundleRoot, "dmg", `${canonicalBase}.dmg`),
      dmgSignature: join(bundleRoot, "dmg", `${canonicalBase}.dmg.sig`),
      archive: join(bundleRoot, "macos", `${canonicalBase}.app.tar.gz`),
      archiveSignature: join(bundleRoot, "macos", `${canonicalBase}.app.tar.gz.sig`),
      app: join(bundleRoot, "macos", "CodeStudio Lite.app")
    },
    canonicalBase
  };
}
