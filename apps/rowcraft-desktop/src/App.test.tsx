// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import { chooseOption } from "./test-utils/select";
import { api, type QueryOutput } from "./api";
import { installMock } from "./mock";
import { runSqlAssistance } from "./ai-harness";

vi.mock("./ai-harness", () => ({ runSqlAssistance: vi.fn() }));

vi.mock("./api", async (original) => {
  const actual = await original<typeof import("./api")>();
  return { ...actual, api: { editSource: vi.fn(), updateSource: vi.fn(), drivers: vi.fn(), driverSettings: vi.fn(), driverPackage: vi.fn().mockResolvedValue({ distribution: "native", installed: true, files: [], version: null, source: null }), aiSettings: vi.fn(), aiRouterStatus: vi.fn(), stopAiRouter: vi.fn(), bootstrap: vi.fn(), connect: vi.fn(), introspect: vi.fn(), runQuery: vi.fn(), previewStatement: vi.fn(), removeSource: vi.fn() } };
});
vi.mock("./components/Editor", async () => {
  const { useEffect, useRef } = await import("react");
  return { Editor: ({ initial, handle, onChange }: { initial: string; handle: {current: unknown}; onChange?(sql: string): void }) => {
    const input = useRef<HTMLTextAreaElement>(null);
    useEffect(() => {
      handle.current = {
        text: () => input.current!.value, statement: () => input.current!.value,
        setText: (value: string) => { input.current!.value = value; },
        selection: () => {
          const { selectionStart: from, selectionEnd: to, value } = input.current!;
          return { from, to, text: value.slice(from, to) };
        },
        replaceRange: (from: number, to: number, value: string) => { input.current!.setRangeText(value, from, to, "end"); },
        focus: () => input.current!.focus(),
      };
      return () => { handle.current = null; };
    }, [handle]);
    return <textarea ref={input} aria-label="SQL editor" defaultValue={initial} onChange={event => onChange?.(event.target.value)} />;
  } };
});
vi.mock("./components/ResultGrid", () => ({ ResultGrid: ({ result }: { result: QueryOutput | null }) => <output aria-label="Query output">{JSON.stringify(result?.rows)}</output> }));

const sources = [
  { name: "Alpha", dialect: "sqlite", host: null, ssh_tunnel: null, color: null },
  { name: "Beta", dialect: "sqlite", host: null, ssh_tunnel: null, color: null },
];
const catalog = { database: null, schemas: [{ name: "main", tables: [{ name: "people", kind: "Table" as const, columns: [{ name: "id", native_type: "INTEGER", nullable: false }] }] }] };
const tab = (title: string) => within(screen.getByRole("tablist", { name: "Open workspace documents" })).getByRole("tab", { name: new RegExp(title) });
const activePanel = () => screen.getByRole("tabpanel");
const addConsole = async () => {
  fireEvent.click(screen.getByText("Independent query editor").closest("button")!);
  await waitFor(() => expect(screen.getByRole("button", { name: /^Run/ }).hasAttribute("disabled")).toBe(false));
};
async function start() {
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /Alpha (sqlite|mysql)/ }));
  await screen.findByRole("button", { name: "Refresh schema" });
  await waitFor(() => expect(api.introspect).toHaveBeenCalledOnce());
  fireEvent.click(within(screen.getByRole("navigation", { name: "Alpha views" })).getByRole("button", { name: "SQL console" }));
  await screen.findByRole("textbox", { name: "SQL editor" });
}
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.mocked(api.aiSettings).mockResolvedValue({ base_url: "http://localhost:20128/v1", model: "", fallback_models: [], has_api_key: false });
  vi.mocked(api.aiRouterStatus).mockResolvedValue({ installed: true, running: false, version: "test", managed: false, pid: null, base_url: "http://localhost:20128/v1" });
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value: function(this: HTMLDialogElement) { this.open = true; } });
  Object.defineProperty(HTMLElement.prototype, "hidePopover", { configurable: true, value: vi.fn() });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.mocked(api.bootstrap).mockResolvedValue({ sources, startup_error: null });
  vi.mocked(api.connect).mockImplementation(async (name) => name);
  vi.mocked(api.introspect).mockResolvedValue(catalog);
  vi.mocked(api.previewStatement).mockResolvedValue('select * from "main"."people" limit 1000');
  vi.mocked(api.runQuery).mockImplementation(async (sql) => ({ columns: ["value"], rows: [[sql]], summary: `result: ${sql}` }));
});
afterEach(cleanup);

