export type HiddenAction =
  | { type: "reveal"; apiName: string }
  | { type: "hide"; apiName: string }
  | { type: "reset" };

export function hiddenReducer(
  state: ReadonlySet<string>,
  action: HiddenAction,
): ReadonlySet<string> {
  if (action.type === "reset") return state.size === 0 ? state : new Set();
  const next = new Set(state);
  if (action.type === "reveal") next.add(action.apiName);
  else next.delete(action.apiName);
  return next;
}
