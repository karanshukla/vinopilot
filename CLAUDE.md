# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

vinoPilot: installs, updates and uninstalls the vinoPilot apps (vinoWhisper, vinoAuthFace, vinoOpenEffects, vinoPop, vinoSpeak, vinoSearch, vinoWatch), ovfetch, the vinoUserInterface library and itself, from their releases, a source checkout, PyPI or crates.io. User docs are the README; don't duplicate them here.

## Layout

A cargo workspace plus a Python package beside it. Keep repo-level tooling language-neutral.

- `crates/core` (`vinopilot-core`): no UI, no egui.
  - `catalog.rs`: the components and, per (component, channel, action), the shell steps. Each step is a script run with `bash -c` (or `pkexec env ... bash -c` when `root`), with a shared prelude (`release_run`, `checkout`, `need_*`). Paths and settings reach scripts as `$CHECKOUT`, `$WORKTREE`, `$OFF_BRANCH`, `$REPO_URL`, `$RELEASE` env vars, never formatted in.
  - `detect.rs`: how each is installed, by looking: `<app>-upgrade` markers in `~/.local/bin`, uv receipts, `~/.cargo/.crates.toml`, `--version` (2 s timeout).
  - `remote.rs`: GitHub latest release, PyPI, crates.io, `git fetch` + behind count. The only module that goes online.
  - `status.rs`: installed + latest into "update available", and the `--json` component shape.
  - `job.rs`: runs steps in a background thread, streams lines through a `Notify` callback.
  - `lib.rs` `command()`: every child process goes through it, for the `PATH` fix-up (`~/.local/bin`, `~/.cargo/bin`).
