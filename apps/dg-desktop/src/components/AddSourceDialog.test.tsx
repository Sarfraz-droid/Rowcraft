// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../api";
import { installMock } from "../mock";
import { AddSourceDialog } from "./AddSourceDialog";
import { chooseOption } from "../test-utils/select";

beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value: function (this: HTMLDialogElement) { this.open = true; } });
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  installMock();
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("uploads multiple JARs, retains manual paths, and saves the imported paths", async () => {
  const upload = vi.spyOn(api, "importJdbcJar")
    .mockResolvedValueOnce("/app/jdbc-imports/driver.jar")
    .mockResolvedValueOnce("/app/jdbc-imports/dependency.jar");
  const save = vi.spyOn(api, "saveDriver").mockResolvedValue([]);
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Add Custom Driver…" }));
  fireEvent.change(screen.getByLabelText("Driver files"), { target: { value: "/manual/existing.jar\n" } });
  const files = [new File(["jar"], "driver.jar"), new File(["jar"], "dependency.jar")];
  fireEvent.change(screen.getByLabelText("Upload JDBC driver files"), { target: { files } });
  await screen.findByText(/2 JAR files uploaded/);
  expect(upload).toHaveBeenNthCalledWith(1, files[0]);
  expect(upload).toHaveBeenNthCalledWith(2, files[1]);
  fireEvent.click(screen.getByRole("button", { name: "Save Driver" }));
  await waitFor(() => expect(save).toHaveBeenCalledWith(expect.objectContaining({
    jdbc_jars: ["/manual/existing.jar", "/app/jdbc-imports/driver.jar", "/app/jdbc-imports/dependency.jar"],
  })));
});

it("keeps successful uploads when a later JAR fails and allows retrying", async () => {
  vi.spyOn(api, "importJdbcJar")
    .mockResolvedValueOnce("/app/jdbc-imports/driver.jar")
    .mockRejectedValueOnce(new Error("This file has no ZIP/JAR header."));
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Add Custom Driver…" }));
  fireEvent.change(screen.getByLabelText("Upload JDBC driver files"), {
    target: { files: [new File(["jar"], "driver.jar"), new File(["invalid"], "bad.jar")] },
  });
  await screen.findAllByText(/This file has no ZIP\/JAR header.*1 uploaded file remains/);
  expect((screen.getByLabelText("Driver files") as HTMLTextAreaElement).value).toBe("/app/jdbc-imports/driver.jar");
  expect((screen.getByRole("button", { name: "Upload JAR files…" }) as HTMLButtonElement).disabled).toBe(false);
});

it("sends the entered password, picked color, and selected connection type when testing and saving", async () => {
  const test = vi.spyOn(api, "testConnection").mockResolvedValue(null);
  const add = vi.spyOn(api, "addSource").mockResolvedValue({ name: "Example", sources: [] });
  const onAdded = vi.fn();
  render(<AddSourceDialog onClose={vi.fn()} onAdded={onAdded} />);
  const password = await screen.findByLabelText("Password");
  expect(password.getAttribute("type")).toBe("password");
  expect(screen.queryByLabelText(/Password environment variable/)).toBeNull();
  fireEvent.change(password, { target: { value: " demo-only password " } });
  const color = screen.getByLabelText("Color");
  expect(color.getAttribute("type")).toBe("color");
  fireEvent.change(color, { target: { value: "#123456" } });
  await chooseOption(screen.getByRole("combobox", { name: "Connection type" }), "Production");
  fireEvent.click(screen.getByRole("button", { name: "Test Connection" }));
  await waitFor(() => expect(test).toHaveBeenCalledWith(expect.objectContaining({ password: " demo-only password ", password_env: "", color: "#123456", connection_type: "production" })));
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  await waitFor(() => expect(add).toHaveBeenCalledWith(expect.objectContaining({ password: " demo-only password ", password_env: "", color: "#123456", connection_type: "production" })));
  await waitFor(() => expect(onAdded).toHaveBeenCalledOnce());
});

