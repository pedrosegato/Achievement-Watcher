import { Tabs as TabsPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

export const Tabs = TabsPrimitive.Root;
export const TabsContent = TabsPrimitive.Content;

export function TabsList({ className, ...props }: ComponentProps<typeof TabsPrimitive.List>) {
  return <TabsPrimitive.List className={cn("flex gap-0.5", className)} {...props} />;
}

export function TabsTrigger({ className, ...props }: ComponentProps<typeof TabsPrimitive.Trigger>) {
  return (
    <TabsPrimitive.Trigger
      className={cn(
        "flex cursor-pointer items-center gap-2.5 rounded-card px-2.5 py-2 text-left font-semibold text-[13px] text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:text-text data-[state=active]:bg-surface-2 data-[state=active]:text-text",
        className,
      )}
      {...props}
    />
  );
}
