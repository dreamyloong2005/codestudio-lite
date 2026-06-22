import { spawnSync } from "node:child_process";
import fs from "node:fs";

const PORT_START = 9229;
const PORT_END = 9300;
const WAIT_MS = Number.parseInt(process.env.CLAUDE_INSPECTOR_WAIT_MS ?? "20000", 10);
const LOG_PATH = process.env.CODESTUDIO_CLAUDE_INSPECTOR_LOG ?? "";

function write(message = "") {
  const line = String(message);
  console.log(line);
  if (LOG_PATH) {
    fs.appendFileSync(LOG_PATH, `${line}\n`, "utf8");
  }
}

function runPowerShell(script) {
  return spawnSync(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script],
    { encoding: "utf8" }
  );
}

function readPowerShellText(script) {
  const result = runPowerShell(script);
  return {
    ok: result.status === 0,
    stdout: (result.stdout ?? "").trim(),
    stderr: (result.stderr ?? "").trim()
  };
}

function findClaudePids() {
  const script = String.raw`
$visible = @(Get-Process -Name 'claude' -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -and $_.Path.IndexOf('Claude', [System.StringComparison]::OrdinalIgnoreCase) -ge 0 } |
  Sort-Object -Property StartTime)
if ($visible.Count -eq 0) {
  $visible = @(Get-Process -Name 'claude' -ErrorAction SilentlyContinue | Sort-Object -Property Id)
}
$visible | ForEach-Object { [string]$_.Id }
`;
  const result = runPowerShell(script);
  if (result.status !== 0) {
    throw new Error(`PowerShell process lookup failed: ${result.stderr || result.stdout}`);
  }
  return result.stdout
    .split(/\r?\n/)
    .map((line) => Number.parseInt(line.trim(), 10))
    .filter((value) => Number.isInteger(value) && value > 0);
}

function attachPid(pid) {
  try {
    process._debugProcess(pid);
    return { ok: true, pid };
  } catch (error) {
    const text = error instanceof Error ? `${error.message}\n${error.stack ?? ""}` : String(error);
    const errno = typeof error === "object" && error !== null && "errno" in error ? error.errno : undefined;
    const syscall = typeof error === "object" && error !== null && "syscall" in error ? error.syscall : undefined;
    const accessDenied = errno === 5
      || syscall === "OpenProcess"
      || text.includes("OpenProcess")
      || text.includes("Access is denied")
      || text.includes("拒绝")
      || text.includes("denied")
      || text.includes("¾Ü¾ø");
    return {
      ok: false,
      pid,
      error: accessDenied
        ? `ACCESS_DENIED ${text}`
        : text
    };
  }
}

async function fetchJson(port) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 900);
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json`, { signal: controller.signal });
    if (!response.ok) {
      return [];
    }
    const targets = await response.json();
    return Array.isArray(targets) ? targets.map((target) => ({ ...target, __port: port })) : [];
  } catch {
    return [];
  } finally {
    clearTimeout(timeout);
  }
}

async function readTargetsOnce() {
  const all = [];
  for (let port = PORT_START; port <= PORT_END; port += 1) {
    const targets = await fetchJson(port);
    all.push(...targets);
  }
  return all;
}

async function waitForTargets() {
  const started = Date.now();
  let lastTargets = [];
  while (Date.now() - started < WAIT_MS) {
    const targets = await readTargetsOnce();
    if (targets.length > 0) {
      lastTargets = targets;
      return targets;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  return lastTargets;
}

function evaluateIdentity(wsUrl) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const timer = setTimeout(() => {
      try {
        ws.close();
      } catch {}
      reject(new Error("Timed out waiting for Runtime.evaluate response."));
    }, 5000);

    ws.addEventListener("open", () => {
      ws.send(JSON.stringify({
        id: 1,
        method: "Runtime.evaluate",
        params: {
          expression: String.raw`
(() => {
  try {
    const requireFromMain = process.getBuiltinModule("module").createRequire(process.execPath);
    const electron = requireFromMain("electron");
    const app = electron.app;
    return JSON.stringify({
      execPath: process.execPath || "",
      argv: process.argv || [],
      appName: app?.getName?.() || "",
      appPath: app?.getAppPath?.() || "",
      userData: app?.getPath?.("userData") || ""
    });
  } catch (error) {
    return JSON.stringify({
      execPath: process.execPath || "",
      argv: process.argv || [],
      error: String(error && error.message || error)
    });
  }
})()
`,
          awaitPromise: false,
          returnByValue: true
        }
      }));
    });

    ws.addEventListener("message", (event) => {
      const value = JSON.parse(String(event.data));
      if (value.id !== 1) {
        return;
      }
      clearTimeout(timer);
      ws.close();
      if (value.error) {
        reject(new Error(JSON.stringify(value.error)));
        return;
      }
      const raw = value.result?.result?.value;
      if (typeof raw !== "string") {
        reject(new Error(`No identity JSON returned: ${JSON.stringify(value)}`));
        return;
      }
      resolve(JSON.parse(raw));
    });

    ws.addEventListener("error", () => {
      clearTimeout(timer);
      reject(new Error(`Failed to connect ${wsUrl}`));
    });
  });
}

function isClaudeIdentity(identity) {
  const text = JSON.stringify(identity).toLowerCase();
  return text.includes("claude") && !text.includes("roxybrowser");
}

async function main() {
  write("=== CodeStudio Claude Inspector Debug ===");
  write(`node=${process.execPath}`);
  const whoami = readPowerShellText("whoami");
  write(`whoami=${whoami.ok ? whoami.stdout : `failed ${whoami.stderr || whoami.stdout}`}`);
  const elevated = readPowerShellText("([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)");
  write(`isAdministrator=${elevated.ok ? elevated.stdout : `failed ${elevated.stderr || elevated.stdout}`}`);
  const pids = findClaudePids();
  write(`claudePids=${pids.join(",") || "(none)"}`);
  if (pids.length === 0) {
    write("ERROR no Claude processes found.");
    process.exitCode = 2;
    return;
  }

  const attachResults = pids.map(attachPid);
  for (const result of attachResults) {
    write(result.ok ? `attach ${result.pid}: ok` : `attach ${result.pid}: ${result.error}`);
  }

  const targets = await waitForTargets();
  write(`inspectorTargets=${targets.length}`);
  for (const target of targets) {
    write(`target port=${target.__port} title=${target.title ?? ""} ws=${target.webSocketDebuggerUrl ?? ""}`);
  }

  for (const target of targets) {
    const wsUrl = target.webSocketDebuggerUrl;
    if (!wsUrl) {
      continue;
    }
    try {
      const identity = await evaluateIdentity(wsUrl);
      write(`identity port=${target.__port} ${JSON.stringify(identity)}`);
      if (isClaudeIdentity(identity)) {
        write(`SUCCESS Claude Node inspector is open on port ${target.__port}`);
        process.exitCode = 0;
        return;
      }
    } catch (error) {
      write(`identity failed port=${target.__port}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  const denied = attachResults.some((result) => !result.ok && String(result.error).includes("ACCESS_DENIED"));
  write(denied ? "ERROR access denied while attaching; run elevated and try again." : "ERROR no Claude Node inspector target found.");
  process.exitCode = denied ? 5 : 4;
}

main().catch((error) => {
  write(`FATAL ${error instanceof Error ? error.stack ?? error.message : String(error)}`);
  process.exitCode = 1;
});
