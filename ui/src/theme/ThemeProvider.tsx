import { type ReactNode, useEffect } from "react";
import { useAppSettings } from "../settings/useAppSettings";
import { useResolvedTheme } from "./useTheme";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const theme = useResolvedTheme(useAppSettings()?.theme ?? "system");

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  return children;
}
