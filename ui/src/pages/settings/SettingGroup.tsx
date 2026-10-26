import { type ReactNode, useId } from "react";
import { Icon, type IconName } from "../../components/Icon";
import { Switch } from "../../components/ui/switch";

interface SettingGroupProps {
  icon: IconName;
  title: string;
  description: string;
  note?: string | null;
  action?: ReactNode;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  footer?: ReactNode;
  children: ReactNode;
}

export function SettingGroup({
  icon,
  title,
  description,
  note,
  action,
  checked,
  onCheckedChange,
  footer,
  children,
}: SettingGroupProps) {
  const bodyId = useId();
  return (
    <section
      className="setting-group mt-4 rounded-card border border-line bg-surface"
      data-state={checked ? "on" : "off"}
    >
      <div className="flex items-center gap-3.5 px-4 py-3.5">
        <span className="grid size-9 flex-none place-items-center rounded-card bg-surface-2">
          <Icon name={icon} size={18} />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 className="font-semibold text-[15px]">{title}</h3>
          <p className="text-[12.5px] text-dim">{description}</p>
          {note && <p className="text-[12.5px] text-muted">{note}</p>}
        </div>
        {action}
        <Switch
          aria-label={title}
          aria-controls={bodyId}
          checked={checked}
          onCheckedChange={onCheckedChange}
        />
      </div>
      <div id={bodyId} className="setting-group-body" inert={!checked}>
        <div className="min-h-0 overflow-hidden">
          <div className="border-line border-t px-4 [&>:last-child]:border-b-0">{children}</div>
          {footer && (
            <p className="border-line border-t px-4 py-3 text-[12.5px] text-dim">{footer}</p>
          )}
        </div>
      </div>
    </section>
  );
}
