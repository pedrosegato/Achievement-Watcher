import { describe, expect, it } from "vitest";
import type { GameDetail } from "../types/domain";
import { createQueryClient, invalidateGames, queries } from "./queries";

const DETAIL = {} as GameDetail;

describe("query cache", () => {
  it("keeps an opened game fresh until the games change", () => {
    const client = createQueryClient();
    client.setQueryData(queries.game(7).queryKey, DETAIL);
    expect(
      client
        .getQueryCache()
        .find({ queryKey: queries.game(7).queryKey })
        ?.isStale(),
    ).toBe(false);
  });

  it("marks only game data stale when games change", async () => {
    const client = createQueryClient();
    client.setQueryData(queries.games().queryKey, []);
    client.setQueryData(queries.game(7).queryKey, DETAIL);
    client.setQueryData(queries.overlayGame().queryKey, null);
    client.setQueryData(queries.presets().queryKey, []);
    await invalidateGames(client);
    const invalidated = (key: readonly unknown[]) => client.getQueryState(key)?.isInvalidated;
    expect(invalidated(queries.games().queryKey)).toBe(true);
    expect(invalidated(queries.game(7).queryKey)).toBe(true);
    expect(invalidated(queries.overlayGame().queryKey)).toBe(true);
    expect(invalidated(queries.presets().queryKey)).toBe(false);
  });
});
