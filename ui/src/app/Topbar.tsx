import { AppLogo } from "../components/AppLogo";
import { Icon } from "../components/Icon";
import { Button } from "../components/ui/button";
import { Tooltip } from "../components/ui/tooltip";
import { APP_NAME } from "../constants";
import { useI18n } from "../i18n/useI18n";
import { cn } from "../lib/cn";
import { WindowControls } from "./WindowControls";

interface TopbarProps {
  settingsOpen: boolean;
  onSettings: () => void;
}

export function Topbar({ settingsOpen, onSettings }: TopbarProps) {
  const { t } = useI18n();

  return (
    <header
      data-tauri-drag-region
      data-titlebar
      className="pointer-events-auto flex h-(--titlebar) shrink-0 items-center border-line border-b bg-bg pl-5"
    >
      <div data-tauri-drag-region className="mr-auto flex items-center gap-2 font-bold text-[13px]">
        <AppLogo className="size-5" />
        {APP_NAME}
      </div>
      <Tooltip label={t("nav.settings")}>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("nav.settings")}
          aria-pressed={settingsOpen}
          onClick={onSettings}
          className={cn("mr-1.5", settingsOpen && "bg-surface-2 text-text")}
        >
          <Icon name="settings" size={18} />
        </Button>
      </Tooltip>
      <WindowControls />
    </header>
  );
}
