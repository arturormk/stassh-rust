import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import {
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Folder,
  Maximize2,
  Minimize2,
  Monitor,
  PanelRightClose,
  PanelRightOpen,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Search,
  TerminalSquare,
  Trash2,
  X,
} from "lucide-react";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

type Id = string;

type Forward =
  | {
      type: "local";
      bind_address: string;
      local_port: number;
      destination_host: string;
      destination_port: number;
    }
  | {
      type: "remote";
      bind_address: string;
      remote_port: number;
      destination_host: string;
      destination_port: number;
    }
  | {
      type: "dynamic";
      bind_address: string;
      local_port: number;
    };

type FolderView = {
  id: Id;
  parentId: Id | null;
  name: string;
  path: string;
  hostCount: number;
};

type HostView = {
  id: Id;
  folderId: Id;
  path: string;
  displayName: string;
  hostname: string;
  port: number;
  username: string | null;
  identityFingerprint: string | null;
  secrets: string | null;
  jumpChain: Id[];
  forwards: Forward[];
  tags: string[];
  notes: string | null;
  actionCount: number;
};

type IdentityView = {
  fingerprint: string;
  path: string;
  preferredName: string | null;
  exists: boolean;
};

type DiagnosticView = {
  severity: "warning" | "error";
  message: string;
  hostId: Id | null;
};

type WorkspaceSnapshot = {
  vaultPath: string;
  localConfigPath: string;
  secretsPath: string;
  folders: FolderView[];
  hosts: HostView[];
  identities: IdentityView[];
  secretsAvailable: boolean;
  diagnostics: DiagnosticView[];
};

type SearchResult = {
  id: Id;
  path: string;
  target: string;
  username: string | null;
  tags: string[];
};

type HostDetails = {
  host: HostView;
  jumps: Array<{
    id: Id;
    displayName: string;
    hostname: string;
    port: number;
    username: string | null;
  }>;
  sshCommand: string;
  diagnostics: DiagnosticView[];
};

type Tab =
  | { type: "terminal"; id: Id; sessionId: Id; hostId: Id; title: string; status: string }
  | {
      type: "layout";
      id: Id;
      title: string;
      sessionIds: Id[];
      activeSessionId: Id | null;
      mode: LayoutMode;
      mainRatio: number;
      broadcastInput: boolean;
    };

type Selection = { type: "host"; id: Id } | { type: "folder"; id: Id };
type EditorMode = "host" | "folder" | "new-host" | "new-folder" | null;
type LayoutMode = "grid" | "main";
type InspectorSource = "details" | "terminal" | "layout";

type InspectorTarget =
  | { type: "host"; source: InspectorSource; host: HostView; details: HostDetails | null; terminal: Extract<Tab, { type: "terminal" }> | null }
  | { type: "folder"; source: "details"; folder: FolderView }
  | { type: "layout"; source: "layout"; layout: Extract<Tab, { type: "layout" }> }
  | null;

type HostForm = {
  folderId: Id;
  displayName: string;
  hostname: string;
  port: string;
  username: string;
  identityFingerprint: string;
  secrets: string;
  jumpChainText: string;
  forwards: Forward[];
  tags: string;
  notes: string;
};

type FolderForm = {
  parentId: Id;
  name: string;
};

const defaultSidebarWidth = 320;
const minSidebarWidth = 240;
const maxSidebarWidth = 560;
const minMainRatio = 0.35;
const maxMainRatio = 0.75;

function clampSidebarWidth(width: number) {
  return Math.min(maxSidebarWidth, Math.max(minSidebarWidth, width));
}