it("can reset the color and hides password entry for no authentication", async () => {
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  await screen.findByLabelText("Password");
  fireEvent.change(screen.getByLabelText("Color"), { target: { value: "#123456" } });
  fireEvent.click(screen.getByRole("button", { name: "Clear color" }));
  expect(screen.getByText("Default", { selector: ".source-color-control .note" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Clear color" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("radio", { name: "No auth" }));
  expect(screen.queryByLabelText("Password")).toBeNull();
});

it("accepts direct SSH passwords, key passphrases, and SSL key passwords without environment fields", async () => {
  const test = vi.spyOn(api, "testConnection").mockResolvedValue(null);
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  await screen.findByLabelText("Password");
  fireEvent.click(screen.getByRole("tab", { name: "SSH/SSL" }));
  fireEvent.change(screen.getByLabelText("Client key password"), { target: { value: "ssl-demo-only" } });
  fireEvent.click(screen.getByRole("checkbox", { name: "Use SSH tunnel" }));
  fireEvent.click(screen.getByRole("button", { name: "New…" }));
  const password = screen.getByLabelText("SSH password");
  expect(password.getAttribute("type")).toBe("password");
  fireEvent.change(password, { target: { value: "ssh-demo-only" } });
  fireEvent.click(screen.getByRole("radio", { name: "Key pair" }));
  const passphrase = screen.getByLabelText("Key passphrase");
  expect(passphrase.getAttribute("type")).toBe("password");
  fireEvent.change(passphrase, { target: { value: "key-demo-only" } });
  expect(screen.queryByText(/environment variable/i)).toBeNull();
  fireEvent.click(screen.getByRole("tab", { name: "General" }));
  fireEvent.click(screen.getByRole("button", { name: "Test Connection" }));
  await waitFor(() => expect(test).toHaveBeenCalledWith(expect.objectContaining({ ssh_password: "ssh-demo-only", ssh_passphrase: "key-demo-only", key_password: "ssl-demo-only", ssh_password_env: "", ssh_passphrase_env: "", key_password_env: "" })));
});

it("updates the generated URL with connection fields and supports a custom override", async () => {
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  await screen.findByLabelText("Password");
  fireEvent.change(screen.getByLabelText("Host"), { target: { value: "db.example.test" } });
  fireEvent.change(screen.getByLabelText("Database"), { target: { value: "my app" } });
  fireEvent.change(screen.getByLabelText("User"), { target: { value: "demo@team" } });
  const url = screen.getByLabelText("URL") as HTMLInputElement;
  expect(url.value).toBe("postgresql://demo%40team@db.example.test:5432/my%20app");
  expect(url.readOnly).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Edit URL" }));
  fireEvent.change(url, { target: { value: "postgresql://custom:5432/data" } });
  fireEvent.change(screen.getByLabelText("Host"), { target: { value: "another.example.test" } });
  expect(url.value).toBe("postgresql://custom:5432/data");
  fireEvent.click(screen.getByRole("button", { name: "Use generated URL" }));
  expect(url.value).toContain("another.example.test:5432");
});

it("saves SSH independently and reuses the saved configuration for the database", async () => {
  const save = vi.spyOn(api, "saveSshConfig").mockResolvedValue(["bastion", "Staging"]);
  const add = vi.spyOn(api, "addSource").mockResolvedValue({ name: "Example", sources: [] });
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  await screen.findByLabelText("Password");
  fireEvent.click(screen.getByRole("tab", { name: "SSH/SSL" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Use SSH tunnel" }));
  fireEvent.click(screen.getByRole("button", { name: "New…" }));
  fireEvent.change(screen.getByLabelText("Configuration name"), { target: { value: "Staging" } });
  fireEvent.change(screen.getByLabelText("SSH host"), { target: { value: "ssh.example.test" } });
  fireEvent.change(screen.getByLabelText("SSH password"), { target: { value: "demo-only" } });
  fireEvent.click(screen.getByRole("button", { name: "Save SSH configuration" }));
  await waitFor(() => expect(screen.getByRole("combobox", { name: "SSH configuration" }).textContent).toBe("Staging"));
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ ssh_new_name: "Staging", ssh_password: "demo-only" }));
  expect(add).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  await waitFor(() => expect(add).toHaveBeenCalledWith(expect.objectContaining({ ssh_new: false, ssh_existing: "Staging", ssh_password: "" })));
});

it("edits an existing source using the full add-source form and preserves saved credentials", async () => {
  const defaults = await api.formDefaults();
  vi.spyOn(api, "editSource").mockResolvedValue({ ...defaults, draft: { ...defaults.draft, name: "Production", driver: "mysql", dialect: "mysql", host: "original.example", user: "reader", use_ssh: true, ssh_existing: "bastion" }, has_saved_password: true, has_saved_key_password: true });
  const update = vi.spyOn(api, "updateSource").mockResolvedValue({ name: "Reporting", sources: [] });
  const add = vi.spyOn(api, "addSource");
  const test = vi.spyOn(api, "testConnection").mockResolvedValue(null);
  const saved = vi.fn();
  render(<AddSourceDialog sourceName="Production" onClose={vi.fn()} onAdded={saved} />);
  await screen.findByRole("dialog", { name: "Connection properties" });
  const host = await screen.findByLabelText("Host");
  expect((host as HTMLInputElement).value).toBe("original.example");
  const password = screen.getByLabelText("Password") as HTMLInputElement;
  expect(password.value).toBe("");
  expect(password.placeholder).toContain("leave blank to keep");
  for (const name of ["General", "Options", "SSH/SSL", "Schemas", "Advanced"]) expect(screen.getByRole("tab", { name })).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Reporting" } });
  fireEvent.change(host, { target: { value: "updated.example" } });
  fireEvent.click(screen.getByRole("button", { name: "Test Connection" }));
  await waitFor(() => expect(test).toHaveBeenCalledWith(expect.objectContaining({ name: "Reporting", host: "updated.example", password: "" }), "Production"));
  await screen.findByText("Succeeded");
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(update).toHaveBeenCalledWith("Production", expect.objectContaining({ name: "Reporting", host: "updated.example", password: "", ssh_existing: "bastion" })));
  expect(add).not.toHaveBeenCalled();
  expect(saved).toHaveBeenCalledWith({ name: "Reporting", sources: [] });
});

it("keeps edits visible when saving an existing source fails", async () => {
  const defaults = await api.formDefaults();
  vi.spyOn(api, "editSource").mockResolvedValue({ ...defaults, draft: { ...defaults.draft, name: "Production" }, has_saved_password: true, has_saved_key_password: false });
  vi.spyOn(api, "updateSource").mockRejectedValue(new Error("A data source with this name already exists."));
  const saved = vi.fn();
  render(<AddSourceDialog sourceName="Production" onClose={vi.fn()} onAdded={saved} />);
  fireEvent.change(await screen.findByLabelText("Name"), { target: { value: "Duplicate" } });
  fireEvent.click(screen.getByRole("checkbox", { name: "Remove saved password" }));
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await screen.findByText(/A data source with this name already exists/);
  expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Duplicate");
  expect(api.updateSource).toHaveBeenCalledWith("Production", expect.objectContaining({ clear_password: true }));
  expect(saved).not.toHaveBeenCalled();
});

it("fills connection fields from a pasted URL and moves its password into the password field", async () => {
  const test = vi.spyOn(api, "testConnection").mockResolvedValue(null);
  render(<AddSourceDialog onClose={vi.fn()} onAdded={vi.fn()} />);
  await screen.findByLabelText("Password");
  fireEvent.click(screen.getByRole("button", { name: "Edit URL" }));
  fireEvent.change(screen.getByLabelText("URL"), { target: { value: "postgresql://demo%40team:demo%3Aonly@db.example.test:5544/my%20app?sslmode=require&application_name=rowcraft" } });
  expect((screen.getByLabelText("Host") as HTMLInputElement).value).toBe("db.example.test");
  expect((screen.getByLabelText("Port") as HTMLInputElement).value).toBe("5544");
  expect((screen.getByLabelText("Database") as HTMLInputElement).value).toBe("my app");
  expect((screen.getByLabelText("User") as HTMLInputElement).value).toBe("demo@team");
  expect((screen.getByLabelText("Password") as HTMLInputElement).value).toBe("demo:only");
  expect((screen.getByLabelText("URL") as HTMLInputElement).value).not.toContain("demo%3Aonly");
  fireEvent.click(screen.getByRole("button", { name: "Test Connection" }));
  await waitFor(() => expect(test).toHaveBeenCalledWith(expect.objectContaining({ host: "db.example.test", port: "5544", database: "my app", user: "demo@team", password: "demo:only", password_env: "", ssl_mode: "require", properties: expect.stringContaining("application_name=rowcraft") })));
});

it("autofills MongoDB URL fields and copies auth options to Advanced", async () => {
  const add = vi.spyOn(api, "addSource").mockResolvedValue({ name: "Mongo example", sources: [] });
  render(<AddSourceDialog initialDriver="mongodb" onClose={vi.fn()} onAdded={vi.fn()} />);
  await screen.findByLabelText("Password");
  fireEvent.click(screen.getByRole("radio", { name: "No auth" }));
  fireEvent.click(screen.getByRole("button", { name: "Edit URL" }));
  const uri = "mongodb://reader:demo%40only@db.example.test:27017/community?authMechanism=DEFAULT&authSource=community";
  fireEvent.change(screen.getByLabelText("URL"), { target: { value: uri } });
  expect((screen.getByLabelText("Host") as HTMLInputElement).value).toBe("db.example.test");
  expect((screen.getByLabelText("Port") as HTMLInputElement).value).toBe("27017");
  expect((screen.getByLabelText("Database") as HTMLInputElement).value).toBe("community");
  expect((screen.getByLabelText("User") as HTMLInputElement).value).toBe("reader");
  expect((screen.getByLabelText("Password") as HTMLInputElement).value).toBe("demo@only");
  expect(screen.getByRole("radio", { name: "User & Password" }).getAttribute("aria-checked")).toBe("true");
  fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
  expect((screen.getByLabelText("Driver properties") as HTMLTextAreaElement).value).toBe("authMechanism=DEFAULT\nauthSource=community");
  fireEvent.click(screen.getByRole("tab", { name: "General" }));
  fireEvent.change(screen.getByLabelText("URL"), { target: { value: "mongodb://db.example.test:" } });
  expect((screen.getByLabelText("Host") as HTMLInputElement).value).toBe("db.example.test");
  expect((screen.getByLabelText("Database") as HTMLInputElement).value).toBe("community");
  fireEvent.change(screen.getByLabelText("URL"), { target: { value: uri } });
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  await waitFor(() => expect(add).toHaveBeenCalledWith(expect.objectContaining({ dialect: "mongo", host: "db.example.test", port: "27017", database: "community", user: "reader", password: "demo@only", url: "mongodb://reader@db.example.test:27017/community?authMechanism=DEFAULT&authSource=community" })));
});
