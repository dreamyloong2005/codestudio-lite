export type SortableProfileLike = {
  id: string;
};

export type ProfileDragDisabledState = {
  deletingId: string | null;
  applyingId: string | null;
  editingId: string | null;
  sortableSaving?: boolean;
};

export function sortableArrayMove<T>(items: readonly T[], oldIndex: number, newIndex: number): T[] {
  const next = [...items];
  if (
    oldIndex === newIndex ||
    oldIndex < 0 ||
    newIndex < 0 ||
    oldIndex >= next.length ||
    newIndex >= next.length
  ) {
    return next;
  }

  const [item] = next.splice(oldIndex, 1);
  next.splice(newIndex, 0, item);
  return next;
}

export function profileIdsFromItems<T extends SortableProfileLike>(items: readonly T[]) {
  return items.map((item) => item.id);
}

export function profileListContentKey<T extends SortableProfileLike>(
  scope: string,
  items: readonly T[]
) {
  return `${scope}:${JSON.stringify(items)}`;
}

export function sortableOrderChanged(left: readonly string[], right: readonly string[]) {
  return left.length !== right.length || left.some((id, index) => id !== right[index]);
}

export function profileDragDisabled(state: ProfileDragDisabledState) {
  return state.deletingId !== null || state.applyingId !== null || state.editingId !== null;
}

export function nextSortableProfileIds<T extends SortableProfileLike>(
  currentItems: readonly T[],
  nextItems: readonly T[]
) {
  const currentIds = profileIdsFromItems(currentItems);
  const nextIds = profileIdsFromItems(nextItems);
  return sortableOrderChanged(currentIds, nextIds) ? nextIds : null;
}
