export interface Workspace {
  id: string;
  name: string;
  sources: string[];
}
export interface WorkspaceSettings {
  version: 1;
  activeId: string;
  workspaces: Workspace[];
}
export const WORKSPACES_KEY = "rowcraft.workspaces.v1";

export function loadWorkspaces(): WorkspaceSettings {
  try {
    const saved = JSON.parse(localStorage.getItem(WORKSPACES_KEY) ?? "null");
    if (saved?.version === 1 && Array.isArray(saved.workspaces) && saved.workspaces.length
      && saved.workspaces.every((item: Workspace) => item && typeof item.id === "string" && item.id && typeof item.name === "string" && item.name.trim() && Array.isArray(item.sources) && item.sources.every((name) => typeof name === "string"))
      && new Set(saved.workspaces.map((item: Workspace) => item.id)).size === saved.workspaces.length) {
      return { ...saved, activeId: saved.workspaces.some((item: Workspace) => item.id === saved.activeId) ? saved.activeId : saved.workspaces[0].id };
    }
  } catch { /* Start with the default workspace if storage is unavailable or invalid. */ }
  return { version: 1, activeId: "default", workspaces: [{ id: "default", name: "Workspace", sources: [] }] };
}

// Existing and externally added connections remain discoverable in the first workspace.
export function reconcileWorkspaceSources(settings: WorkspaceSettings, names: string[]): WorkspaceSettings {
  const assigned = new Set(settings.workspaces.flatMap((workspace) => workspace.sources));
  const available = new Set(names);
  return { ...settings, workspaces: settings.workspaces.map((workspace, index) => ({
    ...workspace,
    sources: [...workspace.sources.filter((name) => available.has(name)), ...(index === 0 ? names.filter((name) => !assigned.has(name)) : [])],
  })) };
}
