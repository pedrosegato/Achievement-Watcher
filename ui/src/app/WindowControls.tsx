import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { useI18n } from "../i18n/useI18n";
import { cn } from "../lib/cn";

const BUTTON =
  "grid h-full w-[46px] cursor-pointer place-items-center text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2 hover:text-text";

export function WindowControls() {
  const { t } = useI18n();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const window = getCurrentWindow();
    let current = true;
    const sync = () => {
      window
        .isMaximized()
        .then((value) => {
          if (current) setMaximized(value);
        })
        .catch(() => undefined);
    };
    sync();
    const pending = window.onResized(sync);
    return () => {
      current = false;
      pending.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, []);

  return (
    <div className="flex self-stretch">
      <button
        type="button"
        aria-label={t("window.minimize")}
        onClick={() => void getCurrentWindow().minimize()}
        className={BUTTON}
      >
        <Icon name="minimize" size={16} />
      </button>
      <button
        type="button"
        aria-label={maximized ? t("window.restore") : t("window.maximize")}
        onClick={() => void getCurrentWindow().toggleMaximize()}
        className={BUTTON}
      >
        <Icon name={maximized ? "restore" : "maximize"} size={14} />
      </button>
      <button
        type="button"
        aria-label={t("window.close")}
        onClick={() => void getCurrentWindow().close()}
        className={cn(BUTTON, "hover:bg-(--close-hover) hover:text-white")}
      >
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}
