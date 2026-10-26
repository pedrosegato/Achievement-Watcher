import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { OVERLAY_HASH, ROOT_ELEMENT_ID } from "./constants";
import { I18nProvider } from "./i18n/I18nProvider";
import { createQueryClient } from "./lib/queries";
import { OverlayPage } from "./pages/overlay/OverlayPage";
import { SettingsProvider } from "./settings/SettingsProvider";
import "./styles/index.css";
import { ThemeProvider } from "./theme/ThemeProvider";

const root = document.getElementById(ROOT_ELEMENT_ID);
const isOverlay = window.location.hash === OVERLAY_HASH;
const queryClient = createQueryClient();

if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <I18nProvider>
          <SettingsProvider>
            <ThemeProvider>{isOverlay ? <OverlayPage /> : <App />}</ThemeProvider>
          </SettingsProvider>
        </I18nProvider>
      </QueryClientProvider>
    </StrictMode>,
  );
}
