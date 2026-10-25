import { Switch as SwitchPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

export function Switch({ className, ...props }: ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      className={cn(
        "relative inline-flex h-[22px] w-[38px] flex-none cursor-pointer items-center rounded-full bg-surface-2 transition-colors duration-(--dur-fast) ease-(--ease-out) disabled:cursor-default disabled:opacity-50 data-[state=checked]:bg-text",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className="block size-4 translate-x-[3px] rounded-full bg-muted transition-transform duration-(--dur-fast) ease-(--ease-out) data-[state=checked]:translate-x-[19px] data-[state=checked]:bg-bg" />
    </SwitchPrimitive.Root>
  );
}
