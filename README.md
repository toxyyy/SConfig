# SConfig

An offline Windows game configuration manager built with Tauri 2, Rust, SQLite, React, TypeScript, Tailwind CSS and Lucide.

## First switch

1. Open SConfig. The library reads local launcher metadata on startup. Use **Add game** for a manual entry, or open a detected game and select **Set up game**.
2. Select the executable that actually runs while playing. Select the individual settings files to manage. **Browse folder** starts a file picker inside the folder you choose. Repeat to add files from multiple locations.
3. Setup captures **Original settings** automatically. Rename it from its profile menu. Use **Create configuration** for additional snapshots.
4. The active configuration saves game-written changes automatically. To preserve it before experimenting, duplicate or create another configuration and activate that copy first. Close the game before switching configurations.
5. Use **Recovery** to restore a previous set of files. Restoration first saves the current files and clears the active profile because a recovery point may contain unsaved settings.

You can also edit a stored profile outside the game. Open its menu, choose **Open profile folder**, edit the numbered file copies, then choose **Save edits from folder**. Each numbered folder corresponds to one destination in the game's managed file list. Saving edits to the active profile also applies them to the live files after checking that the game is closed.

## Desktop builds

Use Windows 10 or 11 with Microsoft Edge WebView2 Runtime. Development requires Node.js 22.12+ or 24, stable Rust MSVC, Microsoft C++ Build Tools and the Windows SDK. [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/)

~~~powershell
$env:ComSpec = Join-Path $env:SystemRoot "System32/cmd.exe"
npm ci
npm run tauri dev
~~~

Build the release executable and NSIS setup package with the following command.

~~~powershell
npm run tauri build
~~~

The executable is written to **src-tauri/target/release/SConfig.exe**. For portable profile storage, place the executable in a writable directory and launch it with **--portable**. Game destination mappings remain specific to the PC. Use ZIP export and import to move profiles between machines.

## Discovery and coverage

- Steam library manifests, Epic installation manifests, GOG and Ubisoft registry records, EA game records exposing an install directory, and accessible executable game manifests in the standard XboxGames folder on fixed drives. Launcher versions, permissions and custom Xbox library layouts affect coverage.
- Steam cover art comes from the local launcher cache. Manual entries accept a PNG or JPEG. Games without artwork receive a title cover.
- Recognized games use curated configuration paths plus bounded searches across Documents and AppData. Unambiguous matches can be configured automatically. Review ambiguous matches during setup. **Launch in Steam** delegates to the Steam client.
- Library search, favorites, hiding, alphabetical/recent sorting, grid/list views, adjustable covers and a right-click quick switch menu.
- Profile names, descriptions, tags, duplication, import from individual files or ZIP, ZIP export, file comparison, automatic saving of active settings across the library, and configurable recovery retention. Drop an exported ZIP onto a configured game page to start import.
- Text comparison shows line changes for bounded UTF-8 text. Binary and large files receive a content comparison. External editing preserves the original byte format.

## Storage and recovery

