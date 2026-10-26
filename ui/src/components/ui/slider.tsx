import { Slider as SliderPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

const THUMB_KEYS = ["start", "end"];

export function Slider({
  className,
  label,
  ...props
}: ComponentProps<typeof SliderPrimitive.Root> & { label?: string | string[] }) {
  const count = (props.value ?? props.defaultValue ?? [0]).length;
  return (
    <SliderPrimitive.Root
      className={cn("relative flex h-5 w-full touch-none select-none items-center", className)}
      {...props}
    >
      <SliderPrimitive.Track className="relative h-1 grow overflow-hidden rounded-full bg-surface-2">
        <SliderPrimitive.Range className="absolute h-full bg-text" />
      </SliderPrimitive.Track>
      {THUMB_KEYS.slice(0, count).map((key, index) => (
        <SliderPrimitive.Thumb
          key={key}
          aria-label={Array.isArray(label) ? label[index] : label}
          className="block size-3.5 cursor-pointer rounded-full border-2 border-bg bg-text shadow-raised"
        />
      ))}
    </SliderPrimitive.Root>
  );
}
