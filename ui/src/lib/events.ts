import type { QueryKey } from "@tanstack/react-query";

export function cloudChangedKeys(id: number | null): QueryKey[] {
  return id === null ? [["cloud-overview"], ["cloud-game"]] : [["cloud-game", id]];
}

export function playChangedKeys(id: number): QueryKey[] {
  return [
    ["play-state", id],
    ["launch-choice", id],
  ];
}
