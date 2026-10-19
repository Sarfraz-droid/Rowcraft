import { fireEvent, screen } from "@testing-library/react";

/** Exercise the same keyboard selection path as a user of the shadcn Select. */
export async function chooseOption(trigger: HTMLElement, name: string) {
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  const option = await screen.findByRole("option", { name });
  fireEvent.keyDown(option, { key: "Enter" });
}
