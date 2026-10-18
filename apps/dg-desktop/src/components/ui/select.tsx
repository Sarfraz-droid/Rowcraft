import * as React from "react"
import { cn } from "cn"
import { CheckIcon, ChevronDownIcon, ChevronUpIcon } from "@radix-ui/react-icons"
import { Select as SelectPrimitive } from "radix-ui"

function Select({
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Root>) {
  return <SelectPrimitive.Root data-slot="select" {...props} />
}

function SelectGroup({
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Group>) {
  return <SelectPrimitive.Group data-slot="select-group" {...props} />
}

function SelectValue({
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Value>) {
  return <SelectPrimitive.Value data-slot="select-value" {...props} />
}

function SelectTrigger({
  className,
  size = "default",
  children,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Trigger> & {
  size?: "sm" | "default"
}) {
  return (
    <SelectPrimitive.Trigger
      data-slot="select-trigger"
      data-size={size}
      className={cn(
        "tw:flex tw:w-full tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-md tw:border tw:border-border tw:bg-background tw:px-[9px] tw:py-[5px] tw:text-label tw:whitespace-nowrap tw:transition-[color,box-shadow] tw:outline-none tw:focus-visible:border-primary tw:disabled:cursor-not-allowed tw:disabled:opacity-50 tw:aria-invalid:border-destructive tw:data-[placeholder]:text-muted-foreground tw:data-[size=default]:h-[32px] tw:data-[size=sm]:h-[26px] tw:*:data-[slot=select-value]:truncate tw:*:data-[slot=select-value]:flex tw:*:data-[slot=select-value]:items-center tw:*:data-[slot=select-value]:gap-2 tw:[&_svg]:pointer-events-none tw:[&_svg]:shrink-0 tw:[&_svg:not([class*=size-])]:size-4 tw:[&_svg:not([class*=text-])]:text-muted-foreground",
        className
      )}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon asChild>
        <ChevronDownIcon className="tw:size-4 tw:opacity-50" />
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  )
}

function SelectContent({
  className,
  children,
  position = "item-aligned",
  align = "center",
  container,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Content> & {
  container?: React.ComponentProps<typeof SelectPrimitive.Portal>["container"]
}) {
  return (
    <SelectPrimitive.Portal container={container}>
      <SelectPrimitive.Content
        data-slot="select-content"
        className={cn(
          "tw:relative tw:z-50 tw:max-h-(--radix-select-content-available-height) tw:min-w-[8rem] tw:origin-(--radix-select-content-transform-origin) tw:overflow-x-hidden tw:overflow-y-auto tw:rounded-md tw:border tw:border-border tw:bg-popover tw:text-popover-foreground tw:shadow-mdnn-0n-95",
          position === "popper" &&
            "tw:data-[side=bottom]:translate-y-1 tw:data-[side=left]:-translate-x-1 tw:data-[side=right]:translate-x-1 tw:data-[side=top]:-translate-y-1",
          className
        )}
        position={position}
        align={align}
        {...props}
      >
        <SelectScrollUpButton />
        <SelectPrimitive.Viewport
          className={cn(
            "tw:p-1",
            position === "popper" &&
              "tw:w-full tw:min-w-[var(--radix-select-trigger-width)] tw:scroll-my-1"
          )}
        >
          {children}
        </SelectPrimitive.Viewport>
        <SelectScrollDownButton />
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  )
}

function SelectLabel({
  className,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Label>) {
  return (
    <SelectPrimitive.Label
      data-slot="select-label"
      className={cn("tw:px-2 tw:py-1.5 tw:text-xs tw:text-muted-foreground", className)}
      {...props}
    />
  )
}

function SelectItem({
  className,
  children,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Item>) {
  return (
    <SelectPrimitive.Item
      data-slot="select-item"
      className={cn(
        "tw:relative tw:flex tw:w-full tw:cursor-default tw:items-center tw:gap-2 tw:rounded-sm tw:py-[6px] tw:pr-[28px] tw:pl-[8px] tw:text-label tw:outline-hidden tw:select-none tw:focus:bg-accent tw:focus:text-accent-foreground tw:data-[disabled]:pointer-events-none tw:data-[disabled]:opacity-50 tw:[&_svg]:pointer-events-none tw:[&_svg]:shrink-0 tw:[&_svg:not([class*=size-])]:size-4 tw:[&_svg:not([class*=text-])]:text-muted-foreground tw:*:[span]:last:flex tw:*:[span]:last:items-center tw:*:[span]:last:gap-2",
        className
      )}
      {...props}
    >
      <span
        data-slot="select-item-indicator"
        className="tw:absolute tw:right-2 tw:flex tw:size-3.5 tw:items-center tw:justify-center"
      >
        <SelectPrimitive.ItemIndicator>
          <CheckIcon className="tw:size-4" />
        </SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  )
}

function SelectSeparator({
  className,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Separator>) {
  return (
    <SelectPrimitive.Separator
      data-slot="select-separator"
      className={cn("tw:pointer-events-none tw:-mx-1 tw:my-1 tw:h-px tw:bg-border", className)}
      {...props}
    />
  )
}

function SelectScrollUpButton({
  className,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.ScrollUpButton>) {
  return (
    <SelectPrimitive.ScrollUpButton
      data-slot="select-scroll-up-button"
      className={cn(
        "tw:flex tw:cursor-default tw:items-center tw:justify-center tw:py-1",
        className
      )}
      {...props}
    >
      <ChevronUpIcon className="tw:size-4" />
    </SelectPrimitive.ScrollUpButton>
  )
}

function SelectScrollDownButton({
  className,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.ScrollDownButton>) {
  return (
    <SelectPrimitive.ScrollDownButton
      data-slot="select-scroll-down-button"
      className={cn(
        "tw:flex tw:cursor-default tw:items-center tw:justify-center tw:py-1",
        className
      )}
      {...props}
    >
      <ChevronDownIcon className="tw:size-4" />
    </SelectPrimitive.ScrollDownButton>
  )
}

export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
}
