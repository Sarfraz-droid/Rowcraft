// Small form controls shared by the dialog's tabs.

import type { ReactNode } from "react";

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
    </label>
  );
}

interface TextProps {
  label: string;
  value: string;
  placeholder?: string;
  onChange(value: string): void;
  autoFocus?: boolean;
}

export function TextField({ label, value, placeholder, onChange, autoFocus }: TextProps) {
  return (
    <Field label={label}>
      <input
        className="input"
        type="text"
        value={value}
        placeholder={placeholder}
        autoFocus={autoFocus}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}

interface AreaProps {
  label: string;
  value: string;
  placeholder?: string;
  rows?: number;
  onChange(value: string): void;
}

export function TextArea({ label, value, placeholder, rows = 4, onChange }: AreaProps) {
  return (
    <Field label={label}>
      <textarea
        className="input mono"
        value={value}
        rows={rows}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}

interface ChoiceProps<T extends string> {
  label: string;
  value: T;
  options: ReadonlyArray<readonly [label: string, value: T]>;
  onChange(value: T): void;
}

/** One option of a few, as a row of toggle buttons. */
export function Choice<T extends string>({ label, value, options, onChange }: ChoiceProps<T>) {
  return (
    <div className="field" role="radiogroup" aria-label={label}>
      <span className="field-label">{label}</span>
      <div className="chips tw:flex tw:flex-wrap tw:gap-[5px]">
        {options.map(([text, option]) => (
          <button
            type="button"
            key={option}
            role="radio"
            aria-checked={option === value}
            className={`chip${option === value ? " on" : ""}`}
            onClick={() => onChange(option)}
          >
            {text}
          </button>
        ))}
      </div>
    </div>
  );
}

export function Check({ label, checked, onChange }: { label: string; checked: boolean; onChange(v: boolean): void }) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

export function Heading({ children }: { children: ReactNode }) {
  return <h3 className="heading">{children}</h3>;
}

export function Note({ children }: { children: ReactNode }) {
  return <p className="note">{children}</p>;
}
