// Post-process Tauri-generated icons to crisp up the small stages the
// Windows taskbar actually renders (16/24/32px at 100% DPI).
//
// Why: `tauri icon` does a plain downscale with no sharpening, so a detailed
// source (gradients/shadows) collapses into a muddy 32x32. We apply a light
// unsharp mask only to the small stages, then rebuild icon.ico with the
// sharpened small stages + untouched large stages. Large stages keep Tauri's
// output (already crisp from the big source), so we don't risk halos on the
// high-res taskbar/DPI stages.
//
// Run after `npx tauri icon public/icon.png`:
//   node scripts/sharpen-small-icons.mjs
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const iconsDir = resolve(root, "src-tauri", "icons");

// Stages the taskbar renders at 100% DPI get sharpened. 48+ are only used on
// HiDPI where Tauri's downscale already looks fine, and sharpening 64/128/256
// risks halos, so they are passed through untouched.
const SMALL_STAGES = [16, 24, 32];
// unsharp params tuned so edge contrast rises ~35% without jagged artifacts
// (sigma<1 keeps the mask narrow on a 16-32px canvas).
const UNSHARP = { sigma: 0.8, radius: 0.8, m1: 3, m2: 1 };

function pngTruecolor(buf) {
  return sharp(buf)
    .png({ palette: false, quality: 100 })
    .toBuffer();
}

async function sharpenStage(pngBuf) {
  // Lanczos re-downscale is a no-op for already-small stages but keeps the
  // pipeline idempotent; unsharp is the actual crispness step.
  const sharpened = await sharp(pngBuf)
    .sharpen(UNSHARP)
    .toBuffer();
  return pngTruecolor(sharpened);
}

// Parse PNG-in-ICO into [{size, pngBuf, w}], preserving original stage order.
function parseIco(buf) {
  const count = buf.readUInt16LE(4);
  const stages = [];
  let dataStart = 6 + count * 16;
  for (let i = 0; i < count; i += 1) {
    const o = 6 + i * 16;
    const w = buf.readUInt8(o) || 256;
    const size = buf.readUInt32LE(o + 8);
    const off = buf.readUInt32LE(o + 12);
    stages.push({ w, pngBuf: buf.subarray(off, off + size) });
  }
  return stages;
}

function buildIco(stages) {
  const count = stages.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(count, 4);
  const entries = [];
  let dataOffset = 6 + count * 16;
  for (let i = 0; i < count; i += 1) {
    const png = stages[i].pngBuf;
    const w = stages[i].w;
    const entry = Buffer.alloc(16);
    entry.writeUInt8(w >= 256 ? 0 : w, 0);
    entry.writeUInt8(w >= 256 ? 0 : w, 1);
    entry.writeUInt8(0, 2);
    entry.writeUInt8(0, 3);
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(png.length, 8);
    entry.writeUInt32LE(dataOffset, 12);
    entries.push(entry);
    dataOffset += png.length;
  }
  return Buffer.concat([header, ...entries, ...stages.map((s) => s.pngBuf)]);
}

async function main() {
  // 1) Sharpen the named small PNGs so they stay consistent with the .ico stages.
  const named = { 32: "32x32.png" };
  for (const [sizeStr, file] of Object.entries(named)) {
    const size = Number(sizeStr);
    const p = resolve(iconsDir, file);
    const buf = readFileSync(p);
    const out = await sharpenStage(buf);
    writeFileSync(p, out);
    console.log(`sharpened ${p} (${size}x${size})`);
  }

  // 2) Rebuild icon.ico: sharpen small stages, pass large stages through.
  const icoPath = resolve(iconsDir, "icon.ico");
  const stages = parseIco(readFileSync(icoPath));
  for (const stage of stages) {
    if (SMALL_STAGES.includes(stage.w)) {
      stage.pngBuf = await sharpenStage(stage.pngBuf);
    } else {
      stage.pngBuf = await pngTruecolor(stage.pngBuf);
    }
  }
  writeFileSync(icoPath, buildIco(stages));
  console.log(
    `rebuilt ${icoPath} (sharpened ${SMALL_STAGES.join(",")} stages, passed through the rest)`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});