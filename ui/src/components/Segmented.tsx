import { ToggleGroup } from "radix-ui";
import { Icon, type IconName } from "./Icon";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  icon?: IconName;
  count?: number;
}

interface SegmentedProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  label?: string;
  variant?: "boxed" | "tabs";
}

const ROOT = {
  boxed: "flex gap-0.5 rounded-card border border-line bg-surface p-[3px]",
  tabs: "flex gap-5",
};

const ITEM = {
  boxed:
    "flex cursor-pointer items-center gap-1.5 rounded-tag px-2.5 py-[5px] font-semibold text-[12.5px] text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:text-text data-[state=on]:bg-surface-2 data-[state=on]:text-text",
  tabs: "relative flex cursor-pointer items-center gap-1.5 py-1.5 font-semibold text-[14px] text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-full after:bg-text after:opacity-0 after:transition-opacity after:duration-(--dur-fast) hover:text-text data-[state=on]:text-text data-[state=on]:after:opacity-100",
};

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
  variant = "boxed",
}: SegmentedProps<T>) {
  const select = (next: string) => {
    const option = options.find((item) => item.value === next);
    if (option) onChange(option.value);
  };

  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={select}
      aria-label={label}
      className={ROOT[variant]}
    >
      {options.map((option) => (
        <ToggleGroup.Item key={option.value} value={option.value} className={ITEM[variant]}>
          {option.icon && <Icon name={option.icon} size={15} />}
          {option.label}
          {option.count !== undefined && (
            <span className="font-medium text-dim">{option.count}</span>
          )}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}
