import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import { setTimeout as wait } from "node:timers/promises";
import assert from "node:assert/strict";
const native = process.argv.includes("--native");
fs.mkdirSync(".test-output", { recursive: true });
const root = fs.mkdtempSync(path.resolve(".test-output/desktop-"));
const exe = path.join(root, "SConfig.exe");
fs.copyFileSync("src-tauri/target/release/SConfig.exe", exe);
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
  ...(native ? [] : ["--registered"]),
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
  assert.equal(await evaluate("document.title.startsWith('SConfig')"), true);
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
  if (native) {
    await click("button", "Add game");
    for (const [label, file] of [
      ["Choose executable…", "FixtureGame.exe"],
      ["Choose files", "settings.ini"],
    ]) {
      await click("button", label, "document.querySelector('dialog')");
      execFileSync(
        "powershell.exe",
        [
          "-NoProfile",
          "-File",
          "tests/desktop/pick-file.ps1",
          String(app.pid),
          path.join(root, "fixture", file),
        ],
        { timeout: 18000, windowsHide: true },
      );
      await until(
        () => evaluate("!document.querySelector('fieldset[disabled]')"),
        "picker finished",
      );
    }
    await click("button", "Add game", "document.querySelector('dialog')");
    await until(
      () =>
        evaluate(
          "!document.querySelector('dialog') && document.querySelector('.detail-head') && !document.querySelector('[aria-busy=true]')",
        ),
      "manual registration",
    );
  } else {
    await until(
      () =>
        evaluate(
          "Boolean(document.querySelector('.game-card[data-game-id=fixture-game]')) && !document.querySelector('[aria-busy=true]')",
        ),
      "fixture ready",
    );
    await evaluate(
      "document.querySelector('.game-card[data-game-id=fixture-game] .game-open').click()",
    );
  }
  await until(
    () =>
      evaluate(
        "document.querySelector('.profile-card h3')?.textContent==='Original settings'",
      ),
    "original snapshot",
  );
  assert.equal(
    await evaluate("document.querySelector('.overview dd')?.textContent"),
    "1",
  );
  await form("Practice");
  fs.writeFileSync(ini, "sensitivity=2\r\n");
  await until(
    () => evaluate("document.body.innerText.includes('Unsaved Changes')"),
    "filesystem watcher",
  );
  await form("Ranked");
  fs.writeFileSync(ini, "sensitivity=3\r\n");
  await evaluate(
    "[...document.querySelectorAll('.profile-card')].find(c=>c.querySelector('h3').textContent==='Ranked').querySelector('.switch-button').click()",
  );
  await until(
    () =>
      evaluate(
        "document.querySelector('dialog h2')?.innerText==='Unsaved Configuration Changes'",
      ),
    "unsaved changes dialog",
  );
  await screenshot("changes");
  await click("button", "Cancel", "document.querySelector('dialog')");
  assert.match(fs.readFileSync(ini, "utf8"), /=3/);
  await evaluate(
    "[...document.querySelectorAll('.profile-card')].find(c=>c.querySelector('h3').textContent==='Ranked').querySelector('.switch-button').click()",
  );
  await click("button", "Save & Switch", "document.querySelector('dialog')");
  await until(
    () => fs.readFileSync(ini, "utf8").includes("=2"),
    "activated bytes",
  );
  await until(
    () =>
      evaluate(
        "!document.querySelector('dialog') && !document.querySelector('[aria-busy=true]')",
      ),
    "commit",
  );
  if (native) {
    await evaluate(
      "[...document.querySelectorAll('.profile-card')].find(c=>c.querySelector('h3').textContent==='Ranked').querySelector('button[aria-label]').click()",
    );
    await click("button", "Choose PNG thumbnail…");
    const thumb = path.join(root, "thumbnail.png");
    fs.copyFileSync("src/logo.png", thumb);
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-File",
        "tests/desktop/pick-file.ps1",
        String(app.pid),
        thumb,
      ],
      { timeout: 18000, windowsHide: true },
    );
    await until(
      () => evaluate("Boolean(document.querySelector('.profile-thumbnail'))"),
      "profile thumbnail",
    );
  }
  await screenshot("profiles");
  await click("aside button", "Recovery");
  await click("button", "Restore");
  await click("button", "Restore files", "document.querySelector('dialog')");
  await until(
    () => fs.readFileSync(ini, "utf8").includes("=3"),
    "restored bytes",
  );

  await evaluate("document.querySelector('aside nav button').click()");
  await evaluate(
    "[...document.querySelectorAll('.game-card')].find(c=>c.textContent.includes('FixtureGame')).querySelector('.favorite-button').click()",
  );
  await until(
    () =>
      evaluate(
        "document.querySelector('.game-card')?.textContent.includes('FixtureGame') && document.querySelector('.favorite-button')?.getAttribute('aria-pressed')==='true'",
      ),
    "favorite pinned first",
  );
  await evaluate(
    "document.querySelector('.game-card').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:440,clientY:330}))",
  );
  await until(
    () => evaluate("Boolean(document.querySelector('[role=menu]'))"),
    "pointer menu",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('[role=menu]').getBoundingClientRect().x",
    ),
    440,
  );
  assert.equal(
    await evaluate("Boolean(document.querySelector('dialog'))"),
    false,
  );
  await screenshot("context-menu");
  await evaluate(
    "document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))",
  );
  await click("aside button", "Settings");
  await evaluate(
    "(()=>{const s=[...document.querySelectorAll('.settings select')][1];s.value='light';s.dispatchEvent(new Event('change',{bubbles:true}))})()",
  );
  await until(
    () =>
      evaluate(
        "getComputedStyle(document.body).backgroundColor==='rgb(243, 245, 250)'",
      ),
    "light background",
  );
  await wait(250);
  await screenshot("light-settings");
  assert.equal(
    await evaluate(
      "document.querySelector('footer')===null && document.querySelector('.local')===null",
    ),
    true,
  );
  await evaluate(
    "(()=>{const s=[...document.querySelectorAll('.settings select')][1];s.value='forest';s.dispatchEvent(new Event('change',{bubbles:true}))})()",
  );
  await until(
    () =>
      evaluate(
        "getComputedStyle(document.body).backgroundColor==='rgb(16, 27, 24)'",
      ),
    "forest preset",
  );
  await wait(250);
  await screenshot("forest-settings");
  await evaluate("document.querySelector('aside nav button').click()");
  await until(
    () =>
      evaluate(
        "Boolean(document.querySelector('.game-open:not(:disabled)')) && !document.querySelector('[aria-busy=true]')",
      ),
    "library",
  );
  await evaluate(
    "document.querySelector('.game-card[data-game-id=prism-fixture-source] .game-open').click()",
  );
  await until(
    () =>
      evaluate(
        "document.querySelector('.profile-card h3')?.textContent==='Original settings'",
      ),
    "Prism original",
  );
  fs.writeFileSync(
    path.join(root, "source", "options.txt"),
    "mouseSensitivity:0.8\nrenderDistance:16\n",
  );
  await form("Shared settings");
  await evaluate(
    "[...document.querySelectorAll('.profile-card')].find(c=>c.querySelector('h3').textContent==='Shared settings').querySelector('button[aria-label]').click()",
  );
  await click("button", "Copy to other instances…");
  await until(
    () =>
      evaluate(
        "Boolean(document.querySelector('dialog input[value=prism-fixture-target-a]'))",
      ),
    "Prism chooser",
  );
  for (const id of ["prism-fixture-target-a", "prism-fixture-target-b"]) {
    await evaluate(
      "document.querySelector('dialog input[value=" + id + "]').click()",
    );
  }
  await click("button", "Copy configuration");
  await until(async () => {
    if (
      ["target-a", "target-b"].every((name) =>
        fs
          .readFileSync(path.join(root, name, "options.txt"), "utf8")
          .includes("0.8"),
      )
    )
      return true;
    return evaluate(
      "Boolean(document.querySelector('dialog .error')?.textContent.match(/Close javaw?\\.exe/))",
    );
  }, "Prism activation or running-game protection");
  const protectedByJava = await evaluate(
    "Boolean(document.querySelector('dialog .error')?.textContent.match(/Close javaw?\\.exe/))",
  );
  if (protectedByJava) {
    assert.match(
      fs.readFileSync(path.join(root, "target-a", "options.txt"), "utf8"),
      /0.1/,
    );
    assert.match(
      fs.readFileSync(path.join(root, "target-b", "options.txt"), "utf8"),
      /0.2/,
    );
    const copies = execFileSync(
      "python",
      [
        "-c",
        "import sqlite3,sys;db=sqlite3.connect(sys.argv[1]);print(db.execute(\"SELECT COUNT(*) FROM profiles WHERE game LIKE 'prism-fixture-target-%' AND json_extract(data,'$.name')='Shared settings'\").fetchone()[0])",
        path.join(root, "SConfigData", "library.sqlite3"),
      ],
      { encoding: "utf8" },
    );
    assert.equal(Number(copies.trim()), 2);
    await click("button", "Close", "document.querySelector('dialog')");
    console.log(
      "PASS: Prism copies persisted; running Java blocks activation and preserves both live instances.",
    );
  } else {
    console.log(
      "PASS: Prism bulk activation applied to both temporary instances.",
    );
  }
  await until(
    () => evaluate("!document.querySelector('[aria-busy=true]')"),
    "Prism completion",
  );
  await screenshot("prism-shared");
  console.log(
    "PASS: desktop capture, file watcher, cancel, save & switch, recovery restore.",
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
