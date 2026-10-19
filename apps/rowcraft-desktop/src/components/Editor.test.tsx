// @vitest-environment happy-dom
import { createRef } from "react";
import { afterEach, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Editor, type EditorHandle } from "./Editor";

afterEach(cleanup);

it("updates the live editor font size without replacing its document or selection", () => {
  const handle = createRef<EditorHandle>();
  const { container, rerender } = render(<Editor initial="select 1" handle={handle} preferences={{ fontSize: 13, lineWrapping: false }} />);
  const scroller = container.querySelector<HTMLElement>(".cm-scroller")!;
  expect(getComputedStyle(scroller).fontSize).toBe("13px");
  handle.current!.setText("select 42");
  rerender(<Editor initial="select 1" handle={handle} preferences={{ fontSize: 20, lineWrapping: false }} />);
  expect(container.querySelector(".cm-scroller")).toBe(scroller);
  expect(getComputedStyle(scroller).fontSize).toBe("20px");
  expect(handle.current!.text()).toBe("select 42");
});
