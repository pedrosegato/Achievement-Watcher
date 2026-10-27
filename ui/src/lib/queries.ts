import { QueryClient, queryOptions } from "@tanstack/react-query";
import { api } from "./commands";

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { staleTime: Number.POSITIVE_INFINITY, retry: false, refetchOnWindowFocus: false },
    },
  });
}

export const queries = {
  games: () => queryOptions({ queryKey: ["games"], queryFn: api.listGames }),
  game: (appId: number) =>
    queryOptions({ queryKey: ["game", appId], queryFn: () => api.gameDetail(appId) }),
  overlayGame: () => queryOptions({ queryKey: ["overlay-game"], queryFn: api.overlayGame }),
  diagnostics: () =>
    queryOptions({ queryKey: ["diagnostics"], queryFn: api.diagnostics, staleTime: 0 }),
  presets: () => queryOptions({ queryKey: ["presets"], queryFn: api.listPresets }),
  sounds: () => queryOptions({ queryKey: ["sounds"], queryFn: api.listSounds }),
  locales: () => queryOptions({ queryKey: ["locales"], queryFn: api.supportedLocales }),
  hotkeyError: (revision: number) =>
    queryOptions({ queryKey: ["hotkey-error", revision], queryFn: api.hotkeyError }),
  updateStatus: () =>
    queryOptions({ queryKey: ["update-status"], queryFn: api.updateStatus, staleTime: 0 }),
  cloudOverview: () =>
    queryOptions({ queryKey: ["cloud-overview"], queryFn: api.cloudOverview, staleTime: 0 }),
  cloudGame: (appId: number) =>
    queryOptions({
      queryKey: ["cloud-game", appId],
      queryFn: () => api.cloudGame(appId),
      staleTime: 0,
    }),
  launchChoice: (appId: number) =>
    queryOptions({
      queryKey: ["launch-choice", appId],
      queryFn: () => api.launchChoice(appId),
      staleTime: 0,
    }),
  playState: (appId: number) =>
    queryOptions({
      queryKey: ["play-state", appId],
      queryFn: () => api.playState(appId),
      staleTime: 0,
    }),
};

export function invalidateGames(client: QueryClient): Promise<unknown> {
  return Promise.all(
    [queries.games(), queries.overlayGame()]
      .map((query) => client.invalidateQueries({ queryKey: query.queryKey }))
      .concat(client.invalidateQueries({ queryKey: ["game"] })),
  );
}
