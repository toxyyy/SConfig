import { invoke } from "@tauri-apps/api/core";
export type Game = {
  activeName?: string;
  id: string;
  name: string;
  platform: string;
  install: string;
  exe: string | null;
  maps: { root: string; name: string }[];
  active: string | null;
  favorite: boolean;
  hidden: boolean;
  modified: number;
  cover: boolean;
};
export type Profile = {
  gameName?: string;
  overview?: { label: string; value: string; group?: string }[];
  thumbnail?: string | null;
  id: string;
  game: string;
  name: string;
  description: string;
  tags: string[];
  created: number;
  modified: number;
  count: number;
  recovery: boolean;
};
export type Decision = "check" | "save" | "discard";
export type Outcome =
  { status: "applied" } | { status: "unsaved"; token: string; files: string[] };
export const call = <T = void>(
  kind: string,
  fields: Record<string, unknown> = {},
) => invoke<T>("request", { action: { kind, ...fields } });
export const date = (stamp: number) =>
  new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(stamp);
export const shortPath = (path: string) => path.replace(/^\\\\\?\\/, "");
export const basename = (path: string) => path.split(/[\\/]/).at(-1) || path;
export const pick = (field: string, game?: string, folder = false) =>
  call<string[]>("pick", { field, game, folder });
export type Run = (fn: () => Promise<void>) => Promise<void>;
