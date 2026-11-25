import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  ArchiveRestore,
  CheckCircle2,
  ChevronRight,
  Database,
  EyeOff,
  Gamepad2,
  HardDrive,
  History,
  LibraryBig,
  LoaderCircle,
  Monitor,
  RotateCcw,
  Search,
  Settings2,
  SlidersHorizontal,
  Star,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react";
import {
  call,
  date,
  shortPath,
  type Decision,
  type Game,
  type Outcome,
  type Profile,
} from "./api";
import { Modal, ProfileForm, Setup } from "./components";
import { Library, type Scope } from "./Library";
import { Detail } from "./Detail";
import "./style.css";
import logo from "./logo.png";
import { presets, parseTheme, applyTheme, type Theme } from "./themes";
type Page = Scope | "Recovery" | "Settings";
type Dialog =
  | { type: "setup"; game?: Game }
  | { type: "form"; p?: Profile; duplicate?: boolean }
  | { type: "delete" | "restore"; p: Profile }
  | { type: "dirty"; g: Game; p: Profile; token: string; files: string[] };
function App() {
  const [page, setPage] = useState<Page>("Library"),
    [games, setGames] = useState<Game[]>([]),
    [id, setId] = useState<string | null>(null);
  const [profiles, setProfiles] = useState<Profile[]>([]),
    [backups, setBackups] = useState<Profile[]>([]),
    [changes, setChanges] = useState<string[]>([]);
  const [dialog, setDialog] = useState<Dialog | null>(null),
    [busy, setBusy] = useState(false),
    [err, setErr] = useState(""),
    [toast, setToast] = useState("");
  const [query, setQuery] = useState(""),
    [retention, setRetention] = useState(20),
    [size, setSize] = useState(localStorage.getItem("covers") || "medium");
  const [theme, setTheme] = useState(localStorage.getItem("theme") || "dark"),
    [report, setReport] = useState<string[]>([]);
  const [custom, setCustom] = useState<Theme | null>(() => {
    try {
      return parseTheme(localStorage.getItem("customTheme") || "null");
    } catch {
      return null;
    }
  });
  const [themeJson, setThemeJson] = useState("");
  const [removedGames, setRemovedGames] = useState<Game[]>([]);
  useEffect(() => {
    if (page === "Settings")
      void call<Game[]>("removed")
        .then(setRemovedGames)
        .catch((e) => setErr(String(e)));
  }, [page]);
  const current = useRef<string | null>(null),
    locked = useRef(false),
    search = useRef<HTMLInputElement>(null);
  const switchQueue = useRef<{ game: Game; profile: Profile }[]>([]);
  const g = games.find((g) => g.id === id);
  const refreshGames = useCallback(
    async () => setGames(await call<Game[]>("library")),
    [],
  );
  const refreshDetail = useCallback(async (game: string) => {
    const data = await call<Profile[]>("detail", { game });
    if (current.current === game) setProfiles(data);
    const dirty = await call<string[]>("changes", { game });
    if (current.current === game) setChanges(dirty);
  }, []);
  async function reload() {
    await refreshGames();
    if (current.current) await refreshDetail(current.current);
    setBackups(await call<Profile[]>("recovery"));
  }
  const run = useCallback(async (fn: () => Promise<void>) => {
    if (locked.current) return;
    locked.current = true;
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      switchQueue.current = [];
      setErr(String(e));
    } finally {
      locked.current = false;
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void run(async () => {
      setReport(await call<string[]>("recover"));
      await refreshGames();
      setBackups(await call<Profile[]>("recovery"));
      setRetention(await call<number>("retention"));
      await call("scan");
      await refreshGames();
    });
  }, [refreshGames, run]);
  useEffect(() => {
    const chosen =
      theme === "custom" && custom ? custom : presets[theme] || presets.dark;
    applyTheme(chosen);
    void call("windowTheme", {
      light: chosen.mode === "light",
      caption: chosen.colors.side,
      foreground: chosen.colors.ink,
    }).catch(() => {});
    localStorage.setItem("theme", theme);
  }, [theme, custom]);
  useEffect(() => {
    if (toast) {
      const t = setTimeout(() => setToast(""), 4200);
      return () => clearTimeout(t);
    }
  }, [toast]);
  useEffect(() => {
    let cancelled = false,
      timer: ReturnType<typeof setTimeout> | undefined,
      unlisten: (() => void) | undefined;
    if (id) {
      const check = async (retry = true) => {
        try {
          const next = await call<string[]>("changes", { game: id });
          if (!cancelled) setChanges(next);
        } catch (e) {
          if (!cancelled) {
            if (retry) timer = setTimeout(() => void check(false), 750);
            else setErr(String(e));
          }
        }
      };
      listen<string>("managed-files-changed", (e) => {
        if (e.payload === id && !cancelled) {
          clearTimeout(timer);
          timer = setTimeout(() => void check(), 350);
        }
      })
        .then((f) => {
          if (cancelled) f();
          else unlisten = f;
        })
        .catch((e) => {
          if (!cancelled) setErr(String(e));
        });
      call("watch", { game: id })
        .then(() => {
          if (!cancelled) void check();
        })
        .catch((e) => {
          if (!cancelled) setErr(String(e));
        });
    } else call("watch", { game: null }).catch(() => {});
    return () => {
      cancelled = true;
      clearTimeout(timer);
      unlisten?.();
    };
  }, [id, g?.maps.length]);
  function navigate(p: Page) {
    current.current = null;
    setId(null);
    setPage(p);
    setQuery("");
    setDialog(null);
  }
  async function enter(game: string) {
    current.current = game;
    setProfiles([]);
    setChanges([]);
    setId(game);
    setPage("Library");
    await refreshDetail(game);
  }
  async function activate(
    game: Game,
    p: Profile,
    decision: Decision = "check",
    token?: string,
  ) {
    const out = await call<Outcome>("activate", {
      game: game.id,
      profile: p.id,
      decision,
      token,
    });
    if (out.status === "unsaved") {
      setDialog({
        type: "dirty",
        g: game,
        p,
        token: out.token,
        files: out.files,
      });
      return;
    }
    setDialog(null);
    await reload();
    setToast(
      p.recovery ? "Recovery point restored." : "Activated " + p.name + ".",
    );
    const next = switchQueue.current.shift();
    if (next) await activate(next.game, next.profile);
  }
  function keyboard(e: KeyboardEvent) {
    if (document.querySelector("dialog[open]") || busy) return;
    if (e.ctrlKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      navigate("Library");
      search.current?.focus();
    }
    if (e.ctrlKey && e.key.toLowerCase() === "n" && g?.maps.length) {
      e.preventDefault();
      setDialog({ type: "form" });
    }
    if (e.ctrlKey && e.key.toLowerCase() === "s" && g?.active) {
      e.preventDefault();
      void run(async () => {
        await call("save", { game: g.id });
        await reload();
        setToast("Active profile saved.");
      });
    }
    if (e.altKey && e.key === "ArrowLeft" && id) {
      e.preventDefault();
      navigate("Library");
    }
  }
  useEffect(() => {
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  });
  const nav = (p: Page, Icon: typeof History, count?: number) => (
    <button
      key={p}
      aria-current={page === p ? "page" : undefined}
      onClick={() => navigate(p)}
    >
      <Icon />
      {p === "Library" ? "Game library" : p}
      {count !== undefined && <small>{count}</small>}
    </button>
  );
  return (
    <div className="shell">
      <aside>
        <div className="brand">
          <span>
            <img src={logo} alt="" />
          </span>
          Switchboard
        </div>
        <nav aria-label="Collection">
          <h4>Workspace</h4>
          {nav("Library", LibraryBig, games.filter((g) => !g.hidden).length)}
          {nav("Hidden", EyeOff)}
        </nav>
        <nav aria-label="Data management">
          <h4>Management</h4>
          {nav("Recovery", ArchiveRestore)}
          {nav("Settings", Settings2)}
        </nav>
      </aside>
      <main>
        <header>
          <div className="crumb">
            <Monitor />
            {page === "Library" ? "Library" : page}
            {g && (
              <>
                <ChevronRight />
                {g.name}
              </>
            )}
          </div>
          <div className="search">
            <Search />
            <input
              ref={search}
              aria-label="Search library"
              placeholder="Search games…"
              value={query}
              onChange={(e) => {
                if (id || page === "Recovery" || page === "Settings")
                  navigate("Library");
                setQuery(e.target.value);
              }}
            />
            <kbd>Ctrl F</kbd>
          </div>
          <button
            className="icon"
            title="Settings"
            aria-label="Open settings"
            onClick={() => navigate("Settings")}
          >
            <Settings2 />
          </button>
        </header>
        <div className="content" aria-busy={busy}>
          {busy && (
            <div className="progress" role="status" aria-label="Working" />
          )}
          {!!report.length && (
            <div className="change-bar">
              <div>
                {report.map((s) => (
                  <p key={s}>{s}</p>
                ))}
              </div>
              <button onClick={() => setReport([])}>Dismiss</button>
            </div>
          )}
          {g ? (
            <Detail
              key={g.id}
              game={g}
              profiles={profiles}
              bulk={async (pairs) => {
                switchQueue.current = pairs;
                const first = switchQueue.current.shift();
                if (first) await activate(first.game, first.profile);
              }}
              changes={changes}
              busy={busy}
              back={() => navigate("Library")}
              setup={() => setDialog({ type: "setup", game: g })}
              form={(p, duplicate) => setDialog({ type: "form", p, duplicate })}
              activate={(p) => void run(() => activate(g, p))}
              remove={(p) => setDialog({ type: "delete", p })}
              run={run}
              reload={reload}
              notice={setToast}
            />
          ) : page === "Recovery" ? (
            <>
              <div className="page-head">
                <div>
                  <h1>Recovery points</h1>
                </div>
                <button
                  onClick={() =>
                    void run(async () => {
                      const r = await call<string[]>("recover");
                      setReport(r);
                      await reload();
                      if (!r.length) setToast("No interrupted switches found.");
                    })
                  }
                >
                  <RotateCcw />
                  Retry recovery
                </button>
              </div>
              <p className="intro">
                Restore the managed files captured before a configuration
                switch.
              </p>
              {backups.length ? (
                <div className="recovery-list">
                  {backups.map((p) => (
                    <article key={p.id}>
                      <span className="profile-icon">
                        <History />
                      </span>
                      <div>
                        <small>
                          {games.find((g) => g.id === p.game)?.name}
                        </small>
                        <h3>{p.name}</h3>
                        <small>
                          {date(p.created)} · {p.count} files
                        </small>
                      </div>
                      <button onClick={() => setDialog({ type: "restore", p })}>
                        <RotateCcw />
                        Restore
                      </button>
                      <button
                        className="icon danger"
                        aria-label={"Delete " + p.name}
                        onClick={() => setDialog({ type: "delete", p })}
                      >
                        <Trash2 />
                      </button>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="empty">
                  <div className="empty-art">
                    <History />
                  </div>
                  <h2>No recovery points yet</h2>
                  <p>
                    Your first configuration switch creates one automatically.
                  </p>
                </div>
              )}
            </>
          ) : page === "Settings" ? (
            <>
              <div className="page-head">
                <div>
                  <h1>Settings</h1>
                </div>
              </div>
              <section className="settings">
                <label>
                  <span>
                    <b>Cover size</b>
                    <small>Artwork size in the library grid</small>
                  </span>
                  <select
                    value={size}
                    onChange={(e) => {
                      setSize(e.target.value);
                      localStorage.setItem("covers", e.target.value);
                    }}
                  >
                    {["small", "medium", "large"].map((v) => (
                      <option key={v}>{v}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>
                    <b>Appearance</b>
                    <small>Color scheme for this workspace</small>
                  </span>
                  <select
                    value={theme}
                    onChange={(e) => setTheme(e.target.value)}
                  >
                    {Object.entries(presets).map(([id, t]) => (
                      <option key={id} value={id}>
                        {t.name}
                      </option>
                    ))}
                    {custom && <option value="custom">{custom.name}</option>}
                  </select>
                </label>
                <label>
                  <span>
                    <b>Recovery points per game</b>
                    <small>
                      Older points are pruned after the next successful switch.
                    </small>
                  </span>
                  <select
                    value={retention}
                    onChange={(e) => {
                      const v = Number(e.target.value);
                      void run(async () =>
                        setRetention(await call("retention", { value: v })),
                      );
                    }}
                  >
                    {[5, 10, 20, 50, 100].map((v) => (
                      <option key={v}>{v}</option>
                    ))}
                  </select>
                </label>
              </section>
              {!!removedGames.length && (
                <details className="theme-import">
                  <summary>Removed games</summary>
                  {removedGames.map((g) => (
                    <div className="spread" key={g.id}>
                      <span>{g.name}</span>
                      <button
                        onClick={() =>
                          void run(async () => {
                            await call("flag", { game: g.id, flag: "restore" });
                            setRemovedGames(
                              removedGames.filter((x) => x.id !== g.id),
                            );
                            await refreshGames();
                          })
                        }
                      >
                        Restore to library
                      </button>
                    </div>
                  ))}
                </details>
              )}
              <details className="theme-import">
                <summary>Custom JSON theme</summary>
                <p>
                  Paste a theme or open a JSON file. Start with the current
                  preset and edit its colors.
                </p>
                <button
                  onClick={() =>
                    setThemeJson(
                      JSON.stringify(
                        presets[theme] || custom || presets.dark,
                        null,
                        2,
                      ),
                    )
                  }
                >
                  Load current theme JSON
                </button>
                <input
                  type="file"
                  accept=".json,application/json"
                  aria-label="Import theme JSON"
                  onChange={(e) => {
                    const f = e.target.files?.[0];
                    if (f)
                      void run(async () => {
                        if (f.size > 16384)
                          throw Error("Theme file exceeds 16 KiB.");
                        setThemeJson(await f.text());
                      });
                  }}
                />
                <textarea
                  aria-label="Theme JSON"
                  rows={9}
                  value={themeJson}
                  onChange={(e) => setThemeJson(e.target.value)}
                />
                <button
                  onClick={() =>
                    void run(async () => {
                      const t = parseTheme(themeJson);
                      localStorage.setItem("customTheme", JSON.stringify(t));
                      setCustom(t);
                      setTheme("custom");
                    })
                  }
                >
                  Apply custom theme
                </button>
              </details>
              <button
                onClick={() =>
                  void run(async () => {
                    await call("prismFolder");
                    await refreshGames();
                    setToast("Prism instances added.");
                  })
                }
              >
                Add Prism Launcher data folder…
              </button>
              <div className="note">
                <Database />
                <p>
                  Profiles and recovery points are stored on this PC. Launcher
                  cloud sync may overwrite a switched profile. If this happens,
                  temporarily turn off settings sync in the launcher after
                  checking which copy you want to keep.
                </p>
              </div>
            </>
          ) : (
            <Library
              games={games}
              scope={page}
              query={query}
              open={(g) => void run(() => enter(g.id))}
              add={() => setDialog({ type: "setup" })}
              scan={() =>
                void run(async () => {
                  const n = await call<number>("scan");
                  await refreshGames();
                  setToast(
                    n
                      ? "Added " + n + " games."
                      : "Library scan complete. No new games found.",
                  );
                })
              }
              run={run}
              refresh={refreshGames}
              quick={(g, p) => void run(() => activate(g, p))}
              busy={busy}
              size={size}
            />
          )}
        </div>
      </main>
      {toast && (
        <div className="toast" role="status">
          <CheckCircle2 />
          {toast}
          <button
            className="icon"
            aria-label="Dismiss notification"
            onClick={() => setToast("")}
          >
            <X />
          </button>
        </div>
      )}
      {dialog?.type === "setup" && (
        <Setup
          game={dialog.game}
          close={() => setDialog(null)}
          busy={busy}
          run={run}
          done={async (id) => {
            setDialog(null);
            await refreshGames();
            await enter(id);
            setToast("Game configuration is ready.");
          }}
        />
      )}
      {dialog?.type === "form" && g && (
        <ProfileForm
          profile={dialog.p}
          duplicate={dialog.duplicate}
          busy={busy}
          close={() => setDialog(null)}
          submit={(name, description, tags) =>
            void run(async () => {
              const edit = dialog.p && !dialog.duplicate;
              await call(edit ? "edit" : "create", {
                game: g.id,
                profile: dialog.p?.id,
                source: dialog.duplicate ? dialog.p?.id : null,
                name,
                description,
                tags,
              });
              setDialog(null);
              await reload();
              setToast(edit ? "Profile updated." : "Configuration created.");
            })
          }
        />
      )}
      {dialog?.type === "dirty" && (
        <Modal
          title="Unsaved Configuration Changes"
          busy={busy}
          close={() => {
            switchQueue.current = [];
            setDialog(null);
          }}
        >
          <p>
            <strong>{dialog.g.name}</strong>
          </p>
          <p>
            Your current game settings have changed since this configuration was
            last saved. What would you like to do before switching?
          </p>
          <details>
            <summary>View changed files ({dialog.files.length})</summary>
            <ul className="paths">
              {dialog.files.map((f) => (
                <li key={f}>{shortPath(f)}</li>
              ))}
            </ul>
          </details>
          <p className="hint">
            Switchboard will keep a recovery point before replacing these files.
          </p>
          <div className="modal-actions wrap">
            <button
              disabled={busy}
              onClick={() => {
                switchQueue.current = [];
                setDialog(null);
              }}
            >
              Cancel
            </button>
            <button
              className="danger"
              disabled={busy}
              onClick={() =>
                void run(() =>
                  activate(dialog.g, dialog.p, "discard", dialog.token),
                )
              }
            >
              {dialog.p.id === dialog.g.active
                ? "Discard & revert"
                : "Discard Changes & Switch"}
            </button>
            {dialog.p.id !== dialog.g.active && (
              <button
                className="primary"
                disabled={busy}
                onClick={() =>
                  void run(() =>
                    activate(dialog.g, dialog.p, "save", dialog.token),
                  )
                }
              >
                Save &amp; Switch
              </button>
            )}
          </div>
        </Modal>
      )}
      {(dialog?.type === "delete" || dialog?.type === "restore") && (
        <Modal
          busy={busy}
          title={
            dialog.type === "delete"
              ? "Delete " +
                (dialog.p.recovery ? "recovery point" : "profile") +
                "?"
              : "Restore recovery point?"
          }
          close={() => setDialog(null)}
        >
          <p>
            {dialog.type === "delete"
              ? "Permanently delete “" +
                dialog.p.name +
                "” from this workspace?"
              : "Replace the game’s managed files with “" +
                dialog.p.name +
                "”? A new recovery point will preserve the current files."}
          </p>
          <div className="modal-actions">
            <button onClick={() => setDialog(null)}>Cancel</button>
            <button
              className={dialog.type === "delete" ? "danger" : "primary"}
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  if (dialog.type === "restore") {
                    const game = games.find((g) => g.id === dialog.p.game);
                    if (game) await activate(game, dialog.p);
                  } else {
                    await call("delete", {
                      game: dialog.p.game,
                      profile: dialog.p.id,
                    });
                    setDialog(null);
                    await reload();
                    setToast("Deleted successfully.");
                  }
                })
              }
            >
              {dialog.type === "delete"
                ? "Delete permanently"
                : "Restore files"}
            </button>
          </div>
        </Modal>
      )}
      {err && (
        <Modal title="Operation needs attention" close={() => setErr("")}>
          <div className="error">
            <TriangleAlert />
            <p>{err}</p>
          </div>
          <div className="modal-actions">
            <button autoFocus onClick={() => setErr("")}>
              Close
            </button>
          </div>
        </Modal>
      )}
      {busy && (
        <span className="sr-only" role="status">
          <LoaderCircle />
          Processing request
        </span>
      )}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<App />);
