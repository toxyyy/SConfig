# SConfig

An offline Windows game configuration manager built with Tauri 2, Rust, SQLite, React, TypeScript, Tailwind CSS and Lucide.

## First switch

1. Open SConfig. The library reads local launcher metadata on startup. Use **Add game** for a manual entry, or open a detected game and select **Set up game**.
2. Select the executable that actually runs while playing. Select the individual settings files to manage. **Browse folder** starts a file picker inside the folder you choose. Repeat to add files from multiple locations.
3. Setup captures **Original settings** automatically. Rename it from its profile menu. Use **Create configuration** for additional snapshots.
4. Change settings in the game, close it, then capture a second configuration. Use **Switch to profile** to activate it. If you want to keep the first profile as originally captured, choose **Discard Changes & Switch**. A recovery point preserves the live bytes.
5. Use **Recovery** to restore a previous set of files. Restoration first saves the current files and clears the active profile because a recovery point may contain unsaved settings.

You can also edit a stored profile outside the game. Open its menu, choose **Open profile folder**, edit the numbered file copies, then choose **Save edits from folder**. Each numbered folder corresponds to one destination in the game's managed file list. Changes to an active profile's stored copy can be applied with **Revert** on the game page.

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
- Terraria and Stardew Valley Steam entries suggest specific settings files if present. Review them during setup. Other games use explicit file selection. **Launch in Steam** delegates to the Steam client.
- Library search, favorites, hiding, alphabetical/recent sorting, grid/list views, adjustable covers and a right-click quick switch menu.
- Profile names, descriptions, tags, duplication, import from individual files or ZIP, ZIP export, file comparison, change monitoring while the game page is open, and configurable recovery retention. Drop an exported ZIP onto a configured game page to start import.
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

After building the release executable, **npm run test:desktop** exercises the Windows file pickers, manual registration, profile creation, unsaved-changes prompt and recovery through the desktop interface. It creates a separate portable workspace under **.test-output/** and saves screenshots there. Close other SConfig instances first because the app enforces a single running instance.



## Library and appearance

Star a cover to pin the game above the rest of the library. Right-click a game or profile to open its actions at the pointer. Removed games and their profiles can be restored from Settings. Cover art uses the local Steam cache with an executable icon fallback when available. The app logo and executable icon use the same artwork.

Settings offers Midnight, Daylight, Forest and Plum. The custom-theme section accepts JSON with a name, a dark/light mode and six-digit hex colors. Load the current theme JSON as a starting point. The native Windows title bar follows the selected theme. Exact caption colors require Windows 11 support.

Game setup searches Documents, My Games and relevant AppData folders using bounded searches and recognized settings filenames. Siege account folders appear as separate choices with their available sensitivity and display values. An unambiguous match can be configured automatically. Review manually selected files before adding them. Unknown or registry-only formats still need manual setup. The file picker starts in Documents/My Games.

Profiles show recognized sensitivity, resolution, FOV and other settings directly from their captured bytes. Unknown settings are retained. PNG thumbnails can be selected from the profile's action menu. A cloud-sync notice can be dismissed after reading it. Pause launcher settings sync if it repeatedly replaces switched settings, after checking which copy you want to keep.

## Prism Launcher

Prism's default roaming data folder is checked during a library scan. For portable installations or a custom instance directory, use **Settings → Add Prism Launcher data folder** and choose the folder containing **prismlauncher.cfg**. Instances with **.minecraft/options.txt** appear as ordinary library entries and receive an original-settings profile. World saves and mod files are excluded.

Use a Prism profile's **Copy to other instances** action to select destinations. Activation after copying uses the same backups and unsaved-settings prompts as an individual switch. Cancel stops the remaining activations. Instances activated earlier in the sequence keep their completed state and recovery points. Close Minecraft first. Java process protection conservatively blocks activation while java.exe or javaw.exe is running.

The desktop smoke test uses registered temporary fixtures by default. Pass **--native** to the Node script to additionally exercise the Windows registration and thumbnail pickers. Native-picker automation depends on the interactive Windows dialog layout. If Java is running, the Prism check verifies that copies persist while activation is blocked and destination files remain intact.

On first launch, SConfig imports the previous app's library, profiles, recovery points and appearance preferences into its new workspace. Close the previous app before launching SConfig for that first import. The import publishes the new workspace only after its database and editable files have been copied successfully.
