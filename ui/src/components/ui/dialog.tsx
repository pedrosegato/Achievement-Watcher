import { Dialog as DialogPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

export const Dialog = DialogPrimitive.Root;
export const DialogClose = DialogPrimitive.Close;
export const DialogTitle = DialogPrimitive.Title;
export const DialogDescription = DialogPrimitive.Description;

export function DialogContent({
  className,
  children,
  onPointerDownOutside,
  ...props
}: ComponentProps<typeof DialogPrimitive.Content>) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="dialog-overlay fixed inset-x-0 top-(--titlebar) bottom-0 z-50 bg-black/55" />
      <DialogPrimitive.Content
        className={cn(
          "dialog-content fixed top-[calc(50%+var(--titlebar)/2)] left-1/2 z-50 flex max-h-[min(86vh,820px)] w-[min(92vw,760px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-card border border-line bg-raised text-text shadow-raised outline-none",
          className,
        )}
        onPointerDownOutside={(event) => {
          if (event.target instanceof Element && event.target.closest("[data-titlebar]")) {
            event.preventDefault();
          }
          onPointerDownOutside?.(event);
        }}
        {...props}
      >
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
