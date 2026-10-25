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
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
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
      className="flex gap-0.5 rounded-card border border-line bg-surface p-[3px]"
    >
      {options.map((option) => (
        <ToggleGroup.Item
          key={option.value}
          value={option.value}
          className="flex cursor-pointer items-center gap-1.5 rounded-tag px-2.5 py-[5px] font-semibold text-[12.5px] text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:text-text data-[state=on]:bg-surface-2 data-[state=on]:text-text"
        >
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
