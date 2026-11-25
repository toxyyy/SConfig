import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import {
  Check,
  FilePlus2,
  FolderOpen,
  Gamepad2,
  ImagePlus,
  Plus,
  X,
} from "lucide-react";
import {
  basename,
  call,
  pick,
  shortPath,
  type Game,
  type Profile,
  type Run,
} from "./api";
export function Modal({
  title,
  close,
  children,
  busy = false,
}: {
  title: string;
  close: () => void;
  children: ReactNode;
  busy?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const el = ref.current;
    el?.showModal();
    return () => el?.close();
  }, []);
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) close();
      }}
    >
      <div className="modal-head">
        <h2 id={titleId}>{title}</h2>
        <button
          className="icon"
          aria-label="Close dialog"
          disabled={busy}
          onClick={close}
        >
          <X />
        </button>
      </div>
      {children}
    </dialog>
  );
}
const coverCache = new Map<string, string>();
export function Cover({ game }: { game: Game }) {
  const ref = useRef<HTMLDivElement>(null);
  const [iconOnly, setIconOnly] = useState(false);
  const [url, setUrl] = useState(coverCache.get(game.id));
  useEffect(() => {
    setUrl(coverCache.get(game.id));
    if (coverCache.has(game.id) || !ref.current) return;
    let live = true;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        observer.disconnect();
        call<string | null>("cover", { game: game.id })
          .then((value) => {
            if (value) {
              coverCache.set(game.id, value);
              if (live) setUrl(value);
            }
          })
          .catch(() => {});
      },
      { rootMargin: "120px" },
    );
    observer.observe(ref.current);
    return () => {
      live = false;
      observer.disconnect();
    };
  }, [game.id, game.cover]);
  const hue = [...game.name].reduce((v, c) => v + c.charCodeAt(0), 0) % 360;
  return (
    <div
      className="cover"
      ref={ref}
      style={{ backgroundColor: "hsl(" + hue + " 25% 23%)" }}
    >
      {url ? (
        <>
          <img
            className={iconOnly ? "exe-icon" : ""}
            src={url}
            alt=""
            loading="lazy"
            onLoad={(e) => setIconOnly(e.currentTarget.naturalWidth < 128)}
          />
          {iconOnly && <b>{game.name}</b>}
        </>
      ) : (
        <>
          <div className="cover-lines" />
          <Gamepad2 className="cover-icon" />
          <b>{game.name}</b>
        </>
      )}
    </div>
  );
}
export function Setup({
  game,
  busy,
  run,
  done,
  close,
}: {
  game?: Game;
  busy: boolean;
  run: Run;
  done: (id: string) => Promise<void>;
  close: () => void;
}) {
  const [name, setName] = useState(game?.name || ""),
    [platform, setPlatform] = useState(game?.platform || "Other");
  const [candidates, setCandidates] = useState<
    { files: string[]; overview: { label: string; value: string }[] }[]
  >([]);
  const [exe, setExe] = useState(""),
    [files, setFiles] = useState<string[]>([]),
    [art, setArt] = useState("");
  useEffect(() => {
    if (!game) return;
    let open = true;
    call<{
      exe: string | null;
      files: string[];
      candidates: {
        files: string[];
        overview: { label: string; value: string }[];
      }[];
    }>("suggestions", {
      game: game.id,
    })
      .then((s) => {
        if (open) {
          setExe((old) => old || s.exe || "");
          setCandidates(s.candidates || []);
          setFiles((old) => (old.length ? old : s.files));
        }
      })
      .catch(() => {});
    return () => {
      open = false;
    };
  }, [game?.id]);
  async function browse(field: string, folder = false) {
    const paths = await pick(field, game?.id, folder);
    if (!paths.length) return;
    if (field === "exe") {
      setExe(paths[0]);
      if (!name) setName(basename(paths[0]).replace(/\.exe$/i, ""));
    }
    if (field === "files") setFiles((old) => [...new Set([...old, ...paths])]);
    if (field === "cover") setArt(paths[0]);
  }
  return (
    <Modal
      title={game ? "Set up configuration management" : "Add game"}
      close={close}
      busy={busy}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            const id = await call<string>("register", {
              game: game?.id,
              name,
              platform,
              exe,
              files,
              cover: art || null,
            });
            await done(id);
          });
        }}
      >
        <fieldset disabled={busy}>
          {candidates.length > 1 && (
            <section className="candidates">
              <h3>Choose your settings</h3>
              {candidates.map((c, i) => (
                <button
                  type="button"
                  key={i}
                  aria-pressed={files.join() === c.files.join()}
                  onClick={() => setFiles(c.files)}
                >
                  <b>{shortPath(c.files[0])}</b>
                  <span>
                    {c.overview
                      .map((v) => v.label + " " + v.value)
                      .join(" · ") || "No recognized settings"}
                  </span>
                </button>
              ))}
            </section>
          )}
          <div className="two-col">
            <label>
              Game name
              <input
                autoFocus
                required
                maxLength={120}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Game title"
              />
            </label>
            <label>
              Platform
              <select
                value={platform}
                onChange={(e) => setPlatform(e.target.value)}
              >
                {[
                  "Steam",
                  "Epic Games",
                  "GOG Galaxy",
                  "Ubisoft Connect",
                  "EA app",
                  "Xbox",
                  "Other",
                ].map((s) => (
                  <option key={s}>{s}</option>
                ))}
              </select>
            </label>
          </div>
          <label>
            Game executable
            <button
              className="picker"
              type="button"
              onClick={() => void run(() => browse("exe"))}
            >
              <Gamepad2 />
              {exe ? basename(exe) : "Choose executable…"}
            </button>
          </label>
          <p className="hint">
            Choose the process that runs while you play. Switchboard checks it
            before replacing settings.
          </p>
          <label>Managed configuration files</label>
          <div className="row">
            <button
              type="button"
              onClick={() => void run(() => browse("files"))}
            >
              <FilePlus2 />
              Choose files
            </button>
            <button
              type="button"
              onClick={() => void run(() => browse("files", true))}
            >
              <FolderOpen />
              Browse folder
            </button>
          </div>
          <p className="hint">
            Select individual settings files. You can add files from several
            folders.
          </p>
          {!!files.length && (
            <ul className="path-list">
              {files.map((f) => (
                <li key={f}>
                  {shortPath(f)}
                  <button
                    type="button"
                    className="icon"
                    aria-label={"Remove " + basename(f)}
                    onClick={() => setFiles(files.filter((x) => x !== f))}
                  >
                    <X />
                  </button>
                </li>
              ))}
            </ul>
          )}
          <button type="button" onClick={() => void run(() => browse("cover"))}>
            <ImagePlus />
            {art ? basename(art) : "Choose cover artwork (optional)"}
          </button>
          <div className="modal-actions">
            <button type="button" onClick={close}>
              Cancel
            </button>
            <button
              className="primary"
              disabled={!name.trim() || !exe || !files.length}
            >
              <Check />
              {game ? "Save setup" : "Add game"}
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}
export function ProfileForm({
  profile,
  duplicate,
  busy,
  close,
  submit,
}: {
  profile?: Profile;
  duplicate?: boolean;
  busy: boolean;
  close: () => void;
  submit: (name: string, description: string, tags: string[]) => void;
}) {
  const [name, setName] = useState(
    profile ? (duplicate ? profile.name + " copy" : profile.name) : "",
  );
  const [description, setDescription] = useState(profile?.description || ""),
    [tags, setTags] = useState(profile?.tags.join(", ") || "");
  return (
    <Modal
      title={
        duplicate
          ? "Duplicate configuration"
          : profile
            ? "Edit configuration"
            : "Create configuration"
      }
      close={close}
      busy={busy}
    >
      {!profile && (
        <p className="hint">
          Capture the current contents of this game’s managed files.
        </p>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit(
            name,
            description,
            tags
              .split(",")
              .map((x) => x.trim())
              .filter(Boolean),
          );
        }}
      >
        <fieldset disabled={busy}>
          <label>
            Profile name
            <input
              autoFocus
              required
              maxLength={120}
              value={name}
              placeholder="Competitive"
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label>
            Description
            <textarea
              value={description}
              maxLength={1024}
              rows={3}
              placeholder="Optional personal notes"
              onChange={(e) => setDescription(e.target.value)}
            />
          </label>
          <label>
            Tags
            <input
              value={tags}
              placeholder="Ranked, 1440p"
              onChange={(e) => setTags(e.target.value)}
            />
          </label>
          <div className="modal-actions">
            <button type="button" onClick={close}>
              Cancel
            </button>
            <button className="primary" disabled={!name.trim()}>
              <Plus />
              {profile && !duplicate ? "Save profile" : "Create configuration"}
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}
export function ContextMenu({
  point,
  close,
  children,
}: {
  point: { x: number; y: number };
  close: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current!;
    el.style.left =
      Math.max(8, Math.min(point.x, innerWidth - el.offsetWidth - 8)) + "px";
    el.style.top =
      Math.max(8, Math.min(point.y, innerHeight - el.offsetHeight - 8)) + "px";
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const outside = (e: PointerEvent) => {
      if (!el.contains(e.target as Node)) close();
    };
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        close();
        return;
      }
      if (!["ArrowDown", "ArrowUp"].includes(e.key)) return;
      e.preventDefault();
      const buttons = [
        ...el.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
      ];
      const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[
        (i + (e.key === "ArrowDown" ? 1 : buttons.length - 1)) % buttons.length
      ]?.focus();
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", keys);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", keys);
      window.removeEventListener("resize", close);
    };
  }, [point.x, point.y]);
  return (
    <div
      ref={ref}
      role="menu"
      className="context-menu"
      style={{ left: point.x, top: point.y }}
    >
      {children}
    </div>
  );
}
