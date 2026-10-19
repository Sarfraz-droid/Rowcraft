// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { api } from "../api";
import { chooseOption } from "../test-utils/select";
import { SqlAssistant } from "./SqlAssistant";

beforeEach(() => {
  vi.spyOn(api, "aiModels").mockResolvedValue([{ id: "free-sql", owned_by: "gateway" }, { id: "alternate", owned_by: "gateway" }]);
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value: function(this: HTMLDialogElement) { this.open = true; } });
  vi.spyOn(api, "aiSettings").mockResolvedValue({ base_url: "http://localhost:20128/v1", model: "free-sql", fallback_models: [], has_api_key: false });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("inserts the chosen MongoDB code block rather than the first block in the response", async () => {
  vi.spyOn(api, "aiComplete").mockResolvedValue({ content: 'List collections:\n```javascript\ndb.getCollectionNames()\n```\nSample bots:\n```javascript\ndb.getCollection("bots").find({}).limit(25)\n```', model: "free-sql" });
  const use = vi.fn();
  render(<SqlAssistant sql="" dialect="mongo" schema="main" catalog={null} onClose={vi.fn()} onSettings={vi.fn()} onUse={use} />);
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Inspect bots" } });
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await screen.findByRole("region", { name: "AI response" });
  const buttons = screen.getAllByRole("button", { name: "Use MongoDB shell in editor" });
  expect(buttons).toHaveLength(2);
  fireEvent.click(buttons[1]!);
  expect(use).toHaveBeenLastCalledWith('db.getCollection("bots").find({}).limit(25)');
  fireEvent.click(buttons[0]!);
  expect(use).toHaveBeenLastCalledWith("db.getCollectionNames()");
});

it("previews generated SQL and inserts it only after the user explicitly chooses Use SQL", async () => {
  const complete = vi.spyOn(api, "aiComplete").mockResolvedValue({ content: "```sql\nSELECT count(*) FROM people;\n```", model: "free/backup" });
  const use = vi.fn();
  render(<SqlAssistant sql="" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={vi.fn()} onUse={use} />);
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Count people" } });
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await screen.findByRole("region", { name: "AI response" });
  expect(complete).toHaveBeenCalledOnce();
  expect(use).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Use SQL in editor" }));
  expect(use).toHaveBeenCalledWith("SELECT count(*) FROM people;");
});

it("points to provider settings when no AI model is configured", async () => {
  vi.mocked(api.aiSettings).mockResolvedValue({ base_url: "http://localhost:20128/v1", model: "", fallback_models: [], has_api_key: false });
  const settings = vi.fn();
  render(<SqlAssistant sql="SELECT 1" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={settings} onUse={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Configure AI providers" }));
  expect(settings).toHaveBeenCalledOnce();
  expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(true);
});

it("keeps the conversation and sends earlier turns when asking a follow-up", async () => {
  const complete = vi.spyOn(api, "aiComplete")
    .mockResolvedValueOnce({ content: "This query selects one value.", model: "free-sql" })
    .mockResolvedValueOnce({ content: "```sql\nSELECT 2;\n```", model: "free-sql" });
  render(<SqlAssistant sql="SELECT 1" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={vi.fn()} onUse={vi.fn()} />);
  expect(screen.queryByRole("tablist")).toBeNull();
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Explain this query" } });
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await screen.findByText("This query selects one value.");
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Make it select two instead" } });
  fireEvent.keyDown(screen.getByLabelText("Message AI assistant"), { key: "Enter" });
  await screen.findByRole("button", { name: "Use SQL in editor" });
  expect(screen.getByText("Explain this query")).toBeTruthy();
  expect(screen.getByText("This query selects one value.")).toBeTruthy();
  expect(complete.mock.calls[1]?.[0]).toEqual(expect.arrayContaining([
    { role: "user", content: "Explain this query" },
    { role: "assistant", content: "This query selects one value." },
    expect.objectContaining({ role: "user", content: expect.stringContaining("Make it select two instead") }),
  ]));
});

it("uses the selected model for requests without changing saved settings", async () => {
  const complete = vi.spyOn(api, "aiComplete").mockResolvedValue({ content: "Selected model response", model: "alternate" });
  const save = vi.spyOn(api, "saveAiSettings");
  render(<SqlAssistant sql="SELECT 1" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={vi.fn()} onUse={vi.fn()} />);
  const selector = screen.getByRole("combobox", { name: "Assistant model" });
  await waitFor(() => expect(selector.hasAttribute("disabled")).toBe(false));
  await chooseOption(selector, "alternate");
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Explain" } });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await screen.findByText("Selected model response");
  expect(complete).toHaveBeenCalledWith(expect.any(Array), false, "alternate");
  expect(save).not.toHaveBeenCalled();
});

it("keeps the configured model usable when model discovery fails", async () => {
  vi.mocked(api.aiModels).mockRejectedValue(new Error("Gateway unavailable"));
  const settings = vi.fn();
  render(<SqlAssistant sql="" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={settings} onUse={vi.fn()} />);
  await screen.findByText(/Could not load models/);
  const composer = screen.getByRole("combobox", { name: "Assistant model" }).closest("form")!;
  fireEvent.click(within(composer).getByRole("button", { name: "Open AI settings" }));
  expect(settings).toHaveBeenCalledOnce();
  expect(screen.getByRole("combobox", { name: "Assistant model" }).textContent).toContain("free-sql");
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Help" } });
  expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false);
});

it("offers an action to open a console when there is no active SQL editor", () => {
  const open = vi.fn();
  render(<SqlAssistant sql="" dialect="mysql" source="Analytics" schema="" catalog={null} enabled={false} onOpenConsole={open} onClose={vi.fn()} onSettings={vi.fn()} onUse={vi.fn()} />);
  expect(screen.getByRole("textbox", { name: "Message AI assistant" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Open SQL console" }));
  expect(open).toHaveBeenCalledOnce();
});

it("fills a starter prompt without sending a request", async () => {
  const complete = vi.spyOn(api, "aiComplete");
  render(<SqlAssistant sql="SELECT 1" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={vi.fn()} onUse={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Explain this SQL" }));
  expect((screen.getByRole("textbox", { name: "Message AI assistant" }) as HTMLTextAreaElement).value).toBe("Explain the SQL in the current editor.");
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Message AI assistant" }));
  expect(complete).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false));
});

it("stops a pending response and ignores its late result", async () => {
  let resolve!: (reply: { content: string; model: string }) => void;
  vi.spyOn(api, "aiComplete").mockImplementation(() => new Promise(done => { resolve = done; }));
  render(<SqlAssistant sql="SELECT 1" dialect="pg" schema="main" catalog={null} onClose={vi.fn()} onSettings={vi.fn()} onUse={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Message AI assistant"), { target: { value: "Explain this" } });
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  fireEvent.click(await screen.findByRole("button", { name: "Stop response" }));
  expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(false);
  await act(async () => { resolve({ content: "Late response", model: "free-sql" }); });
  expect(screen.queryByText("Late response")).toBeNull();
});
