"""Create isolated Prism records for the desktop activation-queue regression test."""
import json
from pathlib import Path
import sqlite3
import sys

root = Path(sys.argv[1]).resolve()
workspace = root / "SConfigData"
workspace.mkdir()
db = sqlite3.connect(workspace / "library.sqlite3")
db.execute("CREATE TABLE games(id TEXT PRIMARY KEY,data TEXT NOT NULL,artwork BLOB)")
for label, sensitivity in [("source", "0.7"), ("target-a", "0.1"), ("target-b", "0.2")]:
    instance = root / label
    instance.mkdir()
    (instance / "options.txt").write_text("mouseSensitivity:" + sensitivity + "\nrenderDistance:12\n")
    game = dict(id="prism-fixture-" + label, name="Prism " + label,
                platform="Prism Launcher", install=str(instance),
                exe=str(root / "fixture" / "FixtureGame.exe"),
                maps=[dict(root="\\\\?\\" + str(instance), name="options.txt")],
                active=None, favorite=False, hidden=False, modified=0, cover=False)
    db.execute("INSERT INTO games(id,data) VALUES(?,?)", (game["id"], json.dumps(game)))
if "--registered" in sys.argv:
    instance = root / "fixture"
    game = dict(id="fixture-game", name="FixtureGame", platform="Other", install=str(instance),
                exe=str(instance / "FixtureGame.exe"),
                maps=[dict(root="\\\\?\\" + str(instance), name="settings.ini")],
                active=None, favorite=False, hidden=False, modified=0, cover=False)
    db.execute("INSERT INTO games(id,data) VALUES(?,?)", (game["id"], json.dumps(game)))
db.commit()
db.close()
