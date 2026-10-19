// @vitest-environment happy-dom
import { useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { chooseOption } from "../test-utils/select";
import { SelectField } from "./SelectField";

beforeEach(() => {
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
});
afterEach(cleanup);

function SchemaFilter() {
  const [schema, setSchema] = useState("");
  return <><SelectField aria-label="Schema filter" value={schema} onValueChange={setSchema} options={[
    { value: "", label: "All schemas" },
    { value: "audit", label: "Audit" },
    { value: "offline", label: "Offline", disabled: true },
  ]} /><output aria-label="Selected schema">{schema}</output></>;
}

it("selects and clears a schema using the keyboard without native select elements", async () => {
  const { container } = render(<SchemaFilter />);
  expect(container.querySelector("select")).toBeNull();
  await chooseOption(screen.getByRole("combobox", { name: "Schema filter" }), "Audit");
  expect(screen.getByLabelText("Selected schema").textContent).toBe("audit");
  await chooseOption(screen.getByRole("combobox", { name: "Schema filter" }), "All schemas");
  expect(screen.getByLabelText("Selected schema").textContent).toBe("");
  expect(screen.getByRole("combobox").textContent).toBe("All schemas");
});

it("mounts the menu inside its dialog and marks disabled options", async () => {
  render(<dialog open><SchemaFilter /></dialog>);
  fireEvent.keyDown(screen.getByRole("combobox"), { key: "ArrowDown" });
  const menu = await screen.findByRole("listbox");
  expect(menu.closest("dialog")).toBe(screen.getByRole("dialog"));
  expect(screen.getByRole("option", { name: "Offline" }).getAttribute("aria-disabled")).toBe("true");
});

it("keeps a disabled select closed and displays its empty-state placeholder", () => {
  render(<SelectField aria-label="Schema" value="" options={[]} placeholder="No schemas available" disabled onValueChange={vi.fn()} />);
  const trigger = screen.getByRole("combobox", { name: "Schema" });
  expect(trigger.hasAttribute("disabled")).toBe(true);
  expect(trigger.textContent).toBe("No schemas available");
  fireEvent.click(trigger);
  expect(screen.queryByRole("listbox")).toBeNull();
});
