import { expect, test, type Page } from "@playwright/test";

type Listener = (event: { payload: unknown }) => void;

const ids = {
  root: "00000000-0000-0000-0000-000000000001",
  edge: "00000000-0000-0000-0000-000000000002",
  prod: "00000000-0000-0000-0000-000000000003",
  staging: "00000000-0000-0000-0000-000000000004",
  shared: "00000000-0000-0000-0000-000000000005",
  bastion: "00000000-0000-0000-0000-000000000011",
  web: "00000000-0000-0000-0000-000000000012",
  db: "00000000-0000-0000-0000-000000000013",
  cache: "00000000-0000-0000-0000-000000000014",
  metrics: "00000000-0000-0000-0000-000000000016",
};

const folders = [
  { id: ids.root, parentId: null, name: "Root", path: "Root", hostCount: 6 },
  { id: ids.edge, parentId: ids.root, name: "Edge", path: "Root / Edge", hostCount: 1 },
  { id: ids.prod, parentId: ids.root, name: "Production", path: "Root / Production", hostCount: 3 },
  { id: ids.staging, parentId: ids.root, name: "Staging", path: "Root / Staging", hostCount: 1 },
  { id: ids.shared, parentId: ids.root, name: "Shared Services", path: "Root / Shared Services", hostCount: 1 },
];

const hosts = [
  host(ids.bastion, ids.edge, "Root / Edge / bastion-01", "bastion-01", "bastion.corp.example", "ops", [
    "edge",
    "jump",
  ]),
  host(ids.web, ids.prod, "Root / Production / web-prod-01", "web-prod-01", "web01.prod.corp.example", "deploy", [
    "prod",
    "web",
    "http",
  ]),
  host(ids.db, ids.prod, "Root / Production / db-prod-01", "db-prod-01", "db01.prod.corp.example", "dba", [
    "prod",
    "database",
  ]),
  host(ids.cache, ids.prod, "Root / Production / cache-prod-01", "cache-prod-01", "cache01.prod.corp.example", "ops", [
    "prod",
    "cache",
  ]),
  host(ids.metrics, ids.shared, "Root / Shared Services / metrics-01", "metrics-01", "metrics.shared.corp.example", "observer", [
    "shared",
    "metrics",
  ]),
];

const snapshot = {
  vaultPath: "simulation://vault.json",
  localConfigPath: "simulation://local.json",
  secretsPath: "simulation://secrets.json",
  folders,
  hosts,
  identities: [
    {
      fingerprint: "SHA256:sim-ops",
      path: "simulation://keys/ops_ed25519",
      preferredName: "ops simulation key",
      exists: true,
    },
    {
      fingerprint: "SHA256:sim-deploy",
      path: "simulation://keys/deploy_ed25519",
      preferredName: "deploy simulation key",
      exists: true,
    },
  ],
  secretsAvailable: true,
  diagnostics: [
    {
      severity: "warning",
      message: "Host db-prod-01 references missing identity mapping SHA256:sim-missing",
      hostId: ids.db,
    },
  ],
};

test.beforeEach(async ({ page }) => {
  await installTauriMock(page);
  await page.goto("/");
  await page.addStyleTag({
    content: `
      *, *::before, *::after { caret-color: transparent !important; }
      .xterm-cursor-layer, .xterm-cursor { display: none !important; }
    `,
  });
  await expect(page.getByTestId("app-shell")).toBeVisible();
});

test("captures the simulated terminal grid layout", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01", "cache-prod-01"]);

  await page.getByTestId("create-layout-tab").click();
  await expect(page.getByTestId("layout-toolbar")).toBeVisible();
  await expect(page.getByTestId("terminal-pane-web-prod-01")).toBeVisible();
  await expect(page.getByTestId("terminal-pane-db-prod-01")).toBeVisible();
  await expect(page.getByTestId("terminal-pane-cache-prod-01")).toBeVisible();

  await page.waitForTimeout(1_100);
  await expect(page.getByTestId("terminal-stage-panel")).toHaveScreenshot("simulation-grid-layout.png");
});