- `crates/cli` (`vinopilot`): hand-rolled args, `--json` is the API (docs/json-api.md, pinned by tests/cli.rs).
- `tui/` (`vinopilot` on PyPI, command `vinopilot-tui`): Textual. `cli.py` is the only thing that runs the command (`--json`, docs/json-api.md); `app.py` is the table, the log and the modal screens. It knows no catalog: names, channels, registries and update states all come from the JSON. Tests run against `tests/fake_vinopilot.py`, never cargo; `VINOPILOT_BIN=target/debug/vinopilot` tries it on the real thing.
- `crates/gui` (`vinopilot-gui`): lib (`Pilot`) + thin main. A 3x3 grid (`CARDS`: seven apps, ovfetch, vinoUserInterface; one column when three don't fit), log panel at the bottom. vinoPilot has no card: its version and self-update are in the header, and Update all skips it.
- `packaging/`: the .deb's `control`, the .rpm's spec, the Flatpak's manifest and its `host-run.sh`, and the metainfo all three share. `.github/package-distro.sh` and `package-flatpak.sh` wrap the tarball's two binaries with them; nothing is rebuilt. The menu entry and icon come from `vinopilot-gui --install-desktop` into the staging tree, not from a second copy here.

## Commands

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
cargo build -p vinopilot -p vinopilot-gui && tests/install-e2e.sh target/debug
.github/package-release.sh && tests/release-smoke.sh dist X.Y.Z   # needs the musl target, dpkg-deb, rpmbuild
cargo test -p vinopilot-gui --test screenshots -- --ignored   # regenerates docs/images
cd tui && uv run --frozen pytest -q && uv run --frozen ruff check . && uv run --frozen ruff format --check .
```

## Gotchas

- **Each app's installer is the source of truth.** Read the app's `install.sh` (and `scripts/install.sh`, `uninstall.sh`) before changing its steps; flags differ per app (vinoWatch's is POSIX `sh`, vinoAuthFace's needs root, vinoOpenEffects' uninstaller shipped only in the repo before `vinoopeneffects-upgrade --uninstall`, vinoWhisper's is `vinowhisper-upgrade --uninstall` in newer releases and the catalog's own `WHISPER_UNINSTALL` otherwise). Uninstall removes an app's models and keeps the user's data; that's each app's uninstaller's call, so don't add per-app purges here (vinoWhisper's fallback is the exception). `every_script_parses` only checks syntax.
- **pkexec clears the environment.** Root steps get their vars through `pkexec env`, plus `SUDO_USER` so vinoAuthFace's `deploy.sh` builds and finds the desktop user as itself. A pkexec child is root's: `Job::stop` can't kill it.
- **`core` forbids unsafe**, so no `env::set_var`; that's why `command()` sets `PATH` per child.
- **The release CLI is static musl, the window is glibc.** eframe dlopens libEGL/wayland/X11, which static musl can't. The window is built on ubuntu-22.04 for an old glibc floor.
- **vinoPilot is `Id::App(App::Pilot)`** (vino-ui's `App` has listed it since 0.4.0), and it updates itself through the catalog's steps like any other component, not vino-ui's `Updater`, so it covers every channel. `repo()` keeps it lowercase (`karanshukla/vinopilot`) to match the release URLs and the checkout folder.
- **`Pilot::ui` doesn't call `theme::follow_desktop`**; the eframe `App` impl does. Otherwise the screenshot test's light render comes out dark.
- **The TUI's command can't be `vinopilot`.** `uv tool install` writes scripts to `~/.local/bin`, where the release installer puts the Rust command; a same-named script would replace it.
- **The Flatpak runs on the host.** `host-run.sh` is `/app/bin/vinopilot{,-gui}` and `flatpak-spawn --host`s the real binary from the app's files (`app-path` in `/.flatpak-info`), because every step needs the host's home, tools and pkexec. So core has no Flatpak mode, and `org.freedesktop.Flatpak` is the bundle's one permission (tests/flatpak-smoke.sh fails on a second).
- **App Flatpaks are `Id::flatpak`** (vinoWhisper, vinoSearch): `<slug>-x86_64.flatpak` in the app's release, app id `io.github.karanshukla.<slug>`, offered last in `channels()`. Installed `--user` unless it's already system-wide; found by its files only when there's no install in the home, with the version from its metainfo (the binary only runs in its sandbox). `fetch` checks against `SHA256SUMS`, or `<asset>.sha256` in a release without one (vinoSearch's). Uninstall keeps `~/.var/app/<id>`.
- **`Channel::Deb` and `Rpm` are vinoPilot's alone, and so is its own `Flatpak`**, found by `detect::packaged` (the package owning `/usr/bin/vinopilot`, else the Flatpak's files) only when there's no copy in the home, and in no `channels()`. Their steps `fetch` the release's file, check it against `SHA256SUMS`, and call the package manager; they aren't `root` steps: only that one command goes through `as_root` (pkexec, or nothing when already root, which is how tests/package-install.sh runs them). No repository is behind the packages. `VINOPILOT_RELEASE_BASE` moves vinoPilot's release for the tests, as in install.sh.
- **Release asset names are unversioned** (`vinopilot-x86_64.deb`), so `releases/latest/download/<name>` keeps working. A new asset goes in `SHA256SUMS`, `attest`'s subjects and `publish`'s list.
- **A checkout off its default branch is never pulled.** `checkout` is one base path (clone, or `git pull --ff-only` on the default branch); off it, `Id::off_branch()` picks the path, reaching the prelude as `$OFF_BRANCH`. `OffBranch::Worktree` builds origin's default branch in `$WORKTREE` (`Paths::worktree`) and points `CHECKOUT` there for the rest of that step; a separate root step finds it with `source_dir`. `checkout_done` records the commit as `refs/vinopilot/installed` (what `remote::behind` counts from) and removes the worktree. `OffBranch::Switch` is for vinoWhisper, vinoPop and vinoUserInterface, which run from the folder they were installed from: it stashes, switches the checkout itself to the default branch, and stays there (a kept worktree would be a second venv the app quietly runs from). The worktree can't live inside the checkout: cargo and uv would find the checkout's workspace above it.
- **Nothing goes online until asked.** `list` and the window's startup scan are local; only `check` / Check for updates call `remote`.

## CI and repo rules

- Actions pinned to commit SHAs with a version comment; Dependabot with a 7-day cooldown, nothing auto-merges.
- Conventional commits; CHANGELOG.md from `git cliff`. A release tag needs a matching `## [x.y.z]` section, a `<release>` line at the top of `packaging/io.github.karanshukla.vinopilot.metainfo.xml`, and every crate at that version.
- Releases: build (no write token; tarball, .deb, .rpm), flatpak (the bundle, on ubuntu-24.04), attest, publish, then crates.io by trusted publishing behind the `crates-io` environment's approval, core first.
