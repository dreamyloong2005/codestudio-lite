import test from "node:test";
import assert from "node:assert/strict";
import {
  profileDragDisabled,
  nextSortableProfileIds,
  profileIdsFromItems,
  sortableArrayMove,
  sortableOrderChanged
} from "../../.tmp-tests/lib/profileSortable.js";

test("sortableArrayMove follows dnd-kit arrayMove semantics", () => {
  const items = ["official", "alpha", "beta"];

  assert.deepEqual(sortableArrayMove(items, 0, 2), ["alpha", "beta", "official"]);
  assert.deepEqual(sortableArrayMove(items, 2, 0), ["beta", "official", "alpha"]);
  assert.deepEqual(sortableArrayMove(items, 1, 1), items);
});

test("profileIdsFromItems returns the persisted order payload", () => {
  assert.deepEqual(
    profileIdsFromItems([{ id: "official" }, { id: "alpha" }, { id: "beta" }]),
    ["official", "alpha", "beta"]
  );
});

test("sortableOrderChanged detects real order changes only", () => {
  assert.equal(sortableOrderChanged(["official", "alpha"], ["official", "alpha"]), false);
  assert.equal(sortableOrderChanged(["official", "alpha"], ["alpha", "official"]), true);
  assert.equal(sortableOrderChanged(["official"], ["official", "alpha"]), true);
});

test("nextSortableProfileIds returns null until a drag creates a new order", () => {
  const current = [{ id: "official" }, { id: "alpha" }, { id: "beta" }];
  assert.equal(nextSortableProfileIds(current, current), null);
  assert.deepEqual(
    nextSortableProfileIds(current, [{ id: "alpha" }, { id: "beta" }, { id: "official" }]),
    ["alpha", "beta", "official"]
  );
});

test("profileDragDisabled does not disable dragging while sort persistence is in flight", () => {
  assert.equal(
    profileDragDisabled({
      deletingId: null,
      applyingId: null,
      editingId: null,
      sortableSaving: true
    }),
    false
  );
  assert.equal(
    profileDragDisabled({
      deletingId: "alpha",
      applyingId: null,
      editingId: null,
      sortableSaving: false
    }),
    true
  );
});

test("nextSortableProfileIds supports consecutive reorder operations", () => {
  const initial = [{ id: "official" }, { id: "alpha" }, { id: "beta" }];
  const firstOrder = sortableArrayMove(initial, 0, 2);
  assert.deepEqual(nextSortableProfileIds(initial, firstOrder), ["alpha", "beta", "official"]);

  const secondOrder = sortableArrayMove(firstOrder, 2, 0);
  assert.deepEqual(nextSortableProfileIds(firstOrder, secondOrder), ["official", "alpha", "beta"]);
});