function App() {
  const [workspace, setWorkspace] = useState<WorkspaceSnapshot | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [details, setDetails] = useState<HostDetails | null>(null);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [expanded, setExpanded] = useState<Set<Id>>(new Set());
  const [checkedHosts, setCheckedHosts] = useState<Set<Id>>(new Set());
  const [draggingHostIds, setDraggingHostIds] = useState<Id[]>([]);
  const [dropTargetFolderId, setDropTargetFolderId] = useState<Id | null>(null);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [terminalOrder, setTerminalOrder] = useState<Id[]>([]);
  const [activeTabId, setActiveTabId] = useState<Id | null>(null);
  const [draggingTabId, setDraggingTabId] = useState<Id | null>(null);
  const [tabDropTargetId, setTabDropTargetId] = useState<Id | null>(null);
  const [tabAddTargetId, setTabAddTargetId] = useState<Id | null>(null);
  const [fullscreenSessionId, setFullscreenSessionId] = useState<Id | null>(null);
  const [editorMode, setEditorMode] = useState<EditorMode>(null);
  const [editingHostId, setEditingHostId] = useState<Id | null>(null);
  const [hostForm, setHostForm] = useState<HostForm | null>(null);
  const [folderForm, setFolderForm] = useState<FolderForm | null>(null);
  const [status, setStatus] = useState("Loading workspace");
  const [error, setError] = useState<string | null>(null);
  const [sidebarWidth, setSidebarWidth] = useState(defaultSidebarWidth);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const sidebarResize = useRef<{ pointerId: number; startX: number; startWidth: number } | null>(null);
  const activeTab = activeTabId ? tabs.find((tab) => tab.id === activeTabId) ?? null : null;

  useEffect(() => {
    loadWorkspace();
  }, []);

  useEffect(() => {
    if (!workspace) return;
    const roots = workspace.folders
      .filter((folder) => folder.parentId === null)
      .map((folder) => folder.id);
    setExpanded(new Set(roots));
  }, [workspace?.vaultPath]);

  useEffect(() => {
    if (tabs.length && (!activeTabId || !tabs.some((tab) => tab.id === activeTabId))) {
      setActiveTabId(tabs[0].id);
    }
  }, [activeTabId, tabs]);

  const inspectorHostId = useMemo(() => {
    if (!activeTab) return selection?.type === "host" ? selection.id : null;
    if (activeTab.type === "terminal") return activeTab.hostId;
    if (activeTab.type === "layout") {
      const activeSessionId = activeTab.activeSessionId ?? activeTab.sessionIds[0];
      return (
        tabs.find(
          (tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal" && tab.sessionId === activeSessionId,
        )?.hostId ?? null
      );
    }
    return null;
  }, [activeTab, selection, tabs]);

  useEffect(() => {
    if (!workspace || !inspectorHostId) {
      setDetails(null);
      return;
    }
    invoke<HostDetails>("host_details", { hostId: inspectorHostId })
      .then(setDetails)
      .catch((err) => setStatus(String(err)));
  }, [workspace, inspectorHostId]);

  useEffect(() => {
    if (editorMode) {
      setInspectorCollapsed(false);
      window.requestAnimationFrame(() => window.dispatchEvent(new Event("resize")));
    }
  }, [editorMode]);

  useEffect(() => {
    if (!workspace) return;
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }
    const handle = window.setTimeout(() => {
      invoke<SearchResult[]>("search_hosts", { query })
        .then(setSearchResults)
        .catch((err) => setStatus(String(err)));
    }, 80);
    return () => window.clearTimeout(handle);
  }, [query, workspace]);

  useEffect(() => {
    const unlistenOutput = listen<{ sessionId: Id; data: string }>("session-output", (event) => {
      window.dispatchEvent(
        new CustomEvent(`terminal-data:${event.payload.sessionId}`, {
          detail: event.payload.data,
        }),
      );
    });
    const unlistenExit = listen<{ sessionId: Id; message: string }>("session-exit", (event) => {
      setFullscreenSessionId((current) => (current === event.payload.sessionId ? null : current));
      setTabs((current) =>
        current.map((tab) =>
          tab.type === "terminal" && tab.sessionId === event.payload.sessionId
            ? { ...tab, status: event.payload.message }
            : tab,
        ),
      );
    });
    return () => {
      unlistenOutput.then((fn) => fn());
      unlistenExit.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!fullscreenSessionId) return;
    function exitFullscreen(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        setFullscreenSessionId(null);
        window.dispatchEvent(new Event("resize"));
      }
    }
    window.addEventListener("keydown", exitFullscreen);
    const frame = window.requestAnimationFrame(() => window.dispatchEvent(new Event("resize")));
    return () => {
      window.removeEventListener("keydown", exitFullscreen);
      window.cancelAnimationFrame(frame);
    };
  }, [fullscreenSessionId]);

  const selectedHost =
    selection?.type === "host" ? workspace?.hosts.find((host) => host.id === selection.id) ?? null : null;
  const selectedFolder =
    selection?.type === "folder"
      ? workspace?.folders.find((folder) => folder.id === selection.id) ?? null
      : null;
  const rootFolder = workspace?.folders.find((folder) => folder.parentId === null) ?? null;
  const openSessionCounts = useMemo(() => {
    const counts = new Map<Id, number>();
    for (const tab of tabs) {
      if (tab.type === "terminal") counts.set(tab.hostId, (counts.get(tab.hostId) ?? 0) + 1);
    }
    return counts;
  }, [tabs]);

  async function loadWorkspace() {
    try {
      setError(null);
      const snapshot = await invoke<WorkspaceSnapshot>("load_workspace");
      setWorkspace(snapshot);
      setStatus("Workspace loaded");
    } catch (err) {
      setError(String(err));
      setStatus("Workspace failed to load");
    }
  }

  async function reloadWorkspace() {
    try {
      const snapshot = await invoke<WorkspaceSnapshot>("reload_workspace");
      setWorkspace(snapshot);
      setStatus("Workspace reloaded");
    } catch (err) {
      setStatus(String(err));
    }
  }

  async function saveHost() {
    if (!hostForm || !workspace) return;
    const payload = {
      folderId: hostForm.folderId,
      displayName: hostForm.displayName.trim(),
      hostname: hostForm.hostname.trim(),
      port: Number(hostForm.port || "22"),
      username: blank(hostForm.username),
      identityFingerprint: blank(hostForm.identityFingerprint),
      secrets: blank(hostForm.secrets),
      jumpChain: hostForm.jumpChainText
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean),
      forwards: hostForm.forwards,
      tags: hostForm.tags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean),
      notes: blank(hostForm.notes),
    };
    try {
      const snapshot =
        editorMode === "new-host"
          ? await invoke<WorkspaceSnapshot>("create_host", { input: payload })
          : await invoke<WorkspaceSnapshot>("update_host", { hostId: editingHostId, input: payload });
      setWorkspace(snapshot);
      setEditorMode(null);
      setEditingHostId(null);
      setStatus("Host saved");
    } catch (err) {
      setStatus(String(err));
    }
  }

  async function saveFolder() {
    if (!folderForm) return;
    const folderId = selection?.id;
    if (editorMode !== "new-folder" && !folderId) return;
    try {
      const snapshot =
        editorMode === "new-folder"
          ? await invoke<WorkspaceSnapshot>("create_folder", { input: folderForm })
          : await invoke<WorkspaceSnapshot>("rename_folder", {
              folderId,
              name: folderForm.name.trim(),
            });
      setWorkspace(snapshot);
      setEditorMode(null);
      setEditingHostId(null);
      setStatus("Folder saved");
    } catch (err) {
      setStatus(String(err));
    }
  }

  async function openTerminal(host: HostView) {
    try {
      const sessionId = await invoke<Id>("start_ssh_session", {
        hostId: host.id,
        cols: 100,
        rows: 28,
      });
      const tab: Tab = {
        type: "terminal",
        id: sessionId,
        sessionId,
        hostId: host.id,
        title: host.displayName,
        status: "running",
      };
      setTabs((current) => [...current, tab]);
      setTerminalOrder((current) => [...current, sessionId]);
      setActiveTabId(sessionId);
      setStatus(`Connected: ${host.path}`);
    } catch (err) {
      setStatus(String(err));
    }
  }

  async function openCheckedHosts() {
    if (!workspace || !checkedHosts.size) return;
    const hostIds = Array.from(checkedHosts);
    const hosts = hostIds
      .map((hostId) => workspace.hosts.find((host) => host.id === hostId))
      .filter((host): host is HostView => Boolean(host));
    for (const host of hosts) {
      await openTerminal(host);
    }
    setCheckedHosts(new Set());
  }

  async function closeTab(tab: Tab) {
    if (tab.type === "terminal") {
      if (tab.status === "running" && !window.confirm(`Close connected terminal ${tab.title}?`)) {
        return;
      }
      setTerminalOrder((current) => current.filter((sessionId) => sessionId !== tab.sessionId));
      setFullscreenSessionId((current) => (current === tab.sessionId ? null : current));
      const nextTabs = tabs
        .filter((item) => item.id !== tab.id)
        .map((item) =>
          item.type === "layout"
            ? {
                ...item,
                sessionIds: item.sessionIds.filter((sessionId) => sessionId !== tab.sessionId),
                activeSessionId:
                  item.activeSessionId === tab.sessionId
                    ? item.sessionIds.find((sessionId) => sessionId !== tab.sessionId) ?? null
                    : item.activeSessionId,
              }
            : item,
        );
      const nextActiveTabId =
        activeTabId === tab.id || !nextTabs.some((item) => item.id === activeTabId)
          ? nextTabs[0]?.id ?? null
          : activeTabId;
      setTabs(nextTabs);
      setActiveTabId(nextActiveTabId);
      if (!nextActiveTabId) {
        setSelection((current) => (current?.type === "host" && current.id === tab.hostId ? null : current));
      }
      setDetails((current) => (current?.host.id === tab.hostId ? null : current));
      invoke("close_session", { sessionId: tab.sessionId }).catch(() => undefined);
    } else {
      const next = tabs.filter((item) => item.id !== tab.id);
      setTabs(next);
      if (activeTabId === tab.id || !next.some((item) => item.id === activeTabId)) {
        setActiveTabId(next[0]?.id ?? null);
      }
    }
    window.requestAnimationFrame(() => window.dispatchEvent(new Event("resize")));
  }

  function createLayoutTab() {
    const terminalTabs = tabs.filter((tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal");
    if (!terminalTabs.length) return;
    const id = `layout-${crypto.randomUUID()}`;
    setTabs((current) => [
      ...current,
      {
        type: "layout",
        id,
        title: `Layout ${current.filter((tab) => tab.type === "layout").length + 1}`,
        sessionIds: terminalTabs.map((tab) => tab.sessionId),
        activeSessionId:
          activeTab?.type === "terminal" ? activeTab.sessionId : terminalTabs[terminalTabs.length - 1]?.sessionId ?? null,
        mode: "grid",
        mainRatio: 0.5,
        broadcastInput: false,
      },
    ]);
    setActiveTabId(id);
    setStatus(`Layout created with ${terminalTabs.length} sessions`);
  }

  function updateLayout(layoutId: Id, patch: Partial<Extract<Tab, { type: "layout" }>>) {
    setTabs((current) =>
      current.map((tab) => (tab.type === "layout" && tab.id === layoutId ? { ...tab, ...patch } : tab)),
    );
  }

  function addTerminalTabToLayout(terminalTabId: Id, layoutTabId: Id) {
    let addedMessage: string | null = null;
    setTabs((current) => {
      const terminalTab = current.find(
        (tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal" && tab.id === terminalTabId,
      );
      if (!terminalTab) return current;
      return current.map((tab) => {
        if (tab.type !== "layout" || tab.id !== layoutTabId || tab.sessionIds.includes(terminalTab.sessionId)) {
          return tab;
        }
        addedMessage = `${terminalTab.title} added to ${tab.title}`;
        return {
          ...tab,
          sessionIds: [...tab.sessionIds, terminalTab.sessionId],
          activeSessionId: terminalTab.sessionId,
        };
      });
    });
    if (addedMessage) setStatus(addedMessage);
  }

  function createLayoutFromTerminalTabs(sourceTabId: Id, targetTabId: Id) {
    if (sourceTabId === targetTabId) return;
    let layoutId: Id | null = null;
    let addedMessage: string | null = null;
    setTabs((current) => {
      const sourceTab = current.find(
        (tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal" && tab.id === sourceTabId,
      );
      const targetTab = current.find(
        (tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal" && tab.id === targetTabId,
      );
      if (!sourceTab || !targetTab) return current;
      const nextLayoutId = `layout-${crypto.randomUUID()}`;
      const orderedSessions = current
        .filter((tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal")
        .filter((tab) => tab.id === sourceTabId || tab.id === targetTabId)
        .map((tab) => tab.sessionId);
      layoutId = nextLayoutId;
      addedMessage = `Layout created with ${targetTab.title} and ${sourceTab.title}`;
      return [
        ...current,
        {
          type: "layout",
          id: nextLayoutId,
          title: `Layout ${current.filter((tab) => tab.type === "layout").length + 1}`,
          sessionIds: orderedSessions,
          activeSessionId: sourceTab.sessionId,
          mode: "grid",
          mainRatio: 0.5,
          broadcastInput: false,
        },
      ];
    });
    if (layoutId) setActiveTabId(layoutId);
    if (addedMessage) setStatus(addedMessage);
  }

  function removeSessionFromLayout(layoutId: Id, sessionId: Id) {
    setTabs((current) =>
      current.map((tab) => {
        if (tab.type !== "layout" || tab.id !== layoutId) return tab;
        const sessionIds = tab.sessionIds.filter((id) => id !== sessionId);
        return {
          ...tab,
          sessionIds,
          activeSessionId: tab.activeSessionId === sessionId ? sessionIds[0] ?? null : tab.activeSessionId,
        };
      }),
    );
  }

  function writeTerminalInput(sessionId: Id, data: string) {
    invoke("write_terminal", { sessionId, data }).catch((err) => setStatus(String(err)));
  }

  function reorderTab(sourceId: Id, targetId: Id) {
    if (sourceId === targetId) return;
    setTabs((current) => {
      const source = current.find((tab) => tab.id === sourceId);
      if (!source) return current;
      const withoutSource = current.filter((tab) => tab.id !== sourceId);
      const targetIndex = withoutSource.findIndex((tab) => tab.id === targetId);
      if (targetIndex < 0) return current;
      const insertIndex = targetIndex;
      const next = [...withoutSource];
      next.splice(insertIndex, 0, source);
      return next;
    });
  }

  async function applySnapshot(command: string, args: Record<string, unknown>, message: string) {
    try {
      const snapshot = await invoke<WorkspaceSnapshot>(command, args);
      setWorkspace(snapshot);
      setStatus(message);
    } catch (err) {
      setStatus(String(err));
    }
  }

  function startHostEditor(mode: EditorMode, host?: HostView) {
    const folderId =
      host?.folderId ?? selectedFolder?.id ?? selectedHost?.folderId ?? rootFolder?.id ?? workspace?.folders[0]?.id ?? "";
    setHostForm({
      folderId,
      displayName: host?.displayName ?? "",
      hostname: host?.hostname ?? "",
      port: String(host?.port ?? 22),
      username: host?.username ?? "",
      identityFingerprint: host?.identityFingerprint ?? "",
      secrets: host?.secrets ?? "",
      jumpChainText: host?.jumpChain.join(", ") ?? "",
      forwards: host?.forwards ?? [],
      tags: host?.tags.join(", ") ?? "",
      notes: host?.notes ?? "",
    });
    setFolderForm(null);
    setEditorMode(mode);
    setEditingHostId(mode === "host" ? host?.id ?? null : null);
  }

  function startFolderEditor(mode: EditorMode, folder?: FolderView) {
    const parentId = folder?.parentId ?? selectedFolder?.id ?? rootFolder?.id ?? "";
    setFolderForm({
      parentId,
      name: mode === "new-folder" ? "" : folder?.name ?? "",
    });
    setHostForm(null);
    setEditorMode(mode);
    setEditingHostId(null);
  }

  function cancelEditor() {
    setEditorMode(null);
    setEditingHostId(null);
  }

  function setInspectorCollapsedAndResize(collapsed: boolean) {
    setInspectorCollapsed(collapsed);
    window.requestAnimationFrame(() => window.dispatchEvent(new Event("resize")));
  }

  function updateSidebarWidth(width: number) {
    setSidebarWidth(clampSidebarWidth(width));
    window.dispatchEvent(new Event("resize"));
  }

  function startSidebarResize(event: React.PointerEvent<HTMLDivElement>) {
    event.preventDefault();
    sidebarResize.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: sidebarWidth,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function resizeSidebar(event: React.PointerEvent<HTMLDivElement>) {
    if (!sidebarResize.current || sidebarResize.current.pointerId !== event.pointerId) return;
    updateSidebarWidth(sidebarResize.current.startWidth + event.clientX - sidebarResize.current.startX);
  }

  function stopSidebarResize(event: React.PointerEvent<HTMLDivElement>) {
    if (!sidebarResize.current || sidebarResize.current.pointerId !== event.pointerId) return;
    sidebarResize.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    window.dispatchEvent(new Event("resize"));
  }

  function resizeSidebarWithKeyboard(event: React.KeyboardEvent<HTMLDivElement>) {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      updateSidebarWidth(sidebarWidth - 16);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      updateSidebarWidth(sidebarWidth + 16);
    } else if (event.key === "Home") {
      event.preventDefault();
      updateSidebarWidth(minSidebarWidth);
    } else if (event.key === "End") {
      event.preventDefault();
      updateSidebarWidth(maxSidebarWidth);
    }
  }

  if (error) {
    return (
      <main className="errorShell">
        <h1>stassh</h1>
        <p>{error}</p>
        <button onClick={loadWorkspace}>
          <RefreshCw size={16} /> Retry
        </button>
      </main>
    );
  }

  if (!workspace) {
    return <main className="loading">Loading stassh workspace</main>;
  }

  const inspectorTarget = resolveInspectorTarget({
    activeTab,
    tabs,
    workspace,
    selection,
    selectedHost,
    selectedFolder,
    details: details?.host.id === inspectorHostId ? details : null,
  });
  const canShowInspector = Boolean(editorMode) || Boolean(inspectorTarget);
  const inspectorExpanded = canShowInspector && (Boolean(editorMode) || !inspectorCollapsed);
  const inspectorWidth = inspectorExpanded ? 340 : 0;

  return (
    <div className="appShell" style={{ gridTemplateColumns: `${sidebarWidth}px 8px minmax(380px, 1fr) ${inspectorWidth}px` }}>
      <aside className="sidebar">
        <div className="sidebarHeader">
          <div className="searchBox">
            <Search size={15} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search hosts" />
          </div>
          <button title="Reload" onClick={reloadWorkspace}>
            <RefreshCw size={16} />
          </button>
        </div>
        <div className="quickActions">
          <button onClick={() => startHostEditor("new-host")}>
            <Plus size={15} /> Host
          </button>
          <button onClick={() => startFolderEditor("new-folder")}>
            <Plus size={15} /> Folder
          </button>
          <button onClick={openCheckedHosts} disabled={!checkedHosts.size}>
            <TerminalSquare size={15} /> Open
          </button>
        </div>
        {query.trim() ? (
          <SearchResults
            results={searchResults}
            selectedId={selection?.type === "host" ? selection.id : null}
            onSelect={(id) => {
              setSelection({ type: "host", id });
            }}
            onOpen={(hostId) => {
              const host = workspace.hosts.find((item) => item.id === hostId);
              if (host) openTerminal(host);
            }}
          />
        ) : (
          <InventoryTree
            workspace={workspace}
            openSessionCounts={openSessionCounts}
            expanded={expanded}
            setExpanded={setExpanded}
            selection={selection}
            setSelection={setSelection}
            checkedHosts={checkedHosts}
            setCheckedHosts={setCheckedHosts}
            draggingHostIds={draggingHostIds}
            dropTargetFolderId={dropTargetFolderId}
            setDraggingHostIds={setDraggingHostIds}
            setDropTargetFolderId={setDropTargetFolderId}
            onMoveHosts={(hostIds, folderId) =>
              applySnapshot("move_hosts", { hostIds, folderId }, "Host moved").then(() => {
                setCheckedHosts(new Set());
                setDropTargetFolderId(null);
                setDraggingHostIds([]);
              })
            }
            onOpen={openTerminal}
          />
        )}
      </aside>

      <div
        className="sidebarResizeHandle"
        role="separator"
        aria-label="Resize sidebar"
        aria-orientation="vertical"
        aria-valuemin={minSidebarWidth}
        aria-valuemax={maxSidebarWidth}
        aria-valuenow={sidebarWidth}
        tabIndex={0}
        onKeyDown={resizeSidebarWithKeyboard}
        onPointerDown={startSidebarResize}
        onPointerMove={resizeSidebar}
        onPointerUp={stopSidebarResize}
        onPointerCancel={stopSidebarResize}
      />

      <section className="workspace">
        {tabs.length > 0 && (
          <div className="tabbar">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                className={`${tab.id === activeTab?.id ? "active" : ""} ${
                  draggingTabId === tab.id ? "dragging" : ""
                } ${tabDropTargetId === tab.id ? "dropTarget" : ""} ${
                  tabAddTargetId === tab.id ? "addTarget" : ""
                }`}
                draggable
                onClick={() => setActiveTabId(tab.id)}
                onDragStart={(event) => {
                  setDraggingTabId(tab.id);
                  event.dataTransfer.effectAllowed = tab.type === "terminal" ? "copyMove" : "move";
                  event.dataTransfer.setData("application/x-stassh-tab", tab.id);
                  event.dataTransfer.setData("text/plain", tab.title);
                }}
                onDragOver={(event) => {
                  if (!draggingTabId || draggingTabId === tab.id) return;
                  const sourceTab = tabs.find((item) => item.id === draggingTabId);
                  if (sourceTab?.type === "terminal" && tab.type === "layout") {
                    if (tab.sessionIds.includes(sourceTab.sessionId)) {
                      event.dataTransfer.dropEffect = "none";
                      setTabAddTargetId(null);
                      setTabDropTargetId(null);
                      return;
                    }
                    event.preventDefault();
                    event.dataTransfer.dropEffect = "copy";
                    setTabAddTargetId(tab.id);
                    setTabDropTargetId(null);
                    return;
                  }
                  if (sourceTab?.type === "terminal" && tab.type === "terminal") {
                    event.preventDefault();
                    event.dataTransfer.dropEffect = "copy";
                    setTabAddTargetId(tab.id);
                    setTabDropTargetId(null);
                    return;
                  }
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "move";
                  setTabAddTargetId(null);
                  setTabDropTargetId(tab.id);
                }}
                onDragLeave={(event) => {
                  if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
                  if (tabDropTargetId === tab.id) setTabDropTargetId(null);
                  if (tabAddTargetId === tab.id) setTabAddTargetId(null);
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  const sourceId = event.dataTransfer.getData("application/x-stassh-tab") || draggingTabId;
                  if (sourceId) {
                    const sourceTab = tabs.find((item) => item.id === sourceId);
                    if (sourceTab?.type === "terminal" && tab.type === "layout") {
                      if (!tab.sessionIds.includes(sourceTab.sessionId)) {
                        addTerminalTabToLayout(sourceId, tab.id);
                      }
                    } else if (sourceTab?.type === "terminal" && tab.type === "terminal") {
                      createLayoutFromTerminalTabs(sourceId, tab.id);
                    } else {
                      reorderTab(sourceId, tab.id);
                    }
                  }
                  setDraggingTabId(null);
                  setTabDropTargetId(null);
                  setTabAddTargetId(null);
                }}
                onDragEnd={() => {
                  setDraggingTabId(null);
                  setTabDropTargetId(null);
                  setTabAddTargetId(null);
                }}
              >
                {tab.type === "terminal" ? <TerminalSquare size={15} /> : <Monitor size={15} />}
                <span>{tab.title}</span>
                <X
                  size={14}
                  onClick={(event) => {
                    event.stopPropagation();
                    closeTab(tab);
                  }}
                />
              </button>
            ))}
            <button
              className="tabbarAction"
              title="Create layout from open terminals"
              onClick={createLayoutTab}
              disabled={!tabs.some((tab) => tab.type === "terminal")}
            >
              <Plus size={15} /> Layout
            </button>
          </div>
        )}
        <div className="content">
          {!tabs.length && (
            <DetailsPane
              workspace={workspace}
            />
          )}
          <TerminalStage
            tabs={tabs}
            hosts={workspace.hosts}
            terminalOrder={terminalOrder}
            activeTab={activeTab}
            fullscreenSessionId={fullscreenSessionId}
            onInput={writeTerminalInput}
            onActivateTab={setActiveTabId}
            onUpdateLayout={updateLayout}
            onRemoveFromLayout={removeSessionFromLayout}
            onEnterFullscreen={setFullscreenSessionId}
            onExitFullscreen={() => setFullscreenSessionId(null)}
          />
        </div>
      </section>

      {canShowInspector && (
        <aside className={`inspector ${inspectorExpanded ? "" : "collapsed"} ${editorMode ? "editing" : ""}`}>
          <Inspector
            mode={editorMode}
            workspace={workspace}
            target={inspectorTarget}
            collapsed={inspectorCollapsed && !editorMode}
            hostForm={hostForm}
            setHostForm={setHostForm}
            folderForm={folderForm}
            setFolderForm={setFolderForm}
            onSaveHost={saveHost}
            onSaveFolder={saveFolder}
            onCancel={cancelEditor}
            onCollapse={() => setInspectorCollapsedAndResize(true)}
            onExpand={() => setInspectorCollapsedAndResize(false)}
            onConnect={(host) => openTerminal(host)}
            onEditHost={(host) => startHostEditor("host", host)}
            onEditFolder={(folder) => startFolderEditor("folder", folder)}
            onCopyHost={(host) => applySnapshot("copy_host", { hostId: host.id }, "Host copied")}
            onDeleteHost={(host) => {
              if (window.confirm(`Delete host ${host.path}?`)) {
                applySnapshot("delete_host", { hostId: host.id }, "Host deleted");
              }
            }}
            onDeleteFolder={(folder) => {
              if (folder.parentId && window.confirm(`Delete folder ${folder.path}?`)) {
                applySnapshot("delete_folder", { folderId: folder.id }, "Folder deleted");
              }
            }}
          />
        </aside>
      )}

      <footer className="statusbar">
        <span>{status}</span>
        <span>{workspace.diagnostics.length} diagnostics</span>
        <span>{checkedHosts.size} selected</span>
        <span>{workspace.vaultPath}</span>
      </footer>
    </div>
  );
}

function InventoryTree(props: {
  workspace: WorkspaceSnapshot;
  openSessionCounts: Map<Id, number>;
  expanded: Set<Id>;
  setExpanded: (value: Set<Id>) => void;
  selection: Selection | null;
  setSelection: (value: Selection) => void;
  checkedHosts: Set<Id>;
  setCheckedHosts: (value: Set<Id>) => void;
  draggingHostIds: Id[];
  dropTargetFolderId: Id | null;
  setDraggingHostIds: (value: Id[]) => void;
  setDropTargetFolderId: (value: Id | null) => void;
  onMoveHosts: (hostIds: Id[], folderId: Id) => Promise<void>;
  onOpen: (host: HostView) => void;
}) {
  const {
    workspace,
    openSessionCounts,
    expanded,
    setExpanded,
    selection,
    setSelection,
    checkedHosts,
    setCheckedHosts,
    draggingHostIds,
    dropTargetFolderId,
    setDraggingHostIds,
    setDropTargetFolderId,
    onMoveHosts,
    onOpen,
  } = props;
  const rows = useMemo(() => treeRows(workspace, expanded), [workspace, expanded]);

  function toggleFolder(folderId: Id) {
    const next = new Set(expanded);
    if (next.has(folderId)) next.delete(folderId);
    else next.add(folderId);
    setExpanded(next);
  }

  function toggleHostSelection(hostId: Id) {
    const next = new Set(checkedHosts);
    if (next.has(hostId)) next.delete(hostId);
    else next.add(hostId);
    setCheckedHosts(next);
  }

  function toggleFolderSelection(folderId: Id) {
    const hostIds = descendantHostIds(workspace, folderId);
    const next = new Set(checkedHosts);
    const allSelected = hostIds.every((id: Id) => next.has(id));
    hostIds.forEach((id: Id) => (allSelected ? next.delete(id) : next.add(id)));
    setCheckedHosts(next);
  }

  function selectFolder(event: React.MouseEvent<HTMLDivElement>, folderId: Id) {
    if (event.metaKey || event.ctrlKey) {
      toggleFolderSelection(folderId);
      return;
    }
    setSelection({ type: "folder", id: folderId });
  }

  function selectHost(event: React.MouseEvent<HTMLDivElement>, hostId: Id) {
    if (event.metaKey || event.ctrlKey) {
      toggleHostSelection(hostId);
      return;
    }
    setSelection({ type: "host", id: hostId });
  }

  function startHostDrag(event: React.DragEvent<HTMLDivElement>, host: HostView) {
    const hostIds = checkedHosts.has(host.id) ? Array.from(checkedHosts) : [host.id];
    setDraggingHostIds(hostIds);
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("application/x-stassh-hosts", JSON.stringify(hostIds));
    event.dataTransfer.setData("text/plain", host.displayName);
  }

  function dragOverFolder(event: React.DragEvent<HTMLDivElement>, folderId: Id) {
    if (!draggingHostIds.length) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    setDropTargetFolderId(folderId);
  }

  function leaveFolderDropTarget(event: React.DragEvent<HTMLDivElement>, folderId: Id) {
    if (dropTargetFolderId !== folderId || event.currentTarget.contains(event.relatedTarget as Node | null)) {
      return;
    }
    setDropTargetFolderId(null);
  }

  async function dropHostsOnFolder(event: React.DragEvent<HTMLDivElement>, folderId: Id) {
    event.preventDefault();
    const serialized = event.dataTransfer.getData("application/x-stassh-hosts");
    let hostIds = draggingHostIds;
    if (serialized) {
      try {
        const parsed = JSON.parse(serialized);
        if (Array.isArray(parsed)) hostIds = parsed.filter((value): value is Id => typeof value === "string");
      } catch {
        hostIds = [];
      }
    }
    const movableHostIds = hostIds.filter((hostId) => workspace.hosts.some((host) => host.id === hostId));
    const needsMove = movableHostIds.some((hostId) => workspace.hosts.find((host) => host.id === hostId)?.folderId !== folderId);
    setDropTargetFolderId(null);
    setDraggingHostIds([]);
    if (!movableHostIds.length || !needsMove) return;
    await onMoveHosts(movableHostIds, folderId);
  }

  function endHostDrag() {
    setDraggingHostIds([]);
    setDropTargetFolderId(null);
  }

  return (
    <div className="tree">
      {rows.map((row) =>
        row.kind === "folder" ? (
          <div
            key={row.folder.id}
            className={`treeRow ${selection?.type === "folder" && selection.id === row.folder.id ? "active" : ""} ${
              folderSelectionState(workspace, checkedHosts, row.folder.id) !== "none" ? "selectedItem" : ""
            } ${dropTargetFolderId === row.folder.id ? "dropTarget" : ""} ${
              dropTargetFolderId === row.folder.id &&
              draggingHostIds.every((hostId) => workspace.hosts.find((host) => host.id === hostId)?.folderId === row.folder.id)
                ? "dropTargetCurrent"
                : ""
            }`}
            style={{ paddingLeft: 10 + row.depth * 16 }}
            onClick={(event) => selectFolder(event, row.folder.id)}
            onDragOver={(event) => dragOverFolder(event, row.folder.id)}
            onDragLeave={(event) => leaveFolderDropTarget(event, row.folder.id)}
            onDrop={(event) => dropHostsOnFolder(event, row.folder.id)}
          >
            <button className="iconButton" onClick={() => toggleFolder(row.folder.id)}>
              {expanded.has(row.folder.id) ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
            </button>
            <Folder size={15} />
            <span>{row.folder.name}</span>
            <small>{row.folder.hostCount}</small>
          </div>
        ) : (
          <div
            key={row.host.id}
            className={`treeRow host ${selection?.type === "host" && selection.id === row.host.id ? "active" : ""} ${
              checkedHosts.has(row.host.id) ? "selectedItem" : ""
            } ${draggingHostIds.includes(row.host.id) ? "dragging" : ""}
            }`}
            style={{ paddingLeft: 34 + row.depth * 16 }}
            draggable
            onClick={(event) => selectHost(event, row.host.id)}
            onDoubleClick={() => onOpen(row.host)}
            onDragStart={(event) => startHostDrag(event, row.host)}
            onDragEnd={endHostDrag}
          >
            <span>{row.host.displayName}</span>
            <SessionCountMarker count={openSessionCounts.get(row.host.id) ?? 0} />
          </div>
        ),
      )}
    </div>
  );
}

function SessionCountMarker({ count }: { count: number }) {
  if (count <= 0) return null;
  if (count > 3) return <span className="sessionCountBadge">{count}</span>;
  return (
    <span className="sessionDots" aria-label={`${count} open sessions`}>
      {Array.from({ length: count }, (_, index) => (
        <span key={index} />
      ))}
    </span>
  );
}

function SearchResults(props: {
  results: SearchResult[];
  selectedId: Id | null;
  onSelect: (id: Id) => void;
  onOpen: (id: Id) => void;
}) {
  return (
    <div className="searchResults">
      {props.results.map((result) => (
        <button
          key={result.id}
          className={props.selectedId === result.id ? "active" : ""}
          onClick={() => props.onSelect(result.id)}
          onDoubleClick={() => props.onOpen(result.id)}
        >
          <span>{result.path}</span>
          <small>
            {result.username ? `${result.username}@` : ""}
            {result.target}
          </small>
        </button>
      ))}
    </div>
  );
}

function DetailsPane(props: {
  workspace: WorkspaceSnapshot;
}) {
  return (
    <div className="details homeDetails">
      <h2>Workspace</h2>
      <div className="detailGrid">
        <label>Vault</label>
        <span>{props.workspace.vaultPath}</span>
        <label>Local Config</label>
        <span>{props.workspace.localConfigPath}</span>
        <label>Secrets</label>
        <span>{props.workspace.secretsAvailable ? props.workspace.secretsPath : "not available"}</span>
        <label>Hosts</label>
        <span>{props.workspace.hosts.length}</span>
        <label>Folders</label>
        <span>{props.workspace.folders.length}</span>
      </div>
      <section>
        <h3>Diagnostics</h3>
        <Diagnostics diagnostics={props.workspace.diagnostics} />
      </section>
    </div>
  );
}

function Inspector(props: {
  mode: EditorMode;
  workspace: WorkspaceSnapshot;
  target: InspectorTarget;
  collapsed: boolean;
  hostForm: HostForm | null;
  setHostForm: (form: HostForm | null) => void;
  folderForm: FolderForm | null;
  setFolderForm: (form: FolderForm | null) => void;
  onSaveHost: () => void;
  onSaveFolder: () => void;
  onCancel: () => void;
  onCollapse: () => void;
  onExpand: () => void;
  onConnect: (host: HostView) => void;
  onEditHost: (host: HostView) => void;
  onEditFolder: (folder: FolderView) => void;
  onCopyHost: (host: HostView) => void;
  onDeleteHost: (host: HostView) => void;
  onDeleteFolder: (folder: FolderView) => void;
}) {
  if (props.collapsed) {
    return (
      <button className="inspectorExpandButton" title="Show inspector" onClick={props.onExpand}>
        <PanelRightOpen size={16} />
      </button>
    );
  }

  if ((props.mode === "host" || props.mode === "new-host") && props.hostForm) {
    const form = props.hostForm;
    const setForm = (patch: Partial<HostForm>) => props.setHostForm({ ...form, ...patch });
    return (
      <div className="editor">
        <EditorHeader title={props.mode === "new-host" ? "New Host" : "Edit Host"} />
        <EditorActions onSave={props.onSaveHost} onCancel={props.onCancel} />
        <Field label="Name" value={form.displayName} onChange={(displayName) => setForm({ displayName })} />
        <Field label="HostName" value={form.hostname} onChange={(hostname) => setForm({ hostname })} />
        <Field label="Port" value={form.port} onChange={(port) => setForm({ port })} />
        <Field label="User" value={form.username} onChange={(username) => setForm({ username })} />
        <label>Folder</label>
        <select value={form.folderId} onChange={(event) => setForm({ folderId: event.target.value })}>
          {props.workspace.folders.map((folder) => (
            <option key={folder.id} value={folder.id}>
              {folder.path}
            </option>
          ))}
        </select>
        <label>Identity</label>
        <select
          value={form.identityFingerprint}
          onChange={(event) => setForm({ identityFingerprint: event.target.value })}
        >
          <option value="">none</option>
          {props.workspace.identities.map((identity) => (
            <option key={identity.fingerprint} value={identity.fingerprint}>
              {(identity.preferredName || identity.fingerprint) + (identity.exists ? "" : " (missing)")}
            </option>
          ))}
        </select>
        <Field label="Secrets Set" value={form.secrets} onChange={(secrets) => setForm({ secrets })} />
        <Field label="Jump IDs" value={form.jumpChainText} onChange={(jumpChainText) => setForm({ jumpChainText })} />
        <Field label="Tags" value={form.tags} onChange={(tags) => setForm({ tags })} />
        <label>Notes</label>
        <textarea value={form.notes} onChange={(event) => setForm({ notes: event.target.value })} />
        <ForwardEditor forwards={form.forwards} onChange={(forwards) => setForm({ forwards })} />
      </div>
    );
  }
  if ((props.mode === "folder" || props.mode === "new-folder") && props.folderForm) {
    const form = props.folderForm;
    const setForm = (patch: Partial<FolderForm>) => props.setFolderForm({ ...form, ...patch });
    return (
      <div className="editor">
        <EditorHeader title={props.mode === "new-folder" ? "New Folder" : "Rename Folder"} />
        <EditorActions onSave={props.onSaveFolder} onCancel={props.onCancel} />
        <Field label="Name" value={form.name} onChange={(name) => setForm({ name })} />
        {props.mode === "new-folder" && (
          <>
            <label>Parent</label>
            <select value={form.parentId} onChange={(event) => setForm({ parentId: event.target.value })}>
              {props.workspace.folders.map((folder) => (
                <option key={folder.id} value={folder.id}>
                  {folder.path}
                </option>
              ))}
            </select>
          </>
        )}
      </div>
    );
  }

  if (props.target?.type === "host") {
    return (
      <HostInspectorDetails
        target={props.target}
        onCollapse={props.onCollapse}
        onConnect={props.onConnect}
        onEdit={props.onEditHost}
        onCopy={props.onCopyHost}
        onDelete={props.onDeleteHost}
      />
    );
  }

  if (props.target?.type === "folder") {
    return (
      <FolderInspectorDetails
        folder={props.target.folder}
        diagnostics={props.workspace.diagnostics}
        onCollapse={props.onCollapse}
        onEdit={props.onEditFolder}
        onDelete={props.onDeleteFolder}
      />
    );
  }

  if (props.target?.type === "layout") {
    return <LayoutInspectorDetails layout={props.target.layout} onCollapse={props.onCollapse} />;
  }

  return (
    <div className="inspectorEmpty">
      <InspectorHeader title="Inspector" subtitle="No active item" onCollapse={props.onCollapse} />
      <p>Select a host, folder, terminal, or layout pane.</p>
    </div>
  );
}

function InspectorHeader(props: { title: string; subtitle: string; onCollapse: () => void }) {
  return (
    <div className="inspectorHeader">
      <div>
        <h2>{props.title}</h2>
        <small>{props.subtitle}</small>
      </div>
      <button className="iconOnlyButton" title="Collapse inspector" onClick={props.onCollapse}>
        <PanelRightClose size={16} />
      </button>
    </div>
  );
}

function EditorHeader(props: { title: string }) {
  return (
    <div className="editorHeader">
      <h2>{props.title}</h2>
    </div>
  );
}

function HostInspectorDetails(props: {
  target: Extract<InspectorTarget, { type: "host" }>;
  onCollapse: () => void;
  onConnect: (host: HostView) => void;
  onEdit: (host: HostView) => void;
  onCopy: (host: HostView) => void;
  onDelete: (host: HostView) => void;
}) {
  const { host, details, terminal, source } = props.target;
  const subtitle =
    source === "terminal"
      ? `${terminal?.status ?? "session"} session`
      : source === "layout"
        ? `${terminal?.title ?? host.displayName} pane`
        : "Selected host";
  return (
    <div className="inspectorDetails">
      <InspectorHeader title={host.displayName} subtitle={subtitle} onCollapse={props.onCollapse} />
      <div className="inspectorActions">
        <button onClick={() => props.onConnect(host)}>
          <TerminalSquare size={16} /> Connect
        </button>
        <button onClick={() => props.onEdit(host)}>
          <Pencil size={16} /> Edit
        </button>
        <button onClick={() => props.onCopy(host)}>
          <Copy size={16} /> Copy
        </button>
        <button className="danger" onClick={() => props.onDelete(host)}>
          <Trash2 size={16} /> Delete
        </button>
      </div>
      <DetailList>
        <DetailRow label="Path" value={host.path} />
        <DetailRow label="HostName" value={host.hostname} />
        <DetailRow label="Port" value={String(host.port)} />
        <DetailRow label="User" value={host.username || "OpenSSH default"} />
        <DetailRow label="Identity" value={host.identityFingerprint || "none"} />
        <DetailRow label="Secrets" value={host.secrets || "none"} />
        <DetailRow label="Actions" value={String(host.actionCount)} />
        <DetailRow label="Tags" value={host.tags.join(", ") || "none"} />
        <DetailRow label="Notes" value={host.notes || "none"} />
      </DetailList>
      <section>
        <h3>OpenSSH Preview</h3>
        <code>{details?.sshCommand ?? "Preparing command"}</code>
      </section>
      <section>
        <h3>Jump Chain</h3>
        {details?.jumps.length ? (
          details.jumps.map((jump) => (
            <p key={jump.id}>
              {jump.displayName} - {jump.username ? `${jump.username}@` : ""}
              {jump.hostname}:{jump.port}
            </p>
          ))
        ) : (
          <p>No jumps configured</p>
        )}
      </section>
      <section>
        <h3>Diagnostics</h3>
        <Diagnostics diagnostics={details?.diagnostics ?? []} />
      </section>
    </div>
  );
}

function FolderInspectorDetails(props: {
  folder: FolderView;
  diagnostics: DiagnosticView[];
  onCollapse: () => void;
  onEdit: (folder: FolderView) => void;
  onDelete: (folder: FolderView) => void;
}) {
  return (
    <div className="inspectorDetails">
      <InspectorHeader title={props.folder.name} subtitle="Selected folder" onCollapse={props.onCollapse} />
      <div className="inspectorActions">
        <button onClick={() => props.onEdit(props.folder)}>
          <Pencil size={16} /> Rename
        </button>
        <button className="danger" onClick={() => props.onDelete(props.folder)} disabled={!props.folder.parentId}>
          <Trash2 size={16} /> Delete
        </button>
      </div>
      <DetailList>
        <DetailRow label="Path" value={props.folder.path} />
        <DetailRow label="Direct Hosts" value={String(props.folder.hostCount)} />
      </DetailList>
      <section>
        <h3>Diagnostics</h3>
        <Diagnostics diagnostics={props.diagnostics} />
      </section>
    </div>
  );
}

function LayoutInspectorDetails(props: { layout: Extract<Tab, { type: "layout" }>; onCollapse: () => void }) {
  return (
    <div className="inspectorDetails">
      <InspectorHeader title={props.layout.title} subtitle="Layout" onCollapse={props.onCollapse} />
      <DetailList>
        <DetailRow label="Mode" value={props.layout.mode === "main" ? "Main pane" : "Grid"} />
        <DetailRow label="Panes" value={String(props.layout.sessionIds.length)} />
        <DetailRow label="Broadcast" value={props.layout.broadcastInput ? "on" : "off"} />
      </DetailList>
    </div>
  );
}

function DetailList(props: { children: React.ReactNode }) {
  return <div className="inspectorDetailList">{props.children}</div>;
}

function DetailRow(props: { label: string; value: string }) {
  return (
    <>
      <label>{props.label}</label>
      <span>{props.value}</span>
    </>
  );
}

function resolveInspectorTarget({
  activeTab,
  tabs,
  workspace,
  selection,
  selectedHost,
  selectedFolder,
  details,
}: {
  activeTab: Tab | null;
  tabs: Tab[];
  workspace: WorkspaceSnapshot;
  selection: Selection | null;
  selectedHost: HostView | null;
  selectedFolder: FolderView | null;
  details: HostDetails | null;
}): InspectorTarget {
  if (!activeTab) {
    if (selection?.type === "host" && selectedHost) {
      return { type: "host", source: "details", host: selectedHost, details, terminal: null };
    }
    if (selection?.type === "folder" && selectedFolder) {
      return { type: "folder", source: "details", folder: selectedFolder };
    }
    return null;
  }
  if (activeTab.type === "terminal") {
    const host = workspace.hosts.find((item) => item.id === activeTab.hostId);
    return host ? { type: "host", source: "terminal", host, details, terminal: activeTab } : null;
  }
  if (activeTab.type === "layout") {
    const activeSessionId = activeTab.activeSessionId ?? activeTab.sessionIds[0];
    const terminal =
      tabs.find(
        (tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal" && tab.sessionId === activeSessionId,
      ) ?? null;
    const host = terminal ? workspace.hosts.find((item) => item.id === terminal.hostId) ?? null : null;
    return host ? { type: "host", source: "layout", host, details, terminal } : { type: "layout", source: "layout", layout: activeTab };
  }
  if (selection?.type === "host" && selectedHost) {
    return { type: "host", source: "details", host: selectedHost, details, terminal: null };
  }
  if (selection?.type === "folder" && selectedFolder) {
    return { type: "folder", source: "details", folder: selectedFolder };
  }
  return null;
}

function TerminalStage(props: {
  tabs: Tab[];
  hosts: HostView[];
  terminalOrder: Id[];
  activeTab: Tab | null;
  fullscreenSessionId: Id | null;
  onInput: (sessionId: Id, data: string) => void;
  onActivateTab: (tabId: Id) => void;
  onUpdateLayout: (layoutId: Id, patch: Partial<Extract<Tab, { type: "layout" }>>) => void;
  onRemoveFromLayout: (layoutId: Id, sessionId: Id) => void;
  onEnterFullscreen: (sessionId: Id) => void;
  onExitFullscreen: () => void;
}) {
  const terminalById = new Map(
    props.tabs
      .filter((tab): tab is Extract<Tab, { type: "terminal" }> => tab.type === "terminal")
      .map((tab) => [tab.sessionId, tab]),
  );
  const terminalTabs = [
    ...props.terminalOrder
      .map((sessionId) => terminalById.get(sessionId))
      .filter((tab): tab is Extract<Tab, { type: "terminal" }> => Boolean(tab)),
    ...Array.from(terminalById.values()).filter((tab) => !props.terminalOrder.includes(tab.sessionId)),
  ];
  const hostsById = new Map(props.hosts.map((host) => [host.id, host]));
  const layout = props.activeTab?.type === "layout" ? props.activeTab : null;
  const visibleSessionIds =
    props.activeTab?.type === "terminal"
      ? [props.activeTab.sessionId]
      : layout
        ? layout.sessionIds
        : [];
  const visibleSessionSet = new Set(
    props.fullscreenSessionId ? [...visibleSessionIds, props.fullscreenSessionId] : visibleSessionIds,
  );
  const focusedSessionId =
    props.fullscreenSessionId ??
    (props.activeTab?.type === "terminal" ? props.activeTab.sessionId : layout?.activeSessionId ?? null);
  const modeClass = layout ? `layoutMode ${layout.mode}` : props.activeTab?.type === "terminal" ? "singleMode" : "";

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => window.dispatchEvent(new Event("resize")));
    return () => window.cancelAnimationFrame(frame);
  }, [props.activeTab]);

  function setMainRatioFromPointer(event: React.PointerEvent<HTMLDivElement>) {
    if (!layout) return;
    const bounds = event.currentTarget.parentElement?.getBoundingClientRect();
    if (!bounds?.width) return;
    const next = (event.clientX - bounds.left) / bounds.width;
    props.onUpdateLayout(layout.id, { mainRatio: Math.min(maxMainRatio, Math.max(minMainRatio, next)) });
  }

  function startMainResize(event: React.PointerEvent<HTMLDivElement>) {
    if (!layout) return;
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    setMainRatioFromPointer(event);
  }

  function moveMainResize(event: React.PointerEvent<HTMLDivElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      setMainRatioFromPointer(event);
    }
  }

  function stopMainResize(event: React.PointerEvent<HTMLDivElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      window.dispatchEvent(new Event("resize"));
    }
  }

  const gridStyle = layout ? layoutStageStyle(layout) : undefined;
  const layoutSessions = terminalTabs.filter((tab) => layout?.sessionIds.includes(tab.sessionId));

  return (
    <div className={`tabPanel terminalStagePanel ${visibleSessionIds.length || props.fullscreenSessionId ? "active" : ""}`}>
      {layout && (
        <div className="layoutToolbar">
          <div className="layoutSegment">
            <button
              className={layout.mode === "grid" ? "active" : ""}
              onClick={() => props.onUpdateLayout(layout.id, { mode: "grid" })}
            >
              Grid
            </button>
            <button
              className={layout.mode === "main" ? "active" : ""}
              onClick={() => props.onUpdateLayout(layout.id, { mode: "main" })}
            >
              Main
            </button>
          </div>
          <button
            className={`broadcastToggle ${layout.broadcastInput ? "active" : ""}`}
            aria-pressed={layout.broadcastInput}
            onClick={() => props.onUpdateLayout(layout.id, { broadcastInput: !layout.broadcastInput })}
          >
            Broadcast
          </button>
          <span>{layout.sessionIds.length} panes</span>
        </div>
      )}
      <div className={`terminalStage ${modeClass}`} style={gridStyle}>
        {terminalTabs.map((tab) => {
          const visible = visibleSessionSet.has(tab.sessionId);
          const focused = focusedSessionId === tab.sessionId;
          const fullscreen = props.fullscreenSessionId === tab.sessionId;
          const paneStyle = layout ? layoutPaneStyle(layout, tab.sessionId) : undefined;
          const notes = hostsById.get(tab.hostId)?.notes ?? null;
          return (
            <TerminalPane
              key={tab.sessionId}
              tab={tab}
              notes={notes}
              visible={visible}
              focused={focused}
              fullscreen={fullscreen}
              style={paneStyle}
              showPaneControls={Boolean(layout && visible)}
              onInput={(sessionId, data) => {
                if (layout?.broadcastInput && layout.sessionIds.includes(sessionId)) {
                  for (const targetSessionId of layout.sessionIds) props.onInput(targetSessionId, data);
                } else {
                  props.onInput(sessionId, data);
                }
              }}
              onFocus={() => {
                if (layout && visible) props.onUpdateLayout(layout.id, { activeSessionId: tab.sessionId });
                else props.onActivateTab(tab.id);
              }}
              onMakeMain={() => layout && props.onUpdateLayout(layout.id, { activeSessionId: tab.sessionId, mode: "main" })}
              onRemove={() => layout && props.onRemoveFromLayout(layout.id, tab.sessionId)}
              onEnterFullscreen={() => props.onEnterFullscreen(tab.sessionId)}
              onExitFullscreen={props.onExitFullscreen}
            />
          );
        })}
        {layout?.mode === "main" && layout.sessionIds.length > 1 && (
          <div
            className="mainSplitHandle"
            role="separator"
            aria-label="Resize main terminal pane"
            aria-orientation="vertical"
            onPointerDown={startMainResize}
            onPointerMove={moveMainResize}
            onPointerUp={stopMainResize}
            onPointerCancel={stopMainResize}
          />
        )}
        {layout && !layoutSessions.length && <div className="empty layoutEmpty">No terminals in this layout</div>}
      </div>
    </div>
  );
}