test("captures main-pane mode with broadcast input enabled", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01", "cache-prod-01"]);
  await page.getByTestId("create-layout-tab").click();
  await page.getByTestId("layout-main-mode").click();
  await page.getByTestId("layout-broadcast-toggle").click();
  await page.getByTestId("terminal-pane-web-prod-01").click();
  await page.keyboard.type("pwd");
  await page.keyboard.press("Enter");

  await expect(page.getByText("Broadcast")).toHaveClass(/active/);
  await page.waitForTimeout(1_100);
  await expect(page.getByTestId("terminal-stage-panel")).toHaveScreenshot("simulation-main-broadcast-layout.png");
});

test("captures terminal find and fullscreen pane states", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01"]);
  await page.getByTestId("create-layout-tab").click();
  await page.getByTestId("terminal-pane-web-prod-01").click();
  await page.getByTestId("terminal-find-button-web-prod-01").click();
  await page.getByTestId("terminal-find-input-web-prod-01").fill("simulation");
  await page.getByTestId("terminal-fullscreen-button-web-prod-01").click();

  await expect(page.getByTestId("terminal-pane-web-prod-01")).toHaveClass(/fullscreen/);
  await page.waitForTimeout(1_100);
  await expect(page.getByTestId("terminal-stage-panel")).toHaveScreenshot("simulation-find-fullscreen.png");
});

test("captures a layout created by dragging one terminal tab onto another", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01"]);

  await dragTabOnto(page, "web-prod-01", "db-prod-01");
  await expect(page.getByTestId("layout-toolbar")).toBeVisible();
  await page.waitForTimeout(1_100);
  await expect(page.getByTestId("terminal-stage-panel")).toHaveScreenshot("simulation-drag-created-layout.png");
});

test("preserves tab contents when layout tabs are reordered", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01"]);
  await page.getByTestId("create-layout-tab").click();
  await expect(page.getByTestId("tab-Layout 1")).toBeVisible();
  await page.getByTestId("tab-web-prod-01").click();
  await page.getByTestId("create-layout-tab").click();
  await expect(page.getByTestId("tab-Layout 2")).toBeVisible();

  await expect(tabTitles(page)).resolves.toEqual(["web-prod-01", "db-prod-01", "Layout 1", "Layout 2"]);
  await dragTabOnto(page, "Layout 2", "Layout 1");
  await expect(tabTitles(page)).resolves.toEqual(["web-prod-01", "db-prod-01", "Layout 2", "Layout 1"]);

  await page.getByTestId("tab-web-prod-01").click();
  await expect(page.getByTestId("terminal-pane-web-prod-01")).toContainText("stassh simulation mode");
  await page.getByTestId("tab-db-prod-01").click();
  await expect(page.getByTestId("terminal-pane-db-prod-01")).toContainText("stassh simulation mode");
});

test("preserves terminal scrollback across layout tab changes", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01"]);
  await page.getByTestId("tab-web-prod-01").click();
  await expect(page.getByTestId("terminal-pane-web-prod-01")).toContainText("stassh simulation mode");
  await page.getByTestId("terminal-pane-web-prod-01").click();

  const sentinel = "scrollback-sentinel-1842";
  const sessionId = await terminalSessionId(page, "web-prod-01");
  await page.evaluate(
    ({ sessionId, sentinel }) => {
      const noise = Array.from({ length: 48 }, (_, index) => `noise-line-${index}`).join("\r\n");
      window.dispatchEvent(new CustomEvent(`terminal-data:${sessionId}`, { detail: `${sentinel}\r\n${noise}\r\n` }));
    },
    { sessionId, sentinel },
  );

  await page.getByTestId("create-layout-tab").click();
  await expect(page.getByTestId("layout-toolbar")).toBeVisible();
  await page.getByTestId("tab-db-prod-01").click();
  await expect(page.getByTestId("terminal-pane-db-prod-01")).toBeVisible();
  await page.getByTestId("tab-web-prod-01").click();
  await page.getByTestId("terminal-find-button-web-prod-01").click();
  await page.getByTestId("terminal-find-input-web-prod-01").fill(sentinel);

  await expect(page.getByTestId("terminal-pane-web-prod-01").locator(".terminalFindCount")).toHaveText("Match");
});

