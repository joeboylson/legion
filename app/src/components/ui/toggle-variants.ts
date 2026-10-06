// The toggle's looks, shared by Toggle and ToggleGroup. Kept out of
// toggle.tsx, which exports components only.

import { cva } from "class-variance-authority"

export const toggleVariants = cva(
  "inline-flex items-center justify-center gap-3 rounded-sm text-body font-medium whitespace-nowrap transition-[color,box-shadow] outline-none hover:bg-muted hover:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-highlight disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 data-[state=on]:bg-accent data-[state=on]:text-accent-foreground dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-transparent",
        outline:
          "border border-input bg-transparent hover:bg-accent hover:text-accent-foreground",
      },
      size: {
        default: "h-6 min-w-6 px-3",
        sm: "h-6 min-w-6 px-2",
        lg: "h-7 min-w-7 px-3",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)
