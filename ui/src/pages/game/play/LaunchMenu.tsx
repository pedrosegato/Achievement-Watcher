import { useQuery } from "@tanstack/react-query";
import { DropdownMenu } from "radix-ui";
import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import { api } from "../../../lib/commands";
import { pickFile } from "../../../lib/dialog";
import { launchLine } from "../../../lib/play";
import { queries } from "../../../lib/queries";

interface LaunchMenuProps {
  appId: number;
  variant: "secondary" | "stop" | "play";
  disabled: boolean;
  run: (work: () => Promise<void>) => Promise<void>;
}

const ITEM =
  "flex cursor-pointer select-none items-center gap-2 rounded-tag px-2 py-1.5 text-[13px] outline-none data-highlighted:bg-hover";

export function LaunchMenu({ appId, variant, disabled, run }: LaunchMenuProps) {
  const { t } = useI18n();
  const { data: choice } = useQuery(queries.launchChoice(appId));
  const line = choice ? launchLine(choice) : null;

  const choose = () =>
    run(async () => {
      const path = await pickFile(t("play.exe.pickTitle"), ["exe"], choice?.exe ?? undefined);
      if (path) await api.setLaunchExe(appId, path);
    });

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild disabled={disabled}>
        <Button
          variant={variant}
          aria-label={t("play.menu.label")}
          className="h-11 w-9 rounded-l-none px-0 disabled:pointer-events-none disabled:bg-surface-2 disabled:text-muted disabled:opacity-100"
        >
          <Icon name="caret-down" size={16} />
        </Button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="start"
          sideOffset={6}
          className="pop-surface z-50 min-w-56 max-w-80 rounded-card border border-line bg-raised p-1 text-text shadow-raised"
        >
          {line && (
            <DropdownMenu.Label
              title={choice?.exe ?? undefined}
              className="truncate px-2 py-1.5 text-[12px] text-dim"
            >
              {t(line.key, { name: line.name })}
            </DropdownMenu.Label>
          )}
          <DropdownMenu.Item className={ITEM} onSelect={() => void choose()}>
            <Icon name="folder-open" size={15} className="text-dim" />
            {t("play.menu.choose")}
          </DropdownMenu.Item>
          {choice?.chosen && (
            <DropdownMenu.Item
              className={ITEM}
              onSelect={() => void run(() => api.setLaunchExe(appId, null))}
            >
              <Icon name="sync" size={15} className="text-dim" />
              {t("play.menu.reset")}
            </DropdownMenu.Item>
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
