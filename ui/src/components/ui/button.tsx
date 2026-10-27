import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

const buttonVariants = cva(
  "inline-flex cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-card font-semibold text-[13px] transition-colors duration-(--dur-fast) ease-(--ease-out) disabled:cursor-default disabled:opacity-50",
  {
    variants: {
      variant: {
        primary: "bg-text text-bg hover:opacity-90",
        secondary: "border border-line bg-surface text-text hover:bg-surface-2",
        ghost: "text-muted hover:bg-surface-2 hover:text-text",
        link: "px-0 text-muted underline-offset-4 hover:text-text hover:underline",
        play: "bg-play text-on-accent hover:bg-play-hover",
        stop: "bg-stop text-on-accent hover:bg-stop-hover",
      },
      size: {
        sm: "h-8 px-3",
        md: "h-9 px-[13px]",
        icon: "size-8",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  };

export function Button({ className, variant, size, asChild = false, type, ...props }: ButtonProps) {
  if (asChild) {
    return <Slot.Root className={cn(buttonVariants({ variant, size }), className)} {...props} />;
  }
  return (
    <button
      type={type ?? "button"}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}
