import { HoverCard as HoverCardPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

export const HoverCard = HoverCardPrimitive.Root;
export const HoverCardTrigger = HoverCardPrimitive.Trigger;

export function HoverCardContent({
  className,
  sideOffset = 8,
  collisionPadding = 12,
  ...props
}: ComponentProps<typeof HoverCardPrimitive.Content>) {
  return (
    <HoverCardPrimitive.Portal>
      <HoverCardPrimitive.Content
        sideOffset={sideOffset}
        collisionPadding={collisionPadding}
        className={cn(
          "pop-surface z-50 rounded-card border border-line bg-raised p-3 text-text shadow-raised outline-none",
          className,
        )}
        {...props}
      />
    </HoverCardPrimitive.Portal>
  );
}
