import { useCallback, useState, type ComponentProps, type ReactNode } from "react";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

type SelectOption = { value: string; label: ReactNode; disabled?: boolean };
type Props = Pick<ComponentProps<typeof SelectTrigger>, "id" | "className" | "disabled" | "title" | "aria-label" | "aria-labelledby"> & {
  value: string;
  onValueChange(value: string): void;
  options: readonly SelectOption[];
  placeholder?: string;
};

/** Shared workbench select, including native-dialog portal placement and empty values. */
export function SelectField({ value, onValueChange, options, placeholder, ...triggerProps }: Props) {
  const [container, setContainer] = useState<HTMLElement | null>(null);
  const attach = useCallback((node: HTMLButtonElement | null) => {
    setContainer(node?.closest("dialog") ?? null);
  }, []);
  // Radix reserves an empty item value for clearing. Encode every value so a
  // selectable "All schemas"/"Default" option can still round-trip as "".
  const encoded = (value: string) => `option:${value}`;
  const selected = options.some(option => option.value === value) ? encoded(value) : "";
  return <Select value={selected} onValueChange={next => onValueChange(next.slice("option:".length))} disabled={triggerProps.disabled}>
    <SelectTrigger {...triggerProps} ref={attach} size="sm"><SelectValue placeholder={placeholder} /></SelectTrigger>
    <SelectContent container={container} position="popper">
      <SelectGroup>{options.map(option => <SelectItem key={option.value} value={encoded(option.value)} disabled={option.disabled}>{option.label}</SelectItem>)}</SelectGroup>
    </SelectContent>
  </Select>;
}
