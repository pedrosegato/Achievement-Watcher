import { Tooltip as TooltipPrimitive } from "radix-ui";
import type { ReactNode } from "react";
import { useMotionMs } from "../../lib/motion";

interface TooltipProps {
  label: ReactNode;
  children: ReactNode;
  side?: "top" | "bottom" | "left" | "right";
}

export function Tooltip({ label, children, side = "bottom" }: TooltipProps) {
  const delay = useMotionMs("--hover-delay");
  return (
    <TooltipPrimitive.Provider delayDuration={delay}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            side={side}
            sideOffset={6}
            className="pop-surface z-50 rounded-tag border border-line bg-raised px-2 py-1 text-[12px] text-text shadow-raised"
          >
            {label}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}