test("closes a selected exited terminal when Enter is pressed", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01", "db-prod-01"]);
  await page.getByTestId("tab-web-prod-01").click();
  await page.getByTestId("terminal-pane-web-prod-01").click();
  const sessionId = await terminalSessionId(page, "web-prod-01");

  await page.evaluate((sessionId) => {
    window.__STASSH_TEST_API__?.emit("session-exit", { sessionId, message: "EXITED" });
  }, sessionId);

  await expect(page.getByTestId("tab-web-prod-01")).toHaveClass(/terminalExited/);
  await page.keyboard.press("Enter");

  await expect(page.getByTestId("tab-web-prod-01")).toHaveCount(0);
  await expect(tabTitles(page)).resolves.toEqual(["db-prod-01"]);
});

test("removes a layout tab after its last terminal is closed", async ({ page }) => {
  await openSimulationTerminals(page, ["web-prod-01"]);
  await page.getByTestId("create-layout-tab").click();
  await expect(page.getByTestId("tab-Layout 1")).toBeVisible();
  await page.getByTestId("terminal-pane-web-prod-01").click();
  const sessionId = await terminalSessionId(page, "web-prod-01");

  await page.evaluate((sessionId) => {
    window.__STASSH_TEST_API__?.emit("session-exit", { sessionId, message: "EXITED" });
  }, sessionId);

  await page.keyboard.press("Enter");

  await expect(page.getByTestId("tab-web-prod-01")).toHaveCount(0);
  await expect(page.getByTestId("tab-Layout 1")).toHaveCount(0);
  await expect(tabTitles(page)).resolves.toEqual([]);
});

async function installTauriMock(page: Page) {
  await page.addInitScript(({ snapshot }) => {
    const listeners = new Map<string, Set<Listener>>();
    const sessionHostById = new Map<string, string>();
    let sessionIndex = 0;

    function emit(eventName: string, payload: unknown) {
      for (const listener of listeners.get(eventName) ?? []) {
        listener({ payload });
      }
    }

    function findHost(hostId: string) {
      const host = snapshot.hosts.find((item) => item.id === hostId);
      if (!host) throw new Error(`unknown host: ${hostId}`);
      return host;
    }

    window.__STASSH_TEST_API__ = {
      emit,
      invoke: async (command: string, args?: Record<string, unknown>) => {
        if (command === "load_workspace" || command === "reload_workspace") return snapshot;
        if (command === "host_details") {
          const host = findHost(String(args?.hostId));
          return {
            host,
            jumps: host.jumpChain.map((id) => findHost(id)),
            sshCommand: `ssh ${host.username}@${host.hostname}`,
            diagnostics: snapshot.diagnostics.filter((item) => item.hostId === host.id),
          };
        }
        if (command === "search_hosts") {
          const query = String(args?.query ?? "").toLowerCase();
          return snapshot.hosts
            .filter((item) => item.path.toLowerCase().includes(query) || item.hostname.toLowerCase().includes(query))
            .map((item) => ({
              id: item.id,
              path: item.path,
              target: `${item.hostname}:${item.port}`,
              username: item.username,
              tags: item.tags,
            }));
        }
        if (command === "start_ssh_session") {
          const host = findHost(String(args?.hostId));
          const sessionId = `session-${++sessionIndex}-${host.displayName}`;
          sessionHostById.set(sessionId, host.displayName);
          return {
            sessionId,
            initialOutput:
              `Connecting to ${host.path} (${host.hostname})...\\r\\n` +
              "stassh simulation mode: no real SSH connection is active.\\r\\n" +
              "Authorized demo environment for stassh simulation.\\r\\n" +
              "No real network connection is active.\\r\\n\\r\\n" +
              `${host.username ?? "user"}@${host.hostname}:/home/demo$ `,
          };
        }
        if (command === "write_terminal") {
          const sessionId = String(args?.sessionId);
          const displayName = sessionHostById.get(sessionId) ?? "host";
          const data = String(args?.data ?? "");
          emit("session-output", {
            sessionId,
            data:
              data.includes("\\r") || data.includes("\\n")
                ? `\\r\\n/home/demo\\r\\n${displayName}: simulated command complete\\r\\n$ `
                : data,
          });
          return null;
        }
        if (command === "resize_terminal" || command === "close_session") return null;
        throw new Error(`unhandled test command: ${command}`);
      },
      listen: async (eventName: string, listener: Listener) => {
        const set = listeners.get(eventName) ?? new Set<Listener>();
        set.add(listener);
        listeners.set(eventName, set);
        return () => set.delete(listener);
      },
    };
  }, { snapshot });
}

