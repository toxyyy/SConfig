export type Theme = {
  name: string;
  mode: "dark" | "light";
  colors: Record<string, string>;
};
const dark = {
  bg: "#10141b",
  side: "#151b24",
  panel: "#1d2531",
  hover: "#293548",
  border: "#344154",
  ink: "#edf3fc",
  muted: "#a2b1c6",
  blue: "#9fc2ff",
  accent: "#396bc9",
  green: "#94d9bd",
};
export const presets: Record<string, Theme> = {
  oled: {
    name: "OLED black",
    mode: "dark",
    colors: {
      ...dark,
      bg: "#000000",
      side: "#090c10",
      panel: "#141920",
      hover: "#222a35",
      border: "#303a48",
    },
  },
  dark: { name: "Midnight", mode: "dark", colors: dark },
  light: {
    name: "Daylight",
    mode: "light",
    colors: {
      bg: "#f3f5fa",
      side: "#e9eef7",
      panel: "#ffffff",
      hover: "#dde6f5",
      border: "#ccd5e3",
      ink: "#1b2940",
      muted: "#52647e",
      blue: "#315fac",
      accent: "#396bc9",
      green: "#217454",
    },
  },
  forest: {
    name: "Forest",
    mode: "dark",
    colors: {
      ...dark,
      bg: "#101b18",
      side: "#15231e",
      panel: "#1c3028",
      hover: "#294437",
      border: "#355447",
      blue: "#a6d8b5",
      accent: "#357657",
    },
  },
  plum: {
    name: "Plum",
    mode: "dark",
    colors: {
      ...dark,
      bg: "#1b1522",
      side: "#221a2d",
      panel: "#2d233b",
      hover: "#3e3151",
      border: "#51405e",
      blue: "#d6b3f3",
      accent: "#8059b0",
    },
  },
};
export function parseTheme(raw: string): Theme {
  const v = JSON.parse(raw);
  if (
    !v ||
    typeof v.name !== "string" ||
    v.name.length > 60 ||
    !["dark", "light"].includes(v.mode) ||
    !v.colors
  )
    throw Error("Theme needs a name, mode (dark or light) and colors.");
  for (const key of Object.keys(dark))
    if (
      typeof v.colors[key] !== "string" ||
      !/^#[0-9a-f]{6}$/i.test(v.colors[key])
    )
      throw Error("Theme color " + key + " must be a six-digit hex color.");
  return {
    name: v.name,
    mode: v.mode,
    colors: Object.fromEntries(Object.keys(dark).map((k) => [k, v.colors[k]])),
  };
}
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  root.dataset.theme = theme.mode;
  for (const [k, v] of Object.entries(theme.colors))
    root.style.setProperty("--" + k, v);
}
