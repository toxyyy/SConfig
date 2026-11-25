import { useMemo, useState } from "react";
import {
  ArrowDownAZ,
  Ellipsis,
  Eye,
  EyeOff,
  Gamepad2,
  Grid2X2,
  List,
  Plus,
  RefreshCw,
  SearchX,
  SlidersHorizontal,
  Star,
  SwitchCamera,
} from "lucide-react";
import { Cover, ContextMenu, Modal } from "./components";
import { call, type Game, type Profile, type Run } from "./api";
export type Scope = "Library" | "Hidden";
export function Library({
  games,
  scope,
  query,
  open,
  add,
  scan,
  run,
  refresh,
  quick,
  busy,
  size,
}: {
  games: Game[];
  scope: Scope;
  query: string;
  open: (g: Game) => void;
  add: () => void;
  scan: () => void;
  run: Run;
  refresh: () => Promise<void>;
  quick: (g: Game, p: Profile) => void;
  busy: boolean;
  size: string;
}) {
  const [view, setView] = useState(localStorage.getItem("view") || "grid"),
    [sort, setSort] = useState(localStorage.getItem("sort") || "name");
  const [menu, setMenu] = useState<Game | null>(null),
    [profiles, setProfiles] = useState<Profile[]>([]),
    [filter, setFilter] = useState("All games");
  const items = useMemo(
    () =>
      games
        .filter(
          (g) =>
            (scope === "Hidden" ? g.hidden : !g.hidden) &&
            g.name.toLowerCase().includes(query.toLowerCase()) &&
            (filter === "All games" ||
              (filter === "Configured") === !!g.maps.length),
        )
        .sort(
          (a, b) =>
            Number(b.favorite) - Number(a.favorite) ||
            (sort === "recent"
              ? b.modified - a.modified || a.name.localeCompare(b.name)
              : a.name.localeCompare(b.name)),
        ),
    [games, scope, query, sort, filter],
  );
  const [removing, setRemoving] = useState<Game | null>(null);
  const [point, setPoint] = useState({ x: 0, y: 0 });
  function context(g: Game, x: number, y: number) {
    setPoint({ x, y });
    void run(async () => {
      setProfiles(await call<Profile[]>("detail", { game: g.id }));
      setMenu(g);
    });
  }
  function mode(v: string) {
    setView(v);
    localStorage.setItem("view", v);
  }
  return (
    <>
      <div className="page-head">
        <div>
          <h1>
            {scope === "Library"
              ? "Game library"
              : scope === "Hidden"
                ? "Hidden games"
                : "Favorites"}
          </h1>
        </div>
        <div className="row">
          <button onClick={scan} disabled={busy}>
            <RefreshCw />
            Scan library
          </button>
          <button className="primary" onClick={add} disabled={busy}>
            <Plus />
            Add game
          </button>
        </div>
      </div>
      <div className="tools">
        <div className="row">
          <select
            aria-label="Setup filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          >
            {["All games", "Configured", "Needs setup"].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
          <span className="count">{items.length}</span>
        </div>
        <div className="row">
          <ArrowDownAZ size={17} />
          <select
            aria-label="Sort games"
            value={sort}
            onChange={(e) => {
              setSort(e.target.value);
              localStorage.setItem("sort", e.target.value);
            }}
          >
            <option value="name">Alphabetical</option>
            <option value="recent">Recently managed</option>
          </select>
          <div className="seg">
            <button
              className="icon"
              aria-label="Grid view"
              aria-pressed={view === "grid"}
              onClick={() => mode("grid")}
            >
              <Grid2X2 />
            </button>
            <button
              className="icon"
              aria-label="List view"
              aria-pressed={view === "list"}
              onClick={() => mode("list")}
            >
              <List />
            </button>
          </div>
        </div>
      </div>
      {items.length ? (
        <section
          aria-label="Games"
          className={view === "list" ? "games-list" : "games-grid " + size}
        >
          {items.map((g) => (
            <article
              key={g.id}
              className="game-card"
              data-game-id={g.id}
              onContextMenu={(e) => {
                e.preventDefault();
                context(g, e.clientX, e.clientY);
              }}
            >
              <button
                className="game-open"
                disabled={busy}
                onClick={() => open(g)}
              >
                <Cover game={g} />
                <div className="card-info">
                  <small>{g.platform}</small>
                  <h3>{g.name}</h3>
                  <span
                    className={"status " + (g.maps.length ? "managed" : "")}
                  >
                    <i />
                    {g.active
                      ? g.activeName || "Active configuration"
                      : g.maps.length
                        ? "Configured"
                        : "Needs setup"}
                  </span>
                </div>
              </button>
              <button
                className="card-menu icon"
                disabled={busy}
                aria-label={"Options for " + g.name}
                onClick={(e) => context(g, e.clientX, e.clientY)}
              >
                <Ellipsis />
              </button>
              <button
                disabled={busy}
                className={
                  "favorite-button icon " + (g.favorite ? "selected" : "")
                }
                aria-label={(g.favorite ? "Unfavorite " : "Favorite ") + g.name}
                aria-pressed={g.favorite}
                onClick={() =>
                  void run(async () => {
                    await call("flag", { game: g.id, flag: "favorite" });
                    await refresh();
                  })
                }
              >
                <Star size={17} />
              </button>
            </article>
          ))}
        </section>
      ) : (
        <div className="empty">
          <div className="empty-art">{query ? <SearchX /> : <Gamepad2 />}</div>
          <h2>
            {query
              ? "No matching games"
              : scope === "Library" && !games.length
                ? "Add your first game"
                : "No games here yet"}
          </h2>
          <p>
            {query
              ? "Try another game title."
              : scope === "Hidden"
                ? "Games hidden from the library appear here."
                : filter !== "All games"
                  ? "Try a different setup filter."
                  : "Scan your installed games, or choose a game manually."}
          </p>
          {scope === "Library" && !query && !games.length && (
            <button className="primary" onClick={add}>
              <Plus />
              Add game
            </button>
          )}
        </div>
      )}
      {menu && (
        <ContextMenu point={point} close={() => setMenu(null)}>
          <div className="menu-list">
            <button
              onClick={() => {
                open(menu);
                setMenu(null);
              }}
            >
              <SlidersHorizontal />
              Manage configurations
            </button>
            {["favorite", "hidden"].map((flag) => (
              <button
                key={flag}
                onClick={() =>
                  void run(async () => {
                    await call("flag", { game: menu.id, flag });
                    setMenu(null);
                    await refresh();
                  })
                }
              >
                {flag === "favorite" ? (
                  <Star />
                ) : menu.hidden ? (
                  <Eye />
                ) : (
                  <EyeOff />
                )}
                {flag === "favorite"
                  ? menu.favorite
                    ? "Remove favorite"
                    : "Add to favorites"
                  : menu.hidden
                    ? "Show in library"
                    : "Hide game"}
              </button>
            ))}
          </div>
          <button
            className="danger"
            onClick={() => {
              setRemoving(menu);
              setMenu(null);
            }}
          >
            Remove game…
          </button>
          {!!profiles.length && (
            <>
              <h4>Quick switch</h4>
              <div className="menu-list">
                {profiles.map((p) => (
                  <button
                    key={p.id}
                    disabled={menu.active === p.id}
                    onClick={() => {
                      quick(menu, p);
                      setMenu(null);
                    }}
                  >
                    <SwitchCamera />
                    {p.name}
                    {menu.active === p.id ? " · Active" : ""}
                  </button>
                ))}
              </div>
            </>
          )}
        </ContextMenu>
      )}
      {removing && (
        <Modal title="Remove game?" close={() => setRemoving(null)}>
          <p>
            Remove {removing.name} from the library? Stored profiles are kept.
            You can restore the game from Settings.
          </p>
          <div className="modal-actions">
            <button onClick={() => setRemoving(null)}>Cancel</button>
            <button
              className="danger"
              onClick={() =>
                void run(async () => {
                  await call("flag", { game: removing.id, flag: "removed" });
                  setRemoving(null);
                  await refresh();
                })
              }
            >
              Remove game
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
