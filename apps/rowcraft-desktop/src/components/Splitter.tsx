import { type PointerEvent as ReactPointerEvent, type RefObject } from "react";

interface Props {
  /** The element whose height the splitter divides (the editor above, the grid below). */
  container: RefObject<HTMLElement | null>;
  /** Called with the editor's share of the container, 0..1. */
  onChange(fraction: number): void;
  value?: number;
}

const MIN_FRACTION = 0.15;

export function Splitter({ container, onChange, value = 0.45 }: Props) {
  const start = (event: ReactPointerEvent<HTMLDivElement>) => {
    const element = container.current;
    if (!element) return;
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => {
      const box = element.getBoundingClientRect();
      const fraction = (e.clientY - box.top) / box.height;
      onChange(Math.min(1 - MIN_FRACTION, Math.max(MIN_FRACTION, fraction)));
    };
    const stop = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", stop);
      target.removeEventListener("pointercancel", stop);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", stop);
    target.addEventListener("pointercancel", stop);
  };

  return <div className="splitter" role="separator" aria-label="Resize editor and results" aria-orientation="horizontal" tabIndex={0} aria-valuemin={15} aria-valuemax={85} aria-valuenow={Math.round(value * 100)} onPointerDown={start} onKeyDown={(event) => {
    if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    onChange(event.key === "Home" ? MIN_FRACTION : event.key === "End" ? 1 - MIN_FRACTION : Math.max(MIN_FRACTION, Math.min(1 - MIN_FRACTION, value + (event.key === "ArrowDown" ? .05 : -.05))));
  }} />;
}
