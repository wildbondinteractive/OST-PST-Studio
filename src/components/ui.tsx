// Small, consistent primitives in the style of shadcn/ui on Radix, kept in one file
// because each is a few lines and the point is that they all look like one system.

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as SelectPrimitive from "@radix-ui/react-select";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import { cva, type VariantProps } from "class-variance-authority";
import { Check, ChevronDown, X } from "lucide-react";
import { cn } from "@/lib/utils";

/* Button */
const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-[13px] font-medium transition-colors disabled:pointer-events-none disabled:opacity-50 select-none",
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-fg hover:bg-accent-hover shadow-sm",
        secondary: "bg-surface border border-border-strong text-fg hover:bg-surface-3",
        ghost: "text-fg hover:bg-surface-3",
        subtle: "bg-surface-3 text-fg hover:bg-border",
        danger: "bg-danger text-white hover:opacity-90",
        link: "text-accent underline-offset-2 hover:underline px-0 h-auto",
      },
      size: {
        sm: "h-7 px-2.5 text-xs",
        md: "h-8 px-3",
        lg: "h-9 px-4 text-sm",
        icon: "h-8 w-8",
        iconSm: "h-7 w-7",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}
export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(({ className, variant, size, type = "button", ...props }, ref) => (
  <button ref={ref} type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />
));
Button.displayName = "Button";

/* Input */
export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(({ className, ...props }, ref) => (
  <input
    ref={ref}
    className={cn(
      "h-8 w-full rounded-md border border-border-strong bg-surface px-2.5 text-[13px] text-fg placeholder:text-fg-subtle focus:border-accent focus:outline-none disabled:opacity-50 selectable",
      className,
    )}
    {...props}
  />
));
Input.displayName = "Input";

export const Textarea = React.forwardRef<HTMLTextAreaElement, React.TextareaHTMLAttributes<HTMLTextAreaElement>>(({ className, ...props }, ref) => (
  <textarea
    ref={ref}
    className={cn("w-full rounded-md border border-border-strong bg-surface px-2.5 py-1.5 text-[13px] text-fg focus:border-accent focus:outline-none selectable", className)}
    {...props}
  />
));
Textarea.displayName = "Textarea";

/* Label and field */
export function Label({ className, ...props }: React.LabelHTMLAttributes<HTMLLabelElement>) {
  return <label className={cn("text-xs font-medium text-fg-muted", className)} {...props} />;
}
export function Field({ label, hint, children, className }: { label: string; hint?: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <Label>{label}</Label>
      {children}
      {hint && <p className="text-xs text-fg-subtle">{hint}</p>}
    </div>
  );
}

/* Badge */
const badgeVariants = cva("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium leading-4 whitespace-nowrap", {
  variants: {
    tone: {
      neutral: "bg-surface-3 text-fg-muted",
      accent: "bg-accent-soft text-accent",
      success: "bg-success-soft text-success",
      warning: "bg-warning-soft text-warning",
      danger: "bg-danger-soft text-danger",
    },
  },
  defaultVariants: { tone: "neutral" },
});
export function Badge({ className, tone, ...props }: React.HTMLAttributes<HTMLSpanElement> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ tone }), className)} {...props} />;
}

/* Card */
export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-lg border border-border bg-surface", className)} {...props} />;
}
export function CardHeader({ title, description, action }: { title: string; description?: string; action?: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        {description && <p className="mt-0.5 text-xs text-fg-muted">{description}</p>}
      </div>
      {action}
    </div>
  );
}

