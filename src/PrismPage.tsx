import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ArrowLeft, Copy, RefreshCw, ArrowRightLeft } from "lucide-react";
import { call, type Game, type Run } from "./api";
import { Cover, Modal } from "./components";
import { Overview, type Setting } from "./Overview";
type Instance = {
  game: Game;
  overview: Setting[];
  version: string | null;
  available: boolean;
};
export function PrismPage({
  run,
  busy,
  back,
  notice,
  error,
}: {
  run: Run;
  busy: boolean;
  back: () => void;
  notice: (s: string) => void;
  error: (s: string) => void;
}) {
  const [loading, setLoading] = useState(true);
  const [instances, setInstances] = useState<Instance[]>([]),
    [destination, setDestination] = useState<Instance | null>(null);
  const load = async () => {
    const data = await call<Instance[]>("prismInstances");
    setInstances(data.sort((a, b) => a.game.name.localeCompare(b.game.name)));
  };
  useEffect(() => {
    void load()
      .catch((e) => error(String(e)))
      .finally(() => setLoading(false));
    const sub = listen<string>("profile-auto-saved", (e) => {
      if (e.payload.startsWith("prism-"))
        void load().catch((e) => error(String(e)));
    });
    return () => {
      void sub.then((off) => off());
    };
  }, []);
  const transfer = (source: Instance) =>
    void run(async () => {
      if (!destination) return;
      const result = await call<{ changed: boolean }>("prismCopy", {
        source: source.game.id,
        destination: destination.game.id,
      });
      setDestination(null);
      await load();
      notice(
        result.changed
          ? "Applied settings from " + source.game.name + "."
          : "These instances already have matching settings.",
      );
    });
  return (
    <>
      <button className="back" onClick={back}>
        <ArrowLeft />
        Game library
      </button>
      <div className="page-head prism-head">
        <h1>Prism Launcher</h1>
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await call("scan");
              await load();
            })
          }
        >
          <RefreshCw />
          Refresh instances
        </button>
      </div>
      <div className="instance-grid">
        {instances.map((instance) => (
          <article className="instance-card" key={instance.game.id}>
            <div className="instance-art">
              <Cover game={instance.game} />
            </div>
            <h3>{instance.game.name}</h3>
            {instance.version && <small>Minecraft {instance.version}</small>}
            <Overview values={instance.overview} />
            <button
              className="primary"
              disabled={busy || !instance.available}
              title={
                instance.available
                  ? "Apply another instance’s settings"
                  : "Launch this instance once in Prism to generate its settings"
              }
              onClick={() =>
                void run(async () => {
                  await load();
                  setDestination(instance);
                })
              }
            >
              <Copy />
              Copy from…
            </button>
          </article>
        ))}
      </div>
      {loading && <p role="status">Loading instances…</p>}
      {!loading && !instances.length && !busy && (
        <div className="empty">
          <h2>No instances found</h2>
          <p>
            Choose your Prism data folder in Settings to load its instances.
          </p>
        </div>
      )}
      {destination && (
        <Modal
          title={"Copy settings to " + destination.game.name}
          busy={busy}
          close={() => setDestination(null)}
        >
          <p>
            Select the source instance. Matching managed settings files will
            replace this instance’s settings, with a recovery point kept first.
          </p>
          <div className="instance-grid source-grid">
            {instances
              .filter((i) => i.game.id !== destination.game.id)
              .map((instance) => (
                <article className="instance-card" key={instance.game.id}>
                  <button
                    className="source-instance"
                    disabled={busy || !instance.available}
                    onClick={() => transfer(instance)}
                  >
                    <div className="instance-art">
                      <Cover game={instance.game} />
                    </div>
                    <h3>{instance.game.name}</h3>
                  </button>
                  {instance.version && (
                    <small>Minecraft {instance.version}</small>
                  )}
                  <Overview values={instance.overview} />
                  <button
                    disabled={busy || !instance.available}
                    onClick={() => transfer(instance)}
                  >
                    <ArrowRightLeft />
                    Use settings
                  </button>
                </article>
              ))}
          </div>
        </Modal>
      )}
    </>
  );
}