async function dragTabOnto(page: Page, sourceTitle: string, targetTitle: string) {
  const source = page.getByTestId(`tab-${sourceTitle}`).locator("span").first();
  const target = page.getByTestId(`tab-${targetTitle}`).locator("span").first();
  const sourceBox = await source.boundingBox();
  const targetBox = await target.boundingBox();
  if (!sourceBox || !targetBox) throw new Error("tab drag source or target not visible");
  const sourceX = sourceBox.x + sourceBox.width / 2;
  const sourceY = sourceBox.y + sourceBox.height / 2;
  const targetX = targetBox.x + targetBox.width / 2;
  const targetY = targetBox.y + targetBox.height / 2;

  await page.mouse.move(sourceX, sourceY);
  await page.mouse.down();
  await page.mouse.move(sourceX + 12, sourceY, { steps: 4 });
  await page.mouse.move(targetX, targetY, { steps: 12 });
  await page.mouse.up();
}

async function tabTitles(page: Page) {
  return page.getByTestId("tabbar").locator('button[data-testid^="tab-"]').evaluateAll((buttons) =>
    buttons.map((button) => {
      const testId = button.getAttribute("data-testid") ?? "";
      const title = testId.replace(/^tab-/, "");
      if (!title) throw new Error("tab title not found");
      return title;
    }),
  );
}

async function terminalSessionId(page: Page, title: string) {
  const sessionId = await page.getByTestId(`tab-${title}`).getAttribute("data-tab-id");
  if (!sessionId) throw new Error(`terminal session id not found for ${title}`);
  return sessionId;
}

async function openSimulationTerminals(page: Page, names: string[]) {
  await page.getByTestId("folder-row-Production").locator("button").click();
  for (const name of names) {
    await page.getByTestId(`host-row-${name}`).dblclick();
    await expect(page.getByTestId(`tab-${name}`)).toBeVisible();
  }
}

function host(
  id: string,
  folderId: string,
  path: string,
  displayName: string,
  hostname: string,
  username: string,
  tags: string[],
) {
  return {
    id,
    folderId,
    path,
    displayName,
    hostname,
    port: displayName === "db-prod-01" ? 2222 : 22,
    username,
    identityFingerprint: displayName === "db-prod-01" ? "SHA256:sim-missing" : "SHA256:sim-deploy",
    secrets: displayName.includes("prod") ? `${displayName}-secrets` : null,
    jumpChain: displayName.endsWith("prod-01") ? [ids.bastion] : [],
    forwards: [],
    tags,
    notes:
      displayName === "web-prod-01"
        ? "Blue pool frontend. Use action palette for common checks."
        : "Safe target for screenshots and workflow demos.",
    actionCount: displayName === "web-prod-01" ? 2 : 0,
  };
}