/* Dialog */
export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;
export function DialogContent({ className, children, title, description, wide, ...props }: DialogPrimitive.DialogContentProps & { title: string; description?: string; wide?: boolean }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-black/40 backdrop-blur-[1px]" />
      <DialogPrimitive.Content
        className={cn(
          "fixed left-1/2 top-1/2 z-50 flex max-h-[85vh] w-full -translate-x-1/2 -translate-y-1/2 flex-col rounded-xl border border-border bg-surface shadow-2xl focus:outline-none",
          wide ? "max-w-3xl" : "max-w-lg",
          className,
        )}
        {...props}
      >
        <div className="flex items-start justify-between gap-4 border-b border-border px-5 py-4">
          <div>
            <DialogPrimitive.Title className="text-base font-semibold">{title}</DialogPrimitive.Title>
            {description ? (
              <DialogPrimitive.Description className="mt-1 text-xs text-fg-muted">{description}</DialogPrimitive.Description>
            ) : (
              <DialogPrimitive.Description className="sr-only">{title}</DialogPrimitive.Description>
            )}
          </div>
          <DialogPrimitive.Close asChild>
            <Button variant="ghost" size="iconSm" aria-label="Close">
              <X size={16} />
            </Button>
          </DialogPrimitive.Close>
        </div>
        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">{children}</div>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
export function DialogFooter({ children }: { children: React.ReactNode }) {
  return <div className="-mx-5 -mb-4 mt-4 flex justify-end gap-2 border-t border-border bg-surface-2 px-5 py-3 rounded-b-xl">{children}</div>;
}

/* Tooltip */
export const TooltipProvider = TooltipPrimitive.Provider;
export function Tooltip({ content, children, side = "bottom" }: { content: React.ReactNode; children: React.ReactNode; side?: "top" | "bottom" | "left" | "right" }) {
  return (
    <TooltipPrimitive.Root delayDuration={400}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="z-50 max-w-xs rounded-md border border-border bg-surface px-2.5 py-1.5 text-xs text-fg shadow-lg"
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

/* Switch */
export function Switch({ checked, onCheckedChange, disabled, id }: { checked: boolean; onCheckedChange: (v: boolean) => void; disabled?: boolean; id?: string }) {
  return (
    <SwitchPrimitive.Root
      id={id}
      checked={checked}
      disabled={disabled}
      onCheckedChange={onCheckedChange}
      className="relative h-5 w-10 shrink-0 rounded-full border border-border-strong bg-surface-3 transition-colors data-[state=checked]:border-accent data-[state=checked]:bg-accent disabled:opacity-50"
    >
      <SwitchPrimitive.Thumb className="block h-3.5 w-3.5 translate-x-[3px] rounded-full bg-fg-muted transition-transform data-[state=checked]:translate-x-[21px] data-[state=checked]:bg-accent-fg" />
    </SwitchPrimitive.Root>
  );
}

/* Checkbox */
export function Checkbox({ checked, onCheckedChange, disabled, "aria-label": ariaLabel, indeterminate }: { checked: boolean; onCheckedChange: (v: boolean) => void; disabled?: boolean; "aria-label"?: string; indeterminate?: boolean }) {
  return (
    <CheckboxPrimitive.Root
      checked={indeterminate ? "indeterminate" : checked}
      disabled={disabled}
      aria-label={ariaLabel}
      onCheckedChange={(v) => onCheckedChange(v === true)}
      className="flex h-4 w-4 shrink-0 items-center justify-center rounded-[3px] border border-border-strong bg-surface data-[state=checked]:border-accent data-[state=checked]:bg-accent data-[state=indeterminate]:border-accent data-[state=indeterminate]:bg-accent disabled:opacity-50"
    >
      <CheckboxPrimitive.Indicator className="text-accent-fg">
        {indeterminate ? <span className="block h-0.5 w-2 bg-accent-fg" /> : <Check size={12} strokeWidth={3} />}
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}

/* Select */
export function Select<T extends string>({ value, onValueChange, options, disabled, className, placeholder }: {
  value: T; onValueChange: (v: T) => void; options: { value: T; label: string; description?: string }[]; disabled?: boolean; className?: string; placeholder?: string;
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onValueChange(v as T)} disabled={disabled}>
      <SelectPrimitive.Trigger
        className={cn(
          "flex h-8 w-full items-center justify-between gap-2 rounded-md border border-border-strong bg-surface px-2.5 text-[13px] text-fg focus:border-accent focus:outline-none disabled:opacity-50 data-[placeholder]:text-fg-subtle",
          className,
        )}
      >
        <SelectPrimitive.Value placeholder={placeholder} />
        <SelectPrimitive.Icon>
          <ChevronDown size={14} className="text-fg-muted" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content position="popper" sideOffset={4} className="z-50 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-md border border-border bg-surface shadow-lg">
          <SelectPrimitive.Viewport className="p-1">
            {options.map((o) => (
              <SelectPrimitive.Item
                key={o.value}
                value={o.value}
                className="relative flex cursor-default select-none flex-col rounded-sm py-1.5 pl-7 pr-2 text-[13px] outline-none data-[highlighted]:bg-surface-3 data-[state=checked]:text-accent"
              >
                <SelectPrimitive.ItemIndicator className="absolute left-2 top-2">
                  <Check size={12} />
                </SelectPrimitive.ItemIndicator>
                <SelectPrimitive.ItemText>{o.label}</SelectPrimitive.ItemText>
                {o.description && <span className="text-[11px] text-fg-subtle">{o.description}</span>}
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

/* Tabs */
export const Tabs = TabsPrimitive.Root;
export function TabsList({ className, ...props }: TabsPrimitive.TabsListProps) {
  return <TabsPrimitive.List className={cn("flex items-center gap-1 border-b border-border", className)} {...props} />;
}
export function TabsTrigger({ className, ...props }: TabsPrimitive.TabsTriggerProps) {
  return (
    <TabsPrimitive.Trigger
      className={cn(
        "-mb-px border-b-2 border-transparent px-3 py-2 text-[13px] font-medium text-fg-muted hover:text-fg data-[state=active]:border-accent data-[state=active]:text-fg",
        className,
      )}
      {...props}
    />
  );
}
export const TabsContent = TabsPrimitive.Content;

/* Dropdown menu */
export const Dropdown = DropdownPrimitive.Root;
export const DropdownTrigger = DropdownPrimitive.Trigger;
const menuContent = "z-50 min-w-[180px] overflow-hidden rounded-md border border-border bg-surface p-1 shadow-lg";
const menuItem = "flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface-3 data-[disabled]:opacity-40";
export function DropdownContent({ className, ...props }: DropdownPrimitive.DropdownMenuContentProps) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.Content sideOffset={4} align="end" className={cn(menuContent, className)} {...props} />
    </DropdownPrimitive.Portal>
  );
}
export function DropdownItem({ className, ...props }: DropdownPrimitive.DropdownMenuItemProps) {
  return <DropdownPrimitive.Item className={cn(menuItem, className)} {...props} />;
}
export function DropdownSeparator() {
  return <DropdownPrimitive.Separator className="my-1 h-px bg-border" />;
}
export function DropdownLabel({ children }: { children: React.ReactNode }) {
  return <DropdownPrimitive.Label className="px-2 py-1 text-[11px] font-medium text-fg-subtle">{children}</DropdownPrimitive.Label>;
}

/* Context menu */
export const ContextMenu = ContextMenuPrimitive.Root;
export const ContextMenuTrigger = ContextMenuPrimitive.Trigger;
export function ContextMenuContent({ className, ...props }: ContextMenuPrimitive.ContextMenuContentProps) {
  return (
    <ContextMenuPrimitive.Portal>
      <ContextMenuPrimitive.Content className={cn(menuContent, className)} {...props} />
    </ContextMenuPrimitive.Portal>
  );
}
export function ContextMenuItem({ className, ...props }: ContextMenuPrimitive.ContextMenuItemProps) {
  return <ContextMenuPrimitive.Item className={cn(menuItem, className)} {...props} />;
}
export function ContextMenuSeparator() {
  return <ContextMenuPrimitive.Separator className="my-1 h-px bg-border" />;
}

/* Progress */
export function Progress({ value, indeterminate, className, tone = "accent" }: { value: number; indeterminate?: boolean; className?: string; tone?: "accent" | "success" | "warning" | "danger" }) {
  const color = { accent: "bg-accent", success: "bg-success", warning: "bg-warning", danger: "bg-danger" }[tone];
  return (
    <div className={cn("relative h-1.5 w-full overflow-hidden rounded-full bg-surface-3", indeterminate && "progress-indeterminate", className)} role="progressbar" aria-valuenow={indeterminate ? undefined : Math.round(value)}>
      {!indeterminate && <div className={cn("h-full rounded-full transition-[width] duration-200", color)} style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />}
    </div>
  );
}

/* Empty state */
export function EmptyState({ icon, title, description, action, compact }: { icon: React.ReactNode; title: string; description?: React.ReactNode; action?: React.ReactNode; compact?: boolean }) {
  return (
    <div className={cn("flex h-full flex-col items-center justify-center text-center", compact ? "gap-2 p-6" : "gap-3 p-10")}>
      <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-surface-3 text-fg-muted">{icon}</div>
      <div>
        <p className="text-sm font-semibold">{title}</p>
        {description && <p className="mx-auto mt-1 max-w-md text-xs text-fg-muted">{description}</p>}
      </div>
      {action}
    </div>
  );
}

/* Keyboard hint */
export function Kbd({ children }: { children: React.ReactNode }) {
  return <kbd className="rounded border border-border-strong bg-surface-2 px-1.5 py-0.5 font-sans text-[10px] text-fg-muted">{children}</kbd>;
}

/* Spinner */
export function Spinner({ className, size = 16 }: { className?: string; size?: number }) {
  return (
    <svg className={cn("animate-spin text-accent", className)} width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}

/* Section heading for pages */
export function PageHeader({ title, description, actions }: { title: string; description?: string; actions?: React.ReactNode }) {
  return (
    <div className="flex items-end justify-between gap-4 px-6 pb-4 pt-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-0.5 text-xs text-fg-muted">{description}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  );
}