describe("workspace document interactions", () => {
  it("reuses a browsing preview, keeps a chosen tab, and preserves console drafts", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select keep_this_draft" } });
    const nav = within(screen.getByRole("navigation", { name: "Alpha views" }));
    fireEvent.click(nav.getByRole("button", { name: "Schema map" }));
    await waitFor(() => expect(tab("Schema map").getAttribute("aria-selected")).toBe("true"));
    expect(screen.queryByRole("tab", { name: /Overview/ })).toBeNull();
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    fireEvent.doubleClick(tab("Schema map"));
    expect(screen.queryByRole("button", { name: "Keep Schema map for Alpha open" })).toBeNull();
    fireEvent.click(nav.getByRole("button", { name: /^Databases/ }));
    await waitFor(() => expect(tab("Databases").getAttribute("aria-selected")).toBe("true"));
    expect(tab("Schema map")).toBeTruthy();
    fireEvent.click(nav.getByRole("button", { name: "Query history" }));
    await waitFor(() => expect(tab("Query history").getAttribute("aria-selected")).toBe("true"));
    expect(screen.queryByRole("tab", { name: /^Databases/ })).toBeNull();
    expect(screen.getAllByRole("tab")).toHaveLength(3);
    fireEvent.click(tab("Console 1"));
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select keep_this_draft");
  });

  it("moves a connection and its editor drafts to another workspace", async () => {
    localStorage.setItem("rowcraft.workspaces.v1", JSON.stringify({ version: 1, activeId: "default", workspaces: [
      { id: "default", name: "Workspace", sources: ["Alpha", "Beta"] },
      { id: "archive", name: "Archive", sources: [] },
    ] }));
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select 'keep this draft'" } });
    fireEvent.pointerDown(screen.getByRole("button", { name: "Actions for Alpha" }), { button: 0, ctrlKey: false, pointerType: "mouse" });
    const menu = await screen.findByRole("menu", { name: "Actions for Alpha" });
    fireEvent.keyDown(within(menu).getByRole("menuitem", { name: "Move to workspace" }), { key: "ArrowRight" });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Archive" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: /Alpha sqlite/ })).toBeNull());
    expect(screen.queryByRole("tab", { name: /Alpha/ })).toBeNull();
    const settings = JSON.parse(localStorage.getItem("rowcraft.workspaces.v1")!);
    expect(settings.workspaces[0].sources).toEqual(["Beta"]);
    expect(settings.workspaces[1].sources).toEqual(["Alpha"]);
    fireEvent.click(screen.getByRole("button", { name: /Archive 1 data source/ }));
    expect(await screen.findByRole("button", { name: /Alpha sqlite/ })).toBeTruthy();
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select 'keep this draft'");
    expect(screen.getByRole("tab", { name: /Console 1.*Alpha/ })).toBeTruthy();
  });

  it("keeps one settings gear with the assistant open or collapsed", async () => {
    await start();
    expect(screen.getAllByRole("button", { name: "Settings" })).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "AI settings" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close SQL assistant" }));
    expect(screen.getAllByRole("button", { name: "Settings" })).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Open AI settings" })).toBeNull();
  });

  it("keeps both sources online and reuses metadata and results when switching tabs", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", {name:"SQL editor"}), {target:{value:"select 'alpha draft'"}});
    fireEvent.click(screen.getByRole("button", {name:/^Run/}));
    await within(activePanel()).findByText("result: select 'alpha draft'");
    await waitFor(() => expect(screen.getByRole("button", {name:/^Run/}).hasAttribute("disabled")).toBe(false));
    const alphaTab = tab("Console 1.*Alpha");
    fireEvent.click(screen.getByRole("button", {name:/Beta sqlite/}));
    await waitFor(() => expect(api.introspect).toHaveBeenCalledTimes(2));
    expect(document.querySelectorAll('.sidebar .connection-dot.online')).toHaveLength(2);
    fireEvent.click(alphaTab);
    await waitFor(() => expect(api.connect).toHaveBeenLastCalledWith("Alpha"));
    expect(api.introspect).toHaveBeenCalledTimes(2);
    expect(api.runQuery).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Connecting…")).toBeNull();
    expect((screen.getByRole("textbox", {name:"SQL editor"}) as HTMLTextAreaElement).value).toBe("select 'alpha draft'");
    expect(within(activePanel()).getByText("result: select 'alpha draft'")).toBeTruthy();
    const calls = vi.mocked(api.connect).mock.calls.length;
    fireEvent.click(alphaTab);
    await waitFor(() => expect(screen.getByRole("button", {name:/^Run/}).hasAttribute('disabled')).toBe(false));
    expect(api.connect).toHaveBeenCalledTimes(calls);
    expect(api.introspect).toHaveBeenCalledTimes(2);
  });

  it("reconnects only when explicitly requested from source actions", async () => {
    await start();
    fireEvent.contextMenu(screen.getByRole('button', {name:/Alpha sqlite/}));
    const menu = await screen.findByRole('menu', {name:'Actions for Alpha'});
    fireEvent.click(within(menu).getByRole('menuitem', {name:'Reconnect'}));
    await waitFor(() => expect(api.connect).toHaveBeenLastCalledWith('Alpha',true));
    await waitFor(() => expect(api.introspect).toHaveBeenCalledTimes(2));
  });

  it("shows the default limit and reruns the original result query with Run full query", async () => {
    await start();
    vi.mocked(api.runQuery)
      .mockResolvedValueOnce({ columns: ["id"], rows: [[1]], summary: "1 row · Default limit: 2000 rows", row_limit: 2000 })
      .mockResolvedValueOnce({ columns: ["id"], rows: [[1], [2]], summary: "2 rows" });
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select * from company_reviews" } });
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    const showAll = await screen.findByRole("button", { name: "Run full query" });
    expect(screen.getAllByText(/Default limit: 2000 rows/).length).toBeGreaterThan(0);
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "delete from company_reviews" } });
    await waitFor(() => expect(showAll.hasAttribute("disabled")).toBe(false));
    fireEvent.click(showAll);
    await waitFor(() => expect(api.runQuery).toHaveBeenLastCalledWith("select * from company_reviews", undefined, true));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Run full query" })).toBeNull());
  });

  it("opens MongoDB consoles with native commands and uses the selected database", async () => {
    vi.mocked(api.bootstrap).mockResolvedValue({ sources: sources.map(source => ({...source,dialect:"mongo"})), startup_error:null });
    vi.mocked(api.introspect).mockResolvedValue({...catalog,database:"communitiesDb",default_schema:"communitiesDb",schemas:[{...catalog.schemas[0]!,name:"communitiesDb"},{name:"otherDb",tables:[]}]});
    render(<App />);
    fireEvent.click(await screen.findByRole("button",{name:/Alpha mongo/}));
    await waitFor(() => expect(api.introspect).toHaveBeenCalledOnce());
    fireEvent.click(within(screen.getByRole("navigation",{name:"Alpha views"})).getByRole("button",{name:"MongoDB console"}));
    const editor = await screen.findByRole("textbox",{name:"SQL editor"}) as HTMLTextAreaElement;
    expect(editor.value).toBe("show collections\n");
    expect(screen.getByRole("combobox",{name:"Database for Console 1"}).hasAttribute("disabled")).toBe(false);
    fireEvent.click(screen.getByRole("button",{name:/^Run/}));
    await waitFor(() => expect(api.runQuery).toHaveBeenCalledWith("show collections\n","communitiesDb"));
    await chooseOption(screen.getByRole("combobox",{name:"Database for Console 1"}),"otherDb");
    fireEvent.click(screen.getByRole("button",{name:/^Run/}));
    await waitFor(() => expect(api.runQuery).toHaveBeenLastCalledWith("show collections\n","otherDb"));
    expect(screen.getByRole("button",{name:"JSON"})).toBeTruthy();
  });

  it("keeps the Retro dark theme class in sync with the saved theme and toggle", async () => {
    localStorage.setItem("rowcraft.theme", "dark");
    render(<App />);
    await screen.findByRole("button", { name: "Switch to light theme" });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(localStorage.getItem("rowcraft.theme")).toBe("light");

    fireEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(localStorage.getItem("rowcraft.theme")).toBe("dark");
  });

  it("opens inline AI with Command K from the editor and shows a shortcut hint", async () => {
    await start();
    expect(screen.queryByRole("button", { name: /Inline AI for/ })).toBeNull();
    expect(screen.getByText("to open inline AI", { exact: false })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("textbox", { name: "SQL editor" }), { key: "k", code: "KeyK", metaKey: true });
    expect(screen.getByRole("region", { name: "Inline AI edit" })).toBeTruthy();
    expect(api.runQuery).not.toHaveBeenCalled();
  });

  it("shows the AI sidebar and opens inline editing with Command Option K", async () => {
    await start();
    expect(screen.getByRole("complementary", { name: "AI assistant" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "AI assistant" })).toBeNull();
    fireEvent.keyDown(window, { key: "k", metaKey: true, altKey: true });
    expect(screen.getByRole("region", { name: "Inline AI edit" })).toBeTruthy();
    expect(api.runQuery).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Inline SQL instruction" }), { key: "Escape" });
    expect(screen.queryByRole("region", { name: "Inline AI edit" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close SQL assistant" }));
    expect(screen.queryByRole("complementary", { name: "AI assistant" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open AI assistant sidebar" }));
    expect(screen.getByRole("complementary", { name: "AI assistant" })).toBeTruthy();
  });

  it("opens settings directly from inline AI while the router is stopped and preserves its instruction", async () => {
    await start();
    fireEvent.keyDown(window, { key: "k", metaKey: true, altKey: true });
    const panel = screen.getByRole("region", { name: "Inline AI edit" });
    fireEvent.change(within(panel).getByRole("textbox"), { target: { value: "Keep this instruction" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Open AI settings" }));
    const settings = await screen.findByRole("dialog", { name: "Settings" });
    await waitFor(() => expect(within(settings).getByRole("button", { name: "Start 9router" }).hasAttribute("disabled")).toBe(false));
    fireEvent.click(within(settings).getByRole("button", { name: "Close settings" }));
    expect((screen.getByRole("textbox", { name: "Inline SQL instruction" }) as HTMLTextAreaElement).value).toBe("Keep this instruction");
  });

  it("autosaves and applies general preferences immediately across settings sections", async () => {
    await start();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    const settings = screen.getByRole("dialog", { name: "Settings" });
    expect(within(settings).getByRole("button", { name: "General" }).getAttribute("aria-current")).toBe("page");
    expect(within(settings).queryByRole("button", { name: "Save settings" })).toBeNull();
    await chooseOption(within(settings).getByRole("combobox", { name: "Theme" }), "Light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("rowcraft.theme")).toBe("light");
    await chooseOption(within(settings).getByRole("combobox", { name: "Font size" }), "16 px");
    expect(document.documentElement.style.getPropertyValue("zoom")).toBe(String(16 / 13));
    fireEvent.click(within(settings).getByRole("checkbox", { name: "Wrap long lines in the SQL editor" }));
    await within(settings).findByText("General settings saved.");
    expect(JSON.parse(localStorage.getItem("rowcraft.editor.v1")!)).toEqual({ fontSize: 16, lineWrapping: true });
    fireEvent.click(within(settings).getByRole("button", { name: "AI" }));
    await within(settings).findByRole("textbox", { name: "Gateway URL" });
    fireEvent.click(within(settings).getByRole("button", { name: "General" }));
    expect((within(settings).getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
    expect(JSON.parse(localStorage.getItem("rowcraft.editor.v1")!)).toEqual({ fontSize: 16, lineWrapping: true });
    expect(document.documentElement.dataset.theme).toBe("light");
    fireEvent.click(within(settings).getByRole("button", { name: "Close settings" }));
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect((screen.getByRole("checkbox", { name: "Wrap long lines in the SQL editor" }) as HTMLInputElement).checked).toBe(true);
  });

  it("resizes the whole app with Command/Ctrl shortcuts, including inside settings", async () => {
    await start();
    const root = document.documentElement;
    const press = (key: string, modifiers: { metaKey?: boolean; ctrlKey?: boolean } = { metaKey: true }) => {
      const event = new KeyboardEvent("keydown", { key, ...modifiers, bubbles: true, cancelable: true });
      fireEvent(window, event);
      expect(event.defaultPrevented).toBe(true);
    };
    press("+");
    expect(root.style.getPropertyValue("zoom")).toBe(String(14 / 13));
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    press("=", { ctrlKey: true });
    expect(root.style.getPropertyValue("zoom")).toBe(String(15 / 13));
    expect(screen.getByRole("combobox", { name: "Font size" }).textContent).toContain("15 px");
    press("-");
    expect(root.style.getPropertyValue("zoom")).toBe(String(14 / 13));
    press("0");
    expect(root.style.getPropertyValue("zoom")).toBe("1");
    for (let i = 0; i < 20; i++) press("-");
    expect(root.style.getPropertyValue("zoom")).toBe(String(11 / 13));
    for (let i = 0; i < 20; i++) press("+");
    expect(root.style.getPropertyValue("zoom")).toBe(String(20 / 13));
    expect(JSON.parse(localStorage.getItem("rowcraft.editor.v1")!).fontSize).toBe(20);
  });

  it("stops a local router started outside the app from AI settings", async () => {
    vi.mocked(api.aiRouterStatus).mockResolvedValue({ installed: false, running: true, version: null, managed: false, pid: 4321, base_url: "http://localhost:20128/v1" });
    vi.mocked(api.stopAiRouter).mockResolvedValue({ installed: false, running: false, version: null, managed: false, pid: null, base_url: null });
    await start();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "AI" }));
    const settings = await screen.findByRole("dialog", { name: "Settings" });
    const stop = await within(settings).findByRole("button", { name: "Stop" });
    expect(stop.hasAttribute("disabled")).toBe(false);
    fireEvent.click(stop);
    await within(settings).findByText("9router stopped.");
    expect(api.stopAiRouter).toHaveBeenCalledWith("http://localhost:20128/v1");
    expect(stop.hasAttribute("disabled")).toBe(true);
  });

  it("keeps Stop disabled for an existing gateway without a verified local router process", async () => {
    vi.mocked(api.aiRouterStatus).mockResolvedValue({ installed: false, running: true, version: null, managed: false, pid: null, base_url: "http://localhost:20128/v1" });
    await start();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "AI" }));
    const settings = await screen.findByRole("dialog", { name: "Settings" });
    expect((await within(settings).findByRole("button", { name: "Stop" })).hasAttribute("disabled")).toBe(true);
    expect(api.stopAiRouter).not.toHaveBeenCalled();
  });

  it("recognizes the macOS Option character and preserves an open inline instruction", async () => {
    await start();
    fireEvent.keyDown(window, { key: "˚", code: "KeyK", metaKey: true, altKey: true });
    const instruction = screen.getByRole("textbox", { name: "Inline SQL instruction" });
    fireEvent.change(instruction, { target: { value: "Count people" } });
    fireEvent.keyDown(instruction, { key: "˚", code: "KeyK", metaKey: true, altKey: true });
    expect((instruction as HTMLTextAreaElement).value).toBe("Count people");
    expect(api.runQuery).not.toHaveBeenCalled();
  });

  it("previews generated SQL and inserts it at the cursor only after applying", async () => {
    await start();
    vi.mocked(api.aiSettings).mockResolvedValue({ base_url: "", model: "test", fallback_models: [], has_api_key: false });
    vi.mocked(runSqlAssistance).mockResolvedValue({ content: "```sql\nSELECT count(*) FROM people;\n```", model: "test" });
    const editor = screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement;
    editor.setSelectionRange(editor.value.length, editor.value.length);
    fireEvent.keyDown(editor, { key: "˚", code: "KeyK", metaKey: true, altKey: true });
    const panel = screen.getByRole("region", { name: "Inline AI edit" });
    expect(within(panel).queryByRole("group", { name: "Inline SQL action" })).toBeNull();
    expect(within(panel).queryByRole("checkbox")).toBeNull();
    const instruction = within(panel).getByRole("textbox", { name: "Inline SQL instruction" });
    fireEvent.change(instruction, { target: { value: "Count people" } });
    fireEvent.keyDown(instruction, { key: "Enter" });
    await within(panel).findByLabelText("SQL edit preview");
    expect(editor.value).toBe("select 1;\n");
    fireEvent.click(within(panel).getByRole("button", { name: "Apply edit" }));
    expect(editor.value).toBe("select 1;\nSELECT count(*) FROM people;");
    expect(api.runQuery).not.toHaveBeenCalled();
  });

  it("fixes selected SQL and includes the database error", async () => {
    await start();
    vi.mocked(api.runQuery).mockRejectedValueOnce(new Error("no such column: missing"));
    const editor = screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement;
    fireEvent.change(editor, { target: { value: "SELECT missing FROM people;\nSELECT 2;" } });
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    await screen.findAllByText("no such column: missing");
    vi.mocked(api.aiSettings).mockResolvedValue({ base_url: "", model: "test", fallback_models: [], has_api_key: false });
    vi.mocked(runSqlAssistance).mockResolvedValue({ content: "```sql\nSELECT id FROM people;\n```", model: "test" });
    editor.setSelectionRange(0, "SELECT missing FROM people;".length);
    fireEvent.keyDown(editor, { key: "k", metaKey: true, altKey: true });
    const panel = screen.getByRole("region", { name: "Inline AI edit" });
    fireEvent.submit(within(panel).getByRole("textbox").closest("form")!);
    await within(panel).findByLabelText("SQL edit preview");
    expect(vi.mocked(runSqlAssistance).mock.calls.at(-1)?.[0]).toMatchObject({ action: "fix", sql: "SELECT missing FROM people;", error: "no such column: missing" });
    fireEvent.click(within(panel).getByRole("button", { name: "Apply edit" }));
    expect(editor.value).toBe("SELECT id FROM people;\nSELECT 2;");
    expect(api.runQuery).toHaveBeenCalledOnce();
  });

  it("refuses to overwrite SQL changed after opening inline AI", async () => {
    await start();
    vi.mocked(api.aiSettings).mockResolvedValue({ base_url: "", model: "test", fallback_models: [], has_api_key: false });
    vi.mocked(runSqlAssistance).mockResolvedValue({ content: "```sql\nSELECT 2;\n```", model: "test" });
    fireEvent.keyDown(window, { key: "k", metaKey: true, altKey: true });
    const panel = screen.getByRole("region", { name: "Inline AI edit" });
    fireEvent.change(within(panel).getByRole("textbox"), { target: { value: "Write a query selecting 2" } });
    fireEvent.submit(within(panel).getByRole("textbox").closest("form")!);
    await within(panel).findByLabelText("SQL edit preview");
    const editor = screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement;
    fireEvent.change(editor, { target: { value: "SELECT keep_my_work;" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Apply edit" }));
    expect(within(panel).getByRole("alert").textContent).toContain("SQL changed");
    expect(editor.value).toBe("SELECT keep_my_work;");
  });

  it("runs MySQL queries in each console’s selected schema", async () => {
    vi.mocked(api.bootstrap).mockResolvedValue({ sources: sources.map((source) => ({ ...source, dialect: "mysql" })), startup_error: null });
    vi.mocked(api.introspect).mockResolvedValue({ ...catalog, default_schema: null, schemas: [...catalog.schemas, { name: "audit", tables: [] }] });
    await start();
    await chooseOption(screen.getByRole("combobox", { name: "Schema for Console 1" }), "audit");
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    await waitFor(() => expect(api.runQuery).toHaveBeenLastCalledWith("select 1;\n", "audit"));
    await waitFor(() => expect(screen.getByRole("button", { name: /^Run/ }).hasAttribute("disabled")).toBe(false));
    await addConsole();
    expect(screen.getByRole("combobox", { name: "Schema for Console 2" }).textContent).toBe("main");
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    await waitFor(() => expect(api.runQuery).toHaveBeenLastCalledWith("select 1;\n", "main"));
    await waitFor(() => expect(screen.getByRole("button", { name: /^Run/ }).hasAttribute("disabled")).toBe(false));
    fireEvent.click(tab("Console 1"));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Schema for Console 1" }).textContent).toBe("audit"));
    fireEvent.click(screen.getByRole("button", { name: /Beta mysql/ }));
    await waitFor(() => expect(api.connect).toHaveBeenLastCalledWith("Beta"));
    fireEvent.click(within(screen.getByRole("navigation", { name: "Beta views" })).getByRole("button", { name: "SQL console" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Schema for Console 1" }).textContent).toBe("main"));
    expect(screen.queryByRole("combobox", { name: /Database for/ })).toBeNull();

  });
  it("renames the current workspace without losing its sources or SQL draft, and retains the name after reload", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select keep_draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Rename workspace" }));
    const input = screen.getByRole("textbox", { name: "Workspace name" }) as HTMLInputElement;
    expect(input.value).toBe("Workspace");
    fireEvent.change(input, { target: { value: "  Development  " } });
    fireEvent.submit(input.closest("form")!);
    expect(screen.getByRole("button", { name: "Switch workspace, current: Development" })).toBeTruthy();
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select keep_draft");
    expect(screen.getByRole("button", { name: /Alpha sqlite/ })).toBeTruthy();
    cleanup(); render(<App />);
    expect(screen.getByRole("button", { name: "Switch workspace, current: Development" })).toBeTruthy();
    await screen.findByRole("button", { name: /Alpha sqlite/ });
  });
  it("creates isolated workspaces, restores drafts on switching, and persists the active workspace", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select unsaved_draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Create workspace" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Workspace name" }), { target: { value: "Analytics" } });
    fireEvent.submit(screen.getByRole("textbox", { name: "Workspace name" }).closest("form")!);
    expect(screen.getByRole("button", { name: "Switch workspace, current: Analytics" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Alpha sqlite/ })).toBeNull();
    expect(screen.queryByRole("textbox", { name: "SQL editor" })).toBeNull();
    expect(screen.getByText("Your workspace is empty.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Workspace 2 data sources" }));
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select unsaved_draft");
    fireEvent.click(screen.getByRole("button", { name: "Analytics 0 data sources" }));
    cleanup();
    render(<App />);
    await waitFor(() => expect(api.bootstrap).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("button", { name: "Switch workspace, current: Analytics" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Alpha sqlite/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Workspace 2 data sources" }));
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select unsaved_draft");
  });
  it("restores open consoles, the active tab, and unexecuted SQL drafts after reopening", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "SELECT first_unsaved_draft;" } });
    await addConsole();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "SELECT latest_keystroke;" } });
    cleanup();
    render(<App />);
    await screen.findByRole("button", { name: /Alpha sqlite/ });
    expect(tab("Console 2").getAttribute("aria-selected")).toBe("true");
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("SELECT latest_keystroke;");
    expect(api.runQuery).not.toHaveBeenCalled();
    fireEvent.click(tab("Console 1"));
    await waitFor(() => expect(tab("Console 1").getAttribute("aria-selected")).toBe("true"));
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("SELECT first_unsaved_draft;");
    await waitFor(() => expect(screen.getByRole("button", { name: /^Run/ }).hasAttribute("disabled")).toBe(false));
    await addConsole();
    expect(tab("Console 3")).toBeTruthy();
  });

  it("edits a source from Properties and keeps renamed console drafts after reopening", async () => {
    installMock();
    const actual = await vi.importActual<typeof import("./api")>("./api");
    const defaults = await actual.api.formDefaults();
    vi.mocked(api.editSource).mockResolvedValue({ ...defaults, draft: { ...defaults.draft, name: "Alpha", driver: "sqlite", dialect: "sqlite", database: "/tmp/example.sqlite", auth: "no-auth" }, has_saved_password: false, has_saved_key_password: false });
    vi.mocked(api.drivers).mockResolvedValue(await actual.api.drivers());
    vi.mocked(api.driverSettings).mockResolvedValue(await actual.api.driverSettings());
    const renamedSources = sources.map(source => source.name === "Alpha" ? { ...source, name: "Renamed" } : source);
    vi.mocked(api.updateSource).mockResolvedValue({ name: "Renamed", sources: renamedSources });
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "SELECT keep_my_draft;" } });
    fireEvent.pointerDown(screen.getByRole("button", { name: "Actions for Alpha" }), { button: 0, ctrlKey: false, pointerType: "mouse" });
    const menu = await screen.findByRole("menu", { name: "Actions for Alpha" });
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Properties…" }));
    const editor = await screen.findByRole("dialog", { name: "Connection properties" });
    fireEvent.change(await within(editor).findByLabelText("Name"), { target: { value: "Renamed" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.updateSource).toHaveBeenCalledWith("Alpha", expect.objectContaining({ name: "Renamed" }));
    expect(tab("Console 1").textContent).toContain("Renamed");
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("SELECT keep_my_draft;");
    expect(screen.getByRole("button", { name: /^Connect$/ })).toBeTruthy();
    expect(api.runQuery).not.toHaveBeenCalled();
    vi.mocked(api.bootstrap).mockResolvedValue({ sources: renamedSources, startup_error: null });
    cleanup(); render(<App />);
    await screen.findByRole("button", { name: /Renamed sqlite/ });
    expect(tab("Console 1").textContent).toContain("Renamed");
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("SELECT keep_my_draft;");
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    await waitFor(() => expect(api.connect).toHaveBeenLastCalledWith("Renamed"));
  });

  it("remembers tab closures when reopening", async () => {
    await start();
    await addConsole();
    fireEvent.click(within(document.querySelector<HTMLElement>(".workspace-tabbar")!).getByRole("button", { name: "Close Console 1 for Alpha" }));
    await waitFor(() => expect(within(screen.getByRole("tablist", { name: "Open workspace documents" })).queryByRole("tab", { name: /Console 1/ })).toBeNull());
    cleanup();
    render(<App />);
    await screen.findByRole("button", { name: /Alpha sqlite/ });
    expect(tab("Console 2").getAttribute("aria-selected")).toBe("true");
    expect(within(screen.getByRole("tablist", { name: "Open workspace documents" })).queryByRole("tab", { name: /Console 1/ })).toBeNull();
  });

  it("rejects blank and duplicate workspace names", async () => {
    render(<App />);
    await screen.findByRole("button", { name: /Alpha sqlite/ });
    fireEvent.click(screen.getByRole("button", { name: "Create workspace" }));
    const input = screen.getByRole("textbox", { name: "Workspace name" });
    expect(screen.getByRole("button", { name: "Create workspace" }).hasAttribute("disabled")).toBe(true);
    fireEvent.change(input, { target: { value: " workspace " } });
    expect(screen.getByRole("alert").textContent).toBe("Choose a different workspace name.");
    expect(screen.getByRole("button", { name: "Create workspace" }).hasAttribute("disabled")).toBe(true);
  });
  it("keeps independent console drafts and results when changing tabs", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select 11" } });
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    await waitFor(() => expect(within(activePanel()).getByLabelText("Query output").textContent).toContain("select 11"));
    await addConsole();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select 22" } });
    fireEvent.click(screen.getByRole("button", { name: /^Run/ }));
    await waitFor(() => expect(within(activePanel()).getByLabelText("Query output").textContent).toContain("select 22"));
    fireEvent.click(tab("Console 1"));
    await waitFor(() => expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select 11"));
    expect(within(activePanel()).getByLabelText("Query output").textContent).toContain("select 11");
    fireEvent.click(within(document.querySelector<HTMLElement>(".workspace-tabbar")!).getByRole("button", { name: "Close Console 2 for Alpha" }));
    expect(tab("Console 1").getAttribute("aria-selected")).toBe("true");
  });
  it("shows connection names in tabs and preserves SQL when switching between connections", async () => {
    await start();
    fireEvent.change(screen.getByRole("textbox", { name: "SQL editor" }), { target: { value: "select 33" } });
    expect(tab("Console 1").textContent).toContain("Alpha");
    expect(screen.queryByRole("combobox", { name: /Database for/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Beta sqlite/ }));
    await waitFor(() => expect(api.connect).toHaveBeenLastCalledWith("Beta"));
    fireEvent.click(within(screen.getByRole("navigation", { name: "Beta views" })).getByRole("button", { name: "SQL console" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /^Run/ }).hasAttribute("disabled")).toBe(false));
    const tabs = within(screen.getByRole("tablist", { name: "Open workspace documents" })).getAllByRole("tab", { name: /Console 1/ });
    expect(tabs.map((item) => item.textContent)).toEqual(["Console 1Alpha", "Console 1Beta"]);
    fireEvent.click(tabs[0]!);
    await waitFor(() => expect(api.connect).toHaveBeenLastCalledWith("Alpha"));
    expect((screen.getByRole("textbox", { name: "SQL editor" }) as HTMLTextAreaElement).value).toBe("select 33");
  });
  it("retries a table preview that initially failed to generate SQL", async () => {
    await start();
    vi.mocked(api.previewStatement).mockRejectedValueOnce(new Error("Schema temporarily unavailable"));
    const details = document.querySelector<HTMLDetailsElement>(".source-tables")!;
    details.open = true;
    fireEvent.click(screen.getByRole("button", { name: "Preview main.people" }));
    await within(activePanel()).findByText("Schema temporarily unavailable");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(api.previewStatement).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(within(activePanel()).getByLabelText("Query output").textContent).toContain("people"));
    expect(screen.queryByRole("textbox", { name: "SQL editor" })).toBeNull();
  });
});
