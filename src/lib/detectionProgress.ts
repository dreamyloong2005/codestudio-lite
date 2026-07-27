import type { DetectionSnapshot, ToolStatus } from "../types";

function mergeStatuses(current: ToolStatus[], completed: ToolStatus[]) {
  const completedById = new Map(completed.map((status) => [status.id, status]));
  const currentIds = new Set(current.map((status) => status.id));
  return [
    ...current.map((status) => completedById.get(status.id) ?? status),
    ...completed.filter((status) => !currentIds.has(status.id))
  ];
}

export function mergeDetectionProgressSnapshot(
  current: DetectionSnapshot | null,
  partial: DetectionSnapshot
): DetectionSnapshot {
  if (!current) {
    return partial;
  }
  return {
    ...partial,
    tools: mergeStatuses(current.tools, partial.tools),
    system: mergeStatuses(current.system, partial.system),
    problems: current.problems,
    claudeInstallKinds: current.claudeInstallKinds,
    chatgptDesktopInstallKinds: current.chatgptDesktopInstallKinds
  };
}
