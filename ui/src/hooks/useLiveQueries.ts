import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { EVENTS } from "../constants";
import { cloudChangedKeys, playChangedKeys } from "../lib/events";
import { invalidateGames } from "../lib/queries";
import { useTauriEvent } from "./useTauriEvent";

function refresh(client: QueryClient, keys: readonly (readonly unknown[])[]) {
  for (const queryKey of keys) {
    void client.invalidateQueries({ queryKey }, { cancelRefetch: false });
  }
}

export function useLiveQueries() {
  const client = useQueryClient();
  useTauriEvent(EVENTS.gamesChanged, () => {
    void invalidateGames(client);
    refresh(client, [["play-state"]]);
  });
  useTauriEvent<number | null>(EVENTS.cloudChanged, (id) => refresh(client, cloudChangedKeys(id)));
  useTauriEvent<number>(EVENTS.playChanged, (id) => refresh(client, playChangedKeys(id)));
}
