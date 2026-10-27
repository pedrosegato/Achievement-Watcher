import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useI18n } from "../i18n/useI18n";
import { cloudErrorKey } from "../lib/cloud";
import { errorMessage } from "../lib/commands";

const CLOUD_QUERIES = [["cloud-overview"], ["cloud-game"]] as const;

export function useCloudAction() {
  const { t } = useI18n();
  const client = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    Promise.all(CLOUD_QUERIES.map((queryKey) => client.invalidateQueries({ queryKey })));

  const run = async (work: () => Promise<void>) => {
    setError(null);
    try {
      await work();
      await refresh();
    } catch (cause) {
      const code = errorMessage(cause);
      const key = cloudErrorKey(code);
      setError(key ? t(key) : code);
    }
  };

  return { error, run, refresh };
}