The default workspace is **%LOCALAPPDATA%/local.sconfig.configmanager**. A single SQLite database stores game metadata, stable profile IDs, relative file slots, SHA-256 hashes, binary snapshots and the switch journal. **profiles/** contains editable working copies on demand. ZIP archives contain slot numbers and filenames in place of absolute Windows paths.

Before writing live settings, the backend validates the target snapshot, compares bytes, checks the selected executable's process name and commits a recovery snapshot and journal entry. It verifies each temporary file, replaces only the explicitly mapped destination and rereads the result. Unchanged files keep their timestamps. A missing file captured in a recovery point is an explicit absence. The active profile changes only after verification succeeds.

An interrupted operation is checked on startup. If a lock prevents rollback, close the game or editor and use **Recovery → Retry recovery**. If a destination contains a third version that matches neither snapshot, recovery stops and reports the conflict to preserve that version. Profiles support up to 128 files, 16 MiB per file and 64 MiB combined. The default retention is 20 recovery points per game.

## Tests

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml
~~~

To run just the storage and switching tests without the desktop UI dependencies, add **--lib --no-default-features**. Tests use temporary directories and include real Windows sharing locks, binary files, multi-root configurations, crash recovery and rejected archive entries.

After building the release executable, **npm run test:desktop** runs a focused desktop check using registered temporary fixtures under **.test-output/**. It checks active-config auto-save, overview formatting, ADS/graphics sections, thumbnails, history, remove/Undo, list favorites, hidden/global search and the Prism picker. If Java is running, it verifies that the Prism write is blocked and destination bytes remain intact. Use **SCONFIG_TEST_EXECUTABLE** for an executable built with a separate application identifier when a normal instance is running. Close other instances otherwise.

## Library and appearance

Star a cover to pin the game above the rest of the library. Right-click a game or profile to open its actions at the pointer. Removed games and their profiles can be restored from Settings. Cover art uses the local Steam cache with an executable icon fallback when available. The app logo and executable icon use the same artwork.

Settings offers OLED Black, Midnight, Daylight, Forest and Plum. The custom-theme section accepts JSON with a name, a dark/light mode and six-digit hex colors. Load the current theme JSON as a starting point. The app uses a matching title strip with window controls and no duplicated name or icon.

Game setup searches Documents, My Games and relevant AppData folders using bounded searches and recognized settings filenames. Siege account folders appear as separate choices with their available sensitivity and display values. An unambiguous match can be configured automatically. Review manually selected files before adding them. Unknown or registry-only formats still need manual setup. The file picker starts at a detected configuration folder when one is available. ROB RUN includes its LocalLow/Kefir/ROB RUN settings and player_prefs DAT folders.

Profiles show recognized sensitivity, resolution, FOV and other settings directly from their captured bytes. Unknown settings are retained. PNG thumbnails can be selected from the profile's action menu. A cloud-sync notice can be dismissed after reading it. Pause launcher settings sync if it repeatedly replaces switched settings, after checking which copy you want to keep.

## Prism Launcher

Prism's default roaming data folder is checked during a library scan. For portable installations or a custom instance directory, use **Settings → Add Prism Launcher data folder** and choose the folder containing **prismlauncher.cfg**. A single **Prism Launcher** library entry opens its instances, names, versions and available icons. Both **minecraft/options.txt** and **.minecraft/options.txt** layouts are supported. World saves and mod files are excluded.

Use **Copy from…** on the destination instance and choose a source in the popup. Matching managed settings are copied from that source with a recovery point kept first. Close Minecraft first. Java process protection conservatively blocks copying while java.exe or javaw.exe is running.

On first launch, SConfig imports the previous app's library, profiles, recovery points and appearance preferences into its new workspace. Close the previous app before launching SConfig for that first import. The import publishes the new workspace only after its database and editable files have been copied successfully.

## Multiple configuration files

A profile captures and switches all files in its game's managed list together. Rust setup groups client.cfg, keys.cfg, favorites.cfg, keys_default.cfg and autoexec.cfg when present. Use the managed-file button in a game's header to review detected additions or select files from any configuration folder.

For games already configured, adding files saves their current contents into every existing profile. Existing profile contents for the earlier files are preserved. Recovery points created before that addition continue to restore only their original files. Older exported archives can still be imported when their mappings match the beginning of the game's current list. Additional files use their current contents.

## Automatic saves and updates

Changes written by a game to all managed files automatically update its active profile after the files settle. A previous snapshot is kept in Recovery, limited to one auto-save recovery point per 30 seconds to avoid filling the history. Missing or unstable files wait and report a warning rather than replacing the snapshot with incomplete data.

Removing a game is immediate with an eight-second Undo notification. Search includes both visible and hidden games. Existing NSIS installations upgrade in place when the same or newer SConfig installer is run. Application data remains outside the installation folder.
