import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowLeft,
  ArrowRightLeft,
  Check,
  ChevronDown,
  Clock3,
  Copy,
  Download,
  FileCog,
  FolderOpen,
  GitCompareArrows,
  Layers3,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  Trash2,
  Upload,
} from "lucide-react";
import { Cover, Modal, ContextMenu } from "./components";
import {
  call,
  date,
  pick,
  shortPath,
  type Game,
  type Profile,
  type Run,
} from "./api";
type Props = {
  game: Game;
  profiles: Profile[];
  changes: string[];
  back: () => void;
  setup: () => void;
  form: (p?: Profile, duplicate?: boolean) => void;
  activate: (p: Profile) => void;
  remove: (p: Profile) => void;
  run: Run;
  reload: () => Promise<void>;
  notice: (s: string) => void;
  busy: boolean;
  bulk: (pairs: { game: Game; profile: Profile }[]) => Promise<void>;
};
export function Detail({
  game: g,
  profiles,
  changes,
  back,
  setup,
  form,
  activate,
  remove,
  run,
  reload,
  notice,
  busy,
  bulk,
}: Props) {
  const [menu, setMenu] = useState<Profile | null>(null),
    [showFiles, setShowFiles] = useState(false),
    [importPath, setImportPath] = useState(""),
    [compare, setCompare] = useState(false),
    [first, setFirst] = useState(""),
    [second, setSecond] = useState(""),
    [diff, setDiff] = useState<
      {
        file: string;
        state: string;
        lines: { added: boolean; number: number; text: string }[];
        more: boolean;
      }[]
    >([]);
  const [point, setPoint] = useState({ x: 0, y: 0 });
  const [sharing, setSharing] = useState<Profile | null>(null),
    [peers, setPeers] = useState<Game[]>([]),
    [selected, setSelected] = useState<string[]>([]);
  const [activateCopies, setActivateCopies] = useState(true);
  const active = profiles.find((p) => p.id === g.active);
  const [importing, setImporting] = useState(false),
    [sources, setSources] = useState<string[]>([]),
    [importName, setImportName] = useState("");
  useEffect(() => {
    let closed = false;
    const sub = listen<string>("profile-archive-dropped", (e) => {
      if (!closed && g.maps.length && !document.querySelector("dialog[open]"))
        setImportPath(e.payload);
    });
    return () => {
      closed = true;
      void sub.then((off) => off()).catch(() => {});
    };
  }, [g.id, g.maps.length]);
  const act = (kind: string, p: Profile, msg: string) =>
    void run(async () => {
      const result = await call<boolean | null>(kind, {
        game: g.id,
        profile: p.id,
      });
      setMenu(null);
      await reload();
      if (result !== false) notice(msg);
    });
  return (
    <>
      <button className="back" onClick={back}>
        <ArrowLeft />
        Game library
      </button>
      <div className="detail-head">
        <div className="detail-art">
          <Cover game={g} />
        </div>
        <div>
          <div className="eyebrow">{g.platform}</div>
          <h1>{g.name}</h1>
          <span className={"status " + (active ? "managed" : "")}>
            <i />
            {active ? active.name : "No active profile"}
          </span>
        </div>
        {g.maps.length > 0 && (
          <div className="file-bar">
            <button
              onClick={() => setShowFiles(!showFiles)}
              aria-expanded={showFiles}
            >
              <FileCog />
              {g.maps.length} managed {g.maps.length === 1 ? "file" : "files"}
              <ChevronDown />
            </button>
            <button
              className="icon"
              aria-label="Open live configuration folder"
              onClick={() =>
                void run(() => call("openFolder", { game: g.id, slot: 0 }))
              }
            >
              <FolderOpen />
            </button>
          </div>
        )}
        {g.id.startsWith("steam-") && (
          <button
            className="launch"
            onClick={() => void run(() => call("launch", { game: g.id }))}
          >
            Launch in Steam
          </button>
        )}
      </div>
      {!localStorage.getItem("cloudDismissed") && (
        <div className="cloud-notice">
          Launcher cloud sync may replace locally switched settings. If that
          happens, pause settings sync in your launcher after choosing which
          copy to keep.
          <button
            onClick={(e) => {
              localStorage.setItem("cloudDismissed", "1");
              e.currentTarget.parentElement?.remove();
            }}
          >
            Dismiss
          </button>
        </div>
      )}
      {showFiles && (
        <ul className="path-list">
          {g.maps.map((m, i) => (
            <li key={i}>
              {shortPath(m.root)}\{m.name}
              <button
                className="icon"
                title="Open containing folder"
                aria-label={"Open folder for " + m.name}
                onClick={() =>
                  void run(() => call("openFolder", { game: g.id, slot: i }))
                }
              >
                <FolderOpen />
              </button>
            </li>
          ))}
        </ul>
      )}
      {!!changes.length && (
        <div className="change-bar">
          <div>
            <strong>Unsaved Changes</strong>
            <details>
              <summary>
                View {changes.length} changed{" "}
                {changes.length === 1 ? "file" : "files"}
              </summary>
              {changes.map((f) => (
                <div key={f}>{shortPath(f)}</div>
              ))}
            </details>
          </div>
          <div className="row">
            <button disabled={busy} onClick={() => active && activate(active)}>
              <RotateCcw />
              Revert
            </button>
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await call("save", { game: g.id });
                  await reload();
                  notice("Live changes saved to the active profile.");
                })
              }
            >
              <Save />
              Save changes
            </button>
          </div>
        </div>
      )}
      {g.maps.length ? (
        <>
          <div className="section-head">
            <div>
              <h2>
                Configurations <span className="count">{profiles.length}</span>
              </h2>
            </div>
            <div className="row">
              {profiles.length > 1 && (
                <button
                  title="Compare profiles"
                  aria-label="Compare profiles"
                  className="icon"
                  onClick={() => {
                    setFirst(profiles[0].id);
                    setSecond(profiles[1].id);
                    setDiff([]);
                    setCompare(true);
                  }}
                >
                  <GitCompareArrows />
                </button>
              )}
              <button
                onClick={() => {
                  setSources(g.maps.map(() => ""));
                  setImportName("");
                  setImporting(true);
                }}
              >
                <Download />
                Import
              </button>
              <button className="primary" onClick={() => form()}>
                <Plus />
                Create configuration
              </button>
            </div>
          </div>
          {!profiles.length && (
            <p className="hint">
              Capture your current settings to create the first profile.
            </p>
          )}
          <div className="profile-grid">
            {profiles.map((p) => (
              <article
                key={p.id}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setPoint({ x: e.clientX, y: e.clientY });
                  setMenu(p);
                }}
                className={
                  "profile-card " + (p.id === g.active ? "active" : "")
                }
              >
                {p.thumbnail && (
                  <img className="profile-thumbnail" src={p.thumbnail} alt="" />
                )}
                <div className="spread">
                  <h3>{p.name}</h3>
                  <button
                    className="icon"
                    aria-label={"Options for " + p.name}
                    onClick={(e) => {
                      setPoint({ x: e.clientX, y: e.clientY });
                      setMenu(p);
                    }}
                  >
                    <MoreHorizontal />
                  </button>
                </div>
                {p.description && <p>{p.description}</p>}
                <dl className="overview">
                  {p.overview?.map((v) => (
                    <div key={v.label}>
                      <dt>{v.label}</dt>
                      <dd>{v.value}</dd>
                    </div>
                  ))}
                </dl>
                <div className="tags">
                  {p.tags.map((t, i) => (
                    <span key={i}>{t}</span>
                  ))}
                </div>
                <small title={"Created " + date(p.created)}>
                  <Clock3 size={13} />
                  {date(p.modified)}
                </small>
                <small>
                  <FileCog size={13} />
                  {p.count} managed {p.count === 1 ? "file" : "files"}
                </small>
                <button
                  className={
                    p.id === g.active ? "active-button" : "switch-button"
                  }
                  disabled={busy || p.id === g.active}
                  onClick={() => activate(p)}
                >
                  {p.id === g.active ? (
                    <>
                      <Check />
                      Active profile
                    </>
                  ) : (
                    <>
                      <ArrowRightLeft />
                      Switch to profile
                    </>
                  )}
                </button>
              </article>
            ))}
          </div>
        </>
      ) : (
        <div className="empty">
          <div className="empty-art">
            <FileCog />
          </div>
          <h2>Choose the settings to manage</h2>
          <p>
            Select this game’s executable and configuration files to start
            creating profiles.
          </p>
          <button className="primary" onClick={setup}>
            Set up game
          </button>
        </div>
      )}
      {menu && (
        <ContextMenu point={point} close={() => setMenu(null)}>
          <div className="menu-list">
            <button
              onClick={() =>
                void run(async () => {
                  const [path] = await pick("cover", g.id);
                  if (path) {
                    await call("thumbnail", {
                      game: g.id,
                      profile: menu.id,
                      path,
                    });
                    setMenu(null);
                    await reload();
                  }
                })
              }
            >
              Choose PNG thumbnail…
            </button>
            {menu.thumbnail && (
              <button
                onClick={() =>
                  void run(async () => {
                    await call("thumbnail", {
                      game: g.id,
                      profile: menu.id,
                      path: null,
                    });
                    setMenu(null);
                    await reload();
                  })
                }
              >
                Remove thumbnail
              </button>
            )}
            {g.platform === "Prism Launcher" && (
              <button
                onClick={() =>
                  void run(async () => {
                    const games = await call<Game[]>("library");
                    setPeers(
                      games.filter(
                        (x) => x.id !== g.id && x.platform === "Prism Launcher",
                      ),
                    );
                    setSharing(menu);
                    setSelected([]);
                    setMenu(null);
                  })
                }
              >
                Copy to other instances…
              </button>
            )}
            <button
              onClick={() => {
                form(menu);
                setMenu(null);
              }}
            >
              <Pencil />
              Rename or edit
            </button>
            <button
              onClick={() => {
                form(menu, true);
                setMenu(null);
              }}
            >
              <Copy />
              Duplicate profile
            </button>
            <button
              onClick={() =>
                act("openProfile", menu, "Opened the profile’s editable files.")
              }
            >
              <FolderOpen />
              Open profile folder
            </button>
            <button
              onClick={() =>
                act(
                  "saveProfileFiles",
                  menu,
                  "Saved edits from the profile folder.",
                )
              }
            >
              <Save />
              Save edits from folder
            </button>
            <button onClick={() => act("export", menu, "Profile exported.")}>
              <Upload />
              Export archive
            </button>
            <button
              className="danger"
              disabled={menu.id === g.active}
              onClick={() => {
                remove(menu);
                setMenu(null);
              }}
            >
              <Trash2 />
              Delete profile
            </button>
          </div>
        </ContextMenu>
      )}
      {sharing && (
        <Modal
          title="Copy settings to Prism instances"
          close={() => setSharing(null)}
        >
          <p>
            Copy this configuration to the selected instances. Each activation
            keeps a recovery point and checks for unsaved settings.
          </p>
          {peers.map((p) => (
            <label className="check-row" key={p.id}>
              <input
                type="checkbox"
                value={p.id}
                checked={selected.includes(p.id)}
                onChange={(e) =>
                  setSelected(
                    e.target.checked
                      ? [...selected, p.id]
                      : selected.filter((id) => id !== p.id),
                  )
                }
              />
              {p.name}
            </label>
          ))}
          <label className="check-row">
            <input
              type="checkbox"
              checked={activateCopies}
              onChange={(e) => setActivateCopies(e.target.checked)}
            />
            Activate after copying
          </label>
          <div className="modal-actions">
            <button onClick={() => setSharing(null)}>Cancel</button>
            <button
              className="primary"
              disabled={!selected.length || busy}
              onClick={() =>
                void run(async () => {
                  const copies = await call<{ game: Game; profile: Profile }[]>(
                    "share",
                    {
                      game: g.id,
                      profile: sharing.id,
                      targets: selected,
                    },
                  );
                  setSharing(null);
                  if (activateCopies) {
                    await bulk(copies);
                    return;
                  }
                  notice(
                    "Settings copied. Open an instance to activate its new configuration.",
                  );
                })
              }
            >
              Copy configuration
            </button>
          </div>
        </Modal>
      )}
      {importing && (
        <Modal
          title="Import configuration"
          close={() => setImporting(false)}
          busy={busy}
        >
          <button
            onClick={() =>
              void run(async () => {
                const [p] = await pick("archive", g.id);
                if (p) {
                  setImporting(false);
                  setImportPath(p);
                }
              })
            }
          >
            <Download />
            Choose profile archive
          </button>
          <h4>Import individual files</h4>
          <label>
            Profile name
            <input
              value={importName}
              maxLength={120}
              onChange={(e) => setImportName(e.target.value)}
            />
          </label>
          {g.maps.map((m, i) => (
            <label key={i}>
              Destination · {shortPath(m.root)}
              {String.fromCharCode(92)}
              {m.name}
              <button
                onClick={() =>
                  void run(async () => {
                    const [p] = await pick("source", g.id);
                    if (p)
                      setSources((prev) =>
                        prev.map((f, j) => (j === i ? p : f)),
                      );
                  })
                }
              >
                {sources[i] ? shortPath(sources[i]) : "Choose source file…"}
              </button>
            </label>
          ))}
          <div className="modal-actions">
            <button onClick={() => setImporting(false)}>Cancel</button>
            <button
              className="primary"
              disabled={busy || !importName.trim() || sources.some((f) => !f)}
              onClick={() =>
                void run(async () => {
                  await call("importFiles", {
                    game: g.id,
                    name: importName,
                    files: sources,
                  });
                  setImporting(false);
                  await reload();
                  notice("Configuration imported.");
                })
              }
            >
              Import files
            </button>
          </div>
        </Modal>
      )}
      {importPath && (
        <Modal
          title="Confirm destination mapping"
          close={() => setImportPath("")}
          busy={busy}
        >
          <p>
            The imported profile will use these destinations when activated.
            Filenames must match the archive in this order.
          </p>
          <ol className="paths">
            {g.maps.map((m, i) => (
              <li key={i}>
                {shortPath(m.root)}\{m.name}
              </li>
            ))}
          </ol>
          <div className="modal-actions">
            <button onClick={() => setImportPath("")}>Cancel</button>
            <button
              className="primary"
              onClick={() =>
                void run(async () => {
                  await call("import", { game: g.id, path: importPath });
                  setImportPath("");
                  await reload();
                  notice("Profile imported.");
                })
              }
            >
              Import profile
            </button>
          </div>
        </Modal>
      )}
      {compare && (
        <Modal title="Compare configurations" close={() => setCompare(false)}>
          <div className="two-col">
            {[first, second].map((v, i) => (
              <label key={i}>
                {i ? "To profile" : "From profile"}
                <select
                  value={v}
                  onChange={(e) => {
                    (i ? setSecond : setFirst)(e.target.value);
                    setDiff([]);
                  }}
                >
                  {profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
          {!!diff.length && (
            <ul className="path-list">
              {diff.map((d, i) => (
                <li key={i}>
                  <div className="diff">
                    <b>
                      {d.file} · {d.state}
                    </b>
                    {d.lines.map((line, k) => (
                      <pre key={k} className={line.added ? "added" : "removed"}>
                        {line.added ? "+" : "−"} {line.number} {line.text}
                      </pre>
                    ))}
                    {d.more && (
                      <small>Showing the first 200 changed lines.</small>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          )}
          <div className="modal-actions">
            <button
              disabled={first === second}
              onClick={() =>
                void run(async () =>
                  setDiff(await call("compare", { game: g.id, first, second })),
                )
              }
            >
              Compare files
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
