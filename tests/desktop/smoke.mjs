import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import { setTimeout as wait } from "node:timers/promises";
import assert from "node:assert/strict";
fs.mkdirSync(".test-output", { recursive: true });
const root = fs.mkdtempSync(path.resolve(".test-output/desktop-"));
const exe = path.join(root, "SConfig.exe");
fs.copyFileSync(
  process.env.SCONFIG_TEST_EXECUTABLE || "src-tauri/target/release/SConfig.exe",
  exe,
);
fs.mkdirSync(path.join(root, "fixture"));
fs.writeFileSync(path.join(root, "fixture", "FixtureGame.exe"), "");
fs.writeFileSync(
  path.join(root, "fixture", "settings.ini"),
  "sensitivity=1\r\n",
);
const ini = path.join(root, "fixture", "settings.ini");
execFileSync("python", [
  "tests/desktop/prism-fixture.py",
  root,
  "--registered",
]);
if (process.argv.includes("--legacy"))
  fs.renameSync(
    path.join(root, "SConfigData"),
    path.join(root, "SwitchboardData"),
  );
const port = await new Promise((r) => {
  const s = http.createServer();
  s.listen(0, "127.0.0.1", () => {
    const p = s.address().port;
    s.close(() => r(p));
  });
});
const app = spawn(exe, ["--portable"], {
  windowsHide: true,
  env: {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: path.join(root, "webview"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
      "--remote-debugging-port=" +
      port +
      " --remote-debugging-address=127.0.0.1",
  },
  stdio: ["ignore", "ignore", "pipe"],
});
let appErrors = "";
app.stderr.on("data", (chunk) => {
  appErrors = (appErrors + chunk.toString()).slice(-4000);
});
let ws,
  n = 0,
  pending = new Map();
async function until(fn, desc) {
  let last;
  for (let i = 0; i < 200; i++) {
    if (app.exitCode !== null)
      throw new Error("SConfig exited with " + app.exitCode + ": " + appErrors);
    try {
      if (await fn()) return;
    } catch (e) {
      last = e;
    }
    await wait(160);
  }
  throw Error("Timed out: " + desc + " " + (last || ""));
}
function send(method, params = {}) {
  const id = ++n;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  const r = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (r.exceptionDetails)
    throw Error(
      r.exceptionDetails.exception?.description || r.exceptionDetails.text,
    );
  return r.result.value;
}
async function click(selector, exact, scope = "document") {
  const q = JSON.stringify(selector),
    v = JSON.stringify(exact);
  await until(
    () =>
      evaluate(
        "(()=>{const e=[..." +
          scope +
          ".querySelectorAll(" +
          q +
          ")].find(e=>!e.matches(':disabled') && e.textContent.trim()===" +
          v +
          ");if(!e)return false;e.click();return true})()",
      ),
    exact,
  );
}
async function form(name) {
  await click("button", "Create configuration");
  await until(
    () =>
      evaluate(
        "Boolean(document.querySelector('dialog input[required]:enabled')) && !document.querySelector('[aria-busy=true]')",
      ),
    "profile form",
  );
  await evaluate(
    "(()=>{const i=document.querySelector('dialog input[required]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i," +
      JSON.stringify(name) +
      ");i.dispatchEvent(new Event('input',{bubbles:true}))})()",
  );
  await click(
    "button",
    "Create configuration",
    "document.querySelector('dialog')",
  );
  await until(
    () =>
      evaluate(
        "!document.querySelector('dialog') && !document.querySelector('[aria-busy=true]')",
      ),
    "profile saved",
  );
}
try {
  let target;
  await until(async () => {
    const pages = await (
      await fetch("http://127.0.0.1:" + port + "/json/list")
    ).json();
    target = pages.find(
      (x) => x.type === "page" && x.url.includes("tauri.localhost"),
    );
    return target;
  }, "desktop webview");
  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r, j) => {
    ws.onopen = r;
    ws.onerror = j;
  });
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    const p = pending.get(m.id);
    if (p) {
      pending.delete(m.id);
      m.error ? p.reject(Error(m.error.message)) : p.resolve(m.result);
    }
  };
  await until(
    () => evaluate("document.title.startsWith('SConfig')"),
    "document loaded",
  );
  assert.ok(fs.existsSync(path.join(root, "SConfigData", "library.sqlite3")));
  await until(
    () =>
      evaluate(
        "document.querySelector('.tools') && !document.querySelector('[aria-busy=true]')",
      ),
    "library scan",
  );
  console.log(
    "Library loaded:",
    await evaluate("document.querySelectorAll('.game-card').length"),
    "games.",
  );
  await wait(700);
  const screenshot = async (name) => {
    const { data } = await send("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(
      path.join(root, name + ".png"),
      Buffer.from(data, "base64"),
    );
  };
  await screenshot("library");

  const request = (kind, fields = {}) =>
    evaluate(
      "window.__TAURI_INTERNALS__.invoke('request',{action:" +
        JSON.stringify({ kind, ...fields }) +
        "})",
    );
  const openFixture = async () => {
    await evaluate(
      "document.querySelector('[data-game-id=fixture-game] .game-open').click()",
    );
    await until(
      () =>
        evaluate(
          "Boolean(document.querySelector('.detail-head')) && !document.querySelector('[aria-busy=true]')",
        ),
      "fixture detail",
    );
  };
  assert.equal(
    await evaluate(
      "getComputedStyle(document.body).getPropertyValue('--bg').trim()",
    ),
    "#000000",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.window-chrome').innerText.includes('SConfig')",
    ),
    false,
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('[data-game-id^=prism-fixture]').length",
    ),
    0,
  );
  assert.equal(
    await evaluate(
      "Boolean(document.querySelector('[data-game-id=prism-launcher]'))",
    ),
    true,
  );
  await openFixture();
  if (!(await request("detail", { game: "fixture-game" })).length)
    await form("Original settings");
  const active = (await request("game", { game: "fixture-game" })).active;
  const settings =
    "sensitivity=2\nDefaultFOV=90.000000\nRefreshRate=165.001007\nVSync=0\nWindowMode=0\nADSMouseUseSpecific=1\nADSMouseSensitivity1x=35\nADSMouseSensitivity2x=50\nOverallQualityLevelName=Custom\nTexture=2\nShadow=1\n";
  fs.writeFileSync(ini, settings);
  await until(async () => {
    const p = (await request("detail", { game: "fixture-game" })).find(
      (p) => p.id === active,
    );
    return p?.overview.some(
      (s) => s.label === "Sensitivity" && s.value === "2",
    );
  }, "automatic save into active snapshot");
  await until(
    () =>
      evaluate(
        "document.querySelector('.profile-card.active')?.innerText.includes('165')",
      ),
    "automatic overview refresh",
  );
  const overview = (await request("detail", { game: "fixture-game" })).find(
    (p) => p.id === active,
  ).overview;
  assert.equal(overview.find((s) => s.label === "Field of view").value, "90");
  assert.equal(overview.find((s) => s.label === "Refresh rate").value, "165");
  assert.equal(
    overview.some((s) => /VSync|Display mode/.test(s.label)),
    false,
  );
  assert.equal(overview.find((s) => s.label === "1x").group, "ads");
  assert.equal(
    overview.find((s) => s.label === "Texture quality").group,
    "graphics",
  );
  assert.equal(
    await evaluate("document.body.innerText.includes('Unsaved Changes')"),
    false,
  );
  await evaluate(
    "document.querySelector('.profile-card.active details').open=true",
  );
  execFileSync("python", [
    "-c",
    "import sqlite3,sys;db=sqlite3.connect(sys.argv[1]);db.execute('INSERT OR REPLACE INTO media VALUES(?,?)',(sys.argv[2],open(sys.argv[3],'rb').read()));db.commit()",
    path.join(root, "SConfigData", "library.sqlite3"),
    active,
    path.resolve("src/logo.png"),
  ]);
  await click(".crumb button", "Library");
  await openFixture();
  await until(
    () => evaluate("Boolean(document.querySelector('.profile-thumbnail'))"),
    "thumbnail",
  );
  const thumb = await evaluate(
    "(()=>{const a=document.querySelector('.profile-thumbnail').getBoundingClientRect(),b=document.querySelector('.profile-card').getBoundingClientRect();return {w:a.width,h:a.height,offset:Math.abs(a.x+a.width/2-b.x-b.width/2)}})()",
  );
  assert.ok(
    thumb.w <= 76 && thumb.h <= 76 && thumb.offset < 3,
    JSON.stringify(thumb),
  );
  await screenshot("profiles");
  await click(".crumb button", "Library");
  await evaluate(
    "window.dispatchEvent(new MouseEvent('mouseup',{button:3,bubbles:true,cancelable:true}))",
  );
  await until(
    () => evaluate("Boolean(document.querySelector('.detail-head'))"),
    "mouse back",
  );
  await evaluate(
    "window.dispatchEvent(new MouseEvent('mouseup',{button:4,bubbles:true,cancelable:true}))",
  );
  await until(
    () => evaluate("Boolean(document.querySelector('.games-grid'))"),
    "mouse forward",
  );
  await evaluate(
    "document.querySelector('[data-game-id=fixture-game] .favorite-button').click()",
  );
  await until(
    () =>
      evaluate(
        "document.querySelector('[data-game-id=fixture-game] .favorite-button')?.getAttribute('aria-pressed')==='true'",
      ),
    "favorite",
  );
  await evaluate(
    "document.querySelector('[data-game-id=fixture-game]').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:440,clientY:330}))",
  );
  await until(
    () => evaluate("Boolean(document.querySelector('[role=menu]'))"),
    "context menu",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('[role=menu]').getBoundingClientRect().x",
    ),
    440,
  );
  await click("button", "Remove game…");
  await until(
    () =>
      evaluate(
        "Boolean(document.querySelector('.undo-toast'))&&!document.querySelector('[data-game-id=fixture-game]')",
      ),
    "instant remove",
  );
  assert.equal(
    await evaluate("Boolean(document.querySelector('dialog'))"),
    false,
  );
  assert.ok(
    await evaluate(
      "document.querySelector('.undo-countdown').getBoundingClientRect().width>0",
    ),
  );
  await screenshot("remove-undo");
  await click(".undo-toast button", "Undo");
  await until(
    () =>
      evaluate(
        "Boolean(document.querySelector('[data-game-id=fixture-game]'))",
      ),
    "undo",
  );
  await evaluate(
    "document.querySelector('button[aria-label=\"List view\"]').click()",
  );
  const star = await evaluate(
    "(()=>{const c=document.querySelector('[data-game-id=fixture-game]'),s=c.querySelector('.favorite-button').getBoundingClientRect(),a=c.querySelector('.cover').getBoundingClientRect();return {sx:s.x,ax:a.right,opacity:getComputedStyle(c.querySelector('.favorite-button')).opacity}})()",
  );
  assert.ok(star.sx >= star.ax);
  assert.equal(star.opacity, "1");
  await screenshot("list");
  await request("flag", { game: "fixture-game", flag: "hidden" });
  await click("aside button", "Hidden");
  // Refresh the library state through the normal scan action.
  await click("button", "Scan library");
  await until(
    () =>
      evaluate(
        "Boolean(document.querySelector('[data-game-id=fixture-game]'))",
      ),
    "hidden view",
  );
  await evaluate(
    "(()=>{const i=document.querySelector('input[aria-label=\"Search library\"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,'fixture');i.dispatchEvent(new Event('input',{bubbles:true}))})()",
  );
  await until(
    () => evaluate("document.querySelectorAll('.game-card').length>=2"),
    "search across hidden and visible",
  );
  await click(".crumb button", "Library");
  await evaluate(
    "document.querySelector('[data-game-id=prism-launcher] .game-open').click()",
  );
  await until(
    () =>
      evaluate(
        "document.querySelectorAll('.instance-card').length>=3 && !document.querySelector('[aria-busy=true]')",
      ),
    "Prism instances",
  );
  await screenshot("prism-instances");
  await evaluate(
    "[...document.querySelectorAll('.instance-card')].find(e=>e.querySelector('h3').textContent==='Prism target-a').querySelector('button').click()",
  );
  await until(
    () => evaluate("Boolean(document.querySelector('dialog .source-grid'))"),
    "Prism source picker",
  );
  await screenshot("prism-source-picker");
  await evaluate(
    "[...document.querySelectorAll('dialog .instance-card')].find(e=>e.querySelector('h3').textContent==='Prism source').querySelector('button').click()",
  );
  await until(
    () => evaluate("!document.querySelector('[aria-busy=true]')"),
    "Prism copy finished",
  );
  const targetFile = path.join(root, "target-a", "minecraft", "options.txt");
  if (fs.readFileSync(targetFile, "utf8").includes("0.7"))
    console.log("PASS: Prism source settings copied to selected destination.");
  else {
    assert.ok(
      await evaluate(
        "document.body.innerText.includes('Close Minecraft')||document.body.innerText.includes('java')",
      ),
      "expected closed-game protection",
    );
    assert.match(fs.readFileSync(targetFile, "utf8"), /0.1/);
    console.log(
      "PASS: running Java blocked Prism write and preserved the destination.",
    );
  }
  console.log(
    "PASS: OLED, title chrome, automatic save, rounded overview, ADS/graphics, centered thumbnail, history, remove/undo, list stars, hidden/global search, grouped Prism UI.",
  );
  console.log("Screenshots:", root);
} catch (e) {
  console.error(e);
  if (ws?.readyState === 1)
    console.error((await evaluate("document.body.innerText")).slice(-1200));
  process.exitCode = 1;
} finally {
  ws?.close();
  app.kill();
}