function TerminalPane({
  tab,
  notes,
  visible,
  focused,
  fullscreen,
  style,
  showPaneControls,
  onInput,
  onFocus,
  onMakeMain,
  onRemove,
  onEnterFullscreen,
  onExitFullscreen,
}: {
  tab: Extract<Tab, { type: "terminal" }>;
  notes: string | null;
  visible: boolean;
  focused: boolean;
  fullscreen: boolean;
  style?: React.CSSProperties;
  showPaneControls: boolean;
  onInput: (sessionId: Id, data: string) => void;
  onFocus: () => void;
  onMakeMain: () => void;
  onRemove: () => void;
  onEnterFullscreen: () => void;
  onExitFullscreen: () => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const activeRef = useRef(visible);
  const focusedRef = useRef(focused);
  const inputRef = useRef(onInput);
  const displayNotes = notes?.trim() || null;
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [findCaseSensitive, setFindCaseSensitive] = useState(false);
  const [findResult, setFindResult] = useState({ index: 0, count: 0 });
  const findOpenRef = useRef(findOpen);

  useEffect(() => {
    activeRef.current = visible;
  }, [visible]);

  useEffect(() => {
    focusedRef.current = focused;
  }, [focused]);

  useEffect(() => {
    findOpenRef.current = findOpen;
  }, [findOpen]);

  useEffect(() => {
    inputRef.current = onInput;
  }, [onInput]);

  useEffect(() => {
    if (!findOpen || !focused) return;
    const frame = window.requestAnimationFrame(() => {
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [findOpen, focused]);

  function clearFind() {
    try {
      searchRef.current?.clearDecorations();
    } catch (error) {
      console.error("terminal find clear failed", error);
    }
    setFindResult({ index: 0, count: 0 });
  }

  function runFind(direction: "next" | "previous", query = findQuery) {
    const search = searchRef.current;
    if (!search) return;
    if (!query) {
      clearFind();
      return;
    }
    try {
      const options = { caseSensitive: findCaseSensitive, incremental: true };
      const found = direction === "previous" ? search.findPrevious(query, options) : search.findNext(query, options);
      setFindResult({ index: found ? 1 : 0, count: found ? 1 : 0 });
    } catch (error) {
      console.error("terminal find failed", error);
      setFindResult({ index: 0, count: 0 });
    }
  }

  function closeFind() {
    setFindOpen(false);
    clearFind();
    terminalRef.current?.focus();
  }

  useEffect(() => {
    if (!findOpen) return;
    runFind("next");
  }, [findOpen, findQuery, findCaseSensitive]);

  function resizeTerminal() {
    const terminal = terminalRef.current;
    const fit = fitRef.current;
    if (!terminal || !fit || !activeRef.current) return;
    fit.fit();
    invoke("resize_terminal", {
      sessionId: tab.sessionId,
      cols: terminal.cols,
      rows: terminal.rows,
    }).catch(() => undefined);
  }

  useEffect(() => {
    if (!ref.current) return;
    const terminal = new Terminal({
      convertEol: true,
      cursorBlink: true,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
      fontSize: 13,
      theme: {
        background: "#101317",
        foreground: "#dce2ea",
        cursor: "#f0b95a",
        selectionBackground: "#f0b95a",
        selectionForeground: "#101317",
        selectionInactiveBackground: "#b98732",
      },
    });
    const fit = new FitAddon();
    const search = new SearchAddon({ highlightLimit: 1000 });
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.open(ref.current);
    terminalRef.current = terminal;
    fitRef.current = fit;
    searchRef.current = search;
    fit.fit();

    terminal.onData((data) => inputRef.current(tab.sessionId, data));
    terminal.attachCustomKeyEventHandler((event) => {
      if (
        event.type === "keydown" &&
        activeRef.current &&
        focusedRef.current &&
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === "f"
      ) {
        setFindOpen(true);
        window.requestAnimationFrame(() => {
          searchInputRef.current?.focus();
          searchInputRef.current?.select();
        });
        return false;
      }
      if (
        event.type === "keydown" &&
        activeRef.current &&
        focusedRef.current &&
        findOpenRef.current &&
        event.key === "Escape"
      ) {
        closeFind();
        return false;
      }
      return true;
    });
    const onData = (event: Event) => terminal.write((event as CustomEvent<string>).detail);
    window.addEventListener(`terminal-data:${tab.sessionId}`, onData);
    window.addEventListener("resize", resizeTerminal);
    resizeTerminal();
    if (focused) terminal.focus();
    return () => {
      window.removeEventListener(`terminal-data:${tab.sessionId}`, onData);
      window.removeEventListener("resize", resizeTerminal);
      terminalRef.current = null;
      fitRef.current = null;
      searchRef.current = null;
      terminal.dispose();
    };
  }, [tab.sessionId]);

  useEffect(() => {
    if (!visible) return;
    const frame = window.requestAnimationFrame(() => {
      resizeTerminal();
      if (focused) terminalRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [visible, focused, fullscreen, style]);

  return (
    <div
      className={`terminalPanel ${visible ? "active" : ""} ${focused ? "focused" : ""} ${
        fullscreen ? "fullscreen" : ""
      }`}
      style={style}
      onMouseDown={onFocus}
    >
      <div className="terminalStatus">
        <div className="terminalTitle">
          <span className="terminalHostTitle">{tab.title}</span>
          {displayNotes && <span className="terminalNotes">{displayNotes}</span>}
        </div>
        {focused &&
          (findOpen ? (
            <div className="terminalFind">
              <input
                ref={searchInputRef}
                value={findQuery}
                onChange={(event) => setFindQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    closeFind();
                  } else if (event.key === "Enter") {
                    event.preventDefault();
                    runFind(event.shiftKey ? "previous" : "next");
                  }
                }}
                placeholder="Find"
              />
              <button title="Previous match" disabled={!findQuery} onClick={() => runFind("previous")}>
                <ChevronUp size={13} />
              </button>
              <button title="Next match" disabled={!findQuery} onClick={() => runFind("next")}>
                <ChevronDown size={13} />
              </button>
              <button
                className={findCaseSensitive ? "active" : ""}
                title="Case sensitive"
                aria-pressed={findCaseSensitive}
                onClick={() => setFindCaseSensitive((current) => !current)}
              >
                <CaseSensitive size={14} />
              </button>
              <span className="terminalFindCount">
                {findQuery ? (findResult.count ? "Match" : "0/0") : ""}
              </span>
              <button title="Close find" onClick={closeFind}>
                <X size={13} />
              </button>
            </div>
          ) : (
            <button className="terminalFindButton" title="Find" onClick={() => setFindOpen(true)}>
              <Search size={13} />
            </button>
          ))}
        <small>{tab.status}</small>
        {showPaneControls && (
          <div className="paneActions">
            <button title="Use as main pane" onClick={onMakeMain}>
              Main
            </button>
            <button title="Remove from layout" onClick={onRemove}>
              <X size={13} />
            </button>
          </div>
        )}
        <button
          className="paneFullscreenButton"
          title={fullscreen ? "Exit full screen" : "Full screen"}
          onClick={fullscreen ? onExitFullscreen : onEnterFullscreen}
        >
          {fullscreen ? <Minimize2 size={13} /> : <Maximize2 size={13} />}
        </button>
      </div>
      <div ref={ref} className="terminal" />
    </div>
  );
}

function Field(props: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <>
      <label>{props.label}</label>
      <input value={props.value} onChange={(event) => props.onChange(event.target.value)} />
    </>
  );
}

function EditorActions(props: { onSave: () => void; onCancel: () => void }) {
  return (
    <div className="editorActions">
      <button onClick={props.onSave}>
        <Save size={16} /> Save
      </button>
      <button onClick={props.onCancel}>
        <X size={16} /> Cancel
      </button>
    </div>
  );
}

function ForwardEditor(props: { forwards: Forward[]; onChange: (forwards: Forward[]) => void }) {
  const addLocal = () =>
    props.onChange([
      ...props.forwards,
      {
        type: "local",
        bind_address: "127.0.0.1",
        local_port: 5901,
        destination_host: "127.0.0.1",
        destination_port: 5901,
      },
    ]);
  return (
    <section className="forwards">
      <h3>Forwards</h3>
      {props.forwards.map((forward, index) => (
        <div className="forwardRow" key={index}>
          <span>{forward.type}</span>
          <code>{formatForward(forward)}</code>
          <button onClick={() => props.onChange(props.forwards.filter((_, i) => i !== index))}>
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      <button onClick={addLocal}>
        <Plus size={15} /> Local Forward
      </button>
    </section>
  );
}

function Diagnostics({ diagnostics }: { diagnostics: DiagnosticView[] }) {
  if (!diagnostics.length) return <p>No diagnostics</p>;
  return (
    <div className="diagnostics">
      {diagnostics.map((diagnostic, index) => (
        <p key={index} className={diagnostic.severity}>
          {diagnostic.message}
        </p>
      ))}
    </div>
  );
}

type TreeRow =
  | { kind: "folder"; folder: FolderView; depth: number }
  | { kind: "host"; host: HostView; depth: number };

function treeRows(workspace: WorkspaceSnapshot, expanded: Set<Id>): TreeRow[] {
  const rows: Array<
    | { kind: "folder"; folder: FolderView; depth: number }
    | { kind: "host"; host: HostView; depth: number }
  > = [];
  const foldersByParent = new Map<Id | null, FolderView[]>();
  workspace.folders.forEach((folder) => {
    const list = foldersByParent.get(folder.parentId) ?? [];
    list.push(folder);
    foldersByParent.set(folder.parentId, list);
  });
  const hostsByFolder = new Map<Id, HostView[]>();
  workspace.hosts.forEach((host) => {
    const list = hostsByFolder.get(host.folderId) ?? [];
    list.push(host);
    hostsByFolder.set(host.folderId, list);
  });
  foldersByParent.forEach((list) => list.sort((a, b) => a.name.localeCompare(b.name)));
  hostsByFolder.forEach((list) => list.sort((a, b) => a.displayName.localeCompare(b.displayName)));
  function visit(parentId: Id | null, depth: number): void {
    for (const folder of foldersByParent.get(parentId) ?? []) {
      rows.push({ kind: "folder", folder, depth });
      if (expanded.has(folder.id)) {
        for (const host of hostsByFolder.get(folder.id) ?? []) rows.push({ kind: "host", host, depth: depth + 1 });
        visit(folder.id, depth + 1);
      }
    }
  }
  visit(null, 0);
  return rows;
}

function descendantHostIds(workspace: WorkspaceSnapshot, folderId: Id): Id[] {
  const childFolders = workspace.folders.filter((folder) => folder.parentId === folderId);
  return [
    ...workspace.hosts.filter((host) => host.folderId === folderId).map((host) => host.id),
    ...childFolders.flatMap((folder) => descendantHostIds(workspace, folder.id)),
  ];
}

function folderSelectionState(
  workspace: WorkspaceSnapshot,
  checkedHosts: Set<Id>,
  folderId: Id,
): "none" | "some" | "all" {
  const descendants = descendantHostIds(workspace, folderId);
  if (!descendants.length) return "none";
  return descendants.every((id: Id) => checkedHosts.has(id))
    ? "all"
    : descendants.some((id: Id) => checkedHosts.has(id))
      ? "some"
      : "none";
}

function formatForward(forward: Forward) {
  if (forward.type === "remote") {
    return `${forward.bind_address}:${forward.remote_port} -> ${forward.destination_host}:${forward.destination_port}`;
  }
  if (forward.type === "dynamic") return `${forward.bind_address}:${forward.local_port}`;
  return `${forward.bind_address}:${forward.local_port} -> ${forward.destination_host}:${forward.destination_port}`;
}

function layoutStageStyle(layout: Extract<Tab, { type: "layout" }>): React.CSSProperties {
  if (layout.mode === "main") {
    const secondaryCount = Math.max(1, layout.sessionIds.length - 1);
    const columns = Math.max(1, Math.ceil(Math.sqrt(secondaryCount)));
    const rows = Math.max(1, Math.ceil(secondaryCount / columns));
    return {
      gridTemplateColumns: `${Math.round(layout.mainRatio * 100)}% 6px repeat(${columns}, minmax(120px, 1fr))`,
      gridTemplateRows: `repeat(${rows}, minmax(0, 1fr))`,
    };
  }
  const count = Math.max(1, layout.sessionIds.length);
  const columns = Math.max(1, Math.ceil(Math.sqrt(count)));
  const rows = Math.max(1, Math.ceil(count / columns));
  return {
    gridTemplateColumns: `repeat(${columns}, minmax(180px, 1fr))`,
    gridTemplateRows: `repeat(${rows}, minmax(0, 1fr))`,
  };
}

function layoutPaneStyle(layout: Extract<Tab, { type: "layout" }>, sessionId: Id): React.CSSProperties {
  const index = layout.sessionIds.indexOf(sessionId);
  if (index < 0) return {};
  if (layout.mode === "grid") {
    const columns = Math.max(1, Math.ceil(Math.sqrt(Math.max(1, layout.sessionIds.length))));
    return {
      gridColumn: String((index % columns) + 1),
      gridRow: String(Math.floor(index / columns) + 1),
    };
  }
  const mainSessionId = layout.activeSessionId ?? layout.sessionIds[0];
  if (sessionId === mainSessionId) {
    return {
      gridColumn: "1",
      gridRow: "1 / -1",
    };
  }
  const secondaryIds = layout.sessionIds.filter((id) => id !== mainSessionId);
  const secondaryIndex = secondaryIds.indexOf(sessionId);
  const columns = Math.max(1, Math.ceil(Math.sqrt(Math.max(1, secondaryIds.length))));
  return {
    gridColumn: String(3 + (secondaryIndex % columns)),
    gridRow: String(Math.floor(secondaryIndex / columns) + 1),
  };
}

function blank(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
