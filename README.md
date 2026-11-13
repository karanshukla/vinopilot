# vinoPilot

One place to install, update and uninstall the [vinoPilot](https://vinopilot.ca/) apps. Each app already has its own hash-checked installer; vinoPilot knows which one to run, how each app got onto your machine, and whether there's something newer for that way in.

![The vinoPilot window in the dark theme: a grid of app cards, each with its version, how it was installed, and Update or Install](docs/images/window-dark.png)

It manages:

| Component | Release | Source | Package |
|---|---|---|---|
| [vinoWhisper](https://github.com/karanshukla/vinoWhisper) | yes | yes | [PyPI](https://pypi.org/project/vinowhisper/) |
| [vinoAuthFace](https://github.com/karanshukla/vinoAuthFace) | yes (asks for your password) | yes | |
| [vinoOpenEffects](https://github.com/karanshukla/vinoOpenEffects) | yes | yes | |
| [vinoPop](https://github.com/karanshukla/vinoPop) | yes | yes | |
| [vinoSpeak](https://github.com/karanshukla/vinoSpeak) | yes | yes | [PyPI](https://pypi.org/project/vinospeak/) |
| [vinoSearch](https://github.com/karanshukla/vinoSearch) | yes | yes | |
| [vinoWatch](https://github.com/karanshukla/vinoWatch) | yes | yes | |
| [ovfetch](https://github.com/karanshukla/ovfetch) | yes | yes | [crates.io](https://crates.io/crates/ovfetch) |
| [vinoUserInterface](https://github.com/karanshukla/vinoUserInterface) | | a checkout that builds | a library, nothing to install |
| vinoPilot itself | yes | yes | [crates.io](https://crates.io/crates/vinopilot) |

- **Release** runs the app's own one-line installer from its latest GitHub release, which checks the download against the release's `SHA256SUMS`. Updates go through the `<app>-upgrade` copy each installer keeps.
- **Source** clones the repo into your source folder (`~/Development` if you have one, else `~/src`; change it in the window or with `vinopilot config source_root DIR`), then runs the checkout's own install script. Updating is `git pull --ff-only` and the same again. A checkout on another branch is left alone: vinoPilot checks out the default branch in `~/.local/share/vinopilot/worktrees/<repo>`, installs from that, then removes it. vinoWhisper, vinoPop and vinoUserInterface run from their checkout, so for those the checkout itself switches to the default branch, with uncommitted changes stashed first; the log says how to get back.
- **Package** is `uv tool` for PyPI and `cargo install` for crates.io.

vinoPilot works out how each app is installed by looking, not by keeping its own records: the `-upgrade` scripts the release installers leave, `uv`'s tool receipts, cargo's `.crates.toml`, and each binary's `--version`. So it also sees apps you installed by hand before vinoPilot existed.

Nothing goes online until you press **Check for updates** (or run `vinopilot check`). Then it asks GitHub, PyPI and crates.io for the newest versions and runs `git fetch` in each checkout.

## Install

Download the Flatpak, the .deb or the .rpm from the [latest release](https://github.com/karanshukla/vinopilot/releases/latest). Each holds `vinopilot` (the command) and `vinopilot-gui` (the window), and adds vinoPilot to your app menu. Opening the downloaded Flatpak in Discover or GNOME Software installs it too.

```sh
# Flatpak: any distro, including rpm-ostree ones (Silverblue, Kinoite, Bazzite)
curl -fsSLO https://github.com/karanshukla/vinopilot/releases/latest/download/vinopilot-x86_64.flatpak
flatpak install --user ./vinopilot-x86_64.flatpak

# Debian, Ubuntu, Mint
curl -fsSLO https://github.com/karanshukla/vinopilot/releases/latest/download/vinopilot-x86_64.deb
sudo apt install ./vinopilot-x86_64.deb

# Fedora
sudo dnf install https://github.com/karanshukla/vinopilot/releases/latest/download/vinopilot-x86_64.rpm
```

The Flatpak isn't sandboxed: vinoPilot installs apps on your system with your own `bash`, `git`, `uv`, `cargo`, `flatpak` and `pkexec`, so its only permission is the one to run on the host (`org.freedesktop.Flatpak`), and the window and `flatpak run --command=vinopilot io.github.karanshukla.vinopilot` run there. The apps it installs go into your home through their own installers, as with every other way in, or as their own Flatpaks where they have one.

### Updating

vinoPilot updates itself the way it was installed: the version button in the header (or `vinopilot update vinopilot`) downloads the new release's Flatpak, .deb or .rpm, checks it against the release's `SHA256SUMS`, and hands it to `flatpak`, `apt` or `dnf`. The .deb and .rpm ask for your password for that one command. There's no Flatpak, apt or dnf repository behind them, so your software centre and system updater don't see new versions.

### Uninstalling

The window has no Uninstall for vinoPilot itself. Remove it the way you installed it:

```sh
flatpak uninstall --user io.github.karanshukla.vinopilot   # the Flatpak
sudo apt remove vinopilot                                  # the .deb
sudo dnf remove vinopilot                                  # the .rpm
```

or with Remove in your software centre. `vinopilot uninstall vinopilot` works for every way in (from the Flatpak: `flatpak run --command=vinopilot io.github.karanshukla.vinopilot uninstall vinopilot`). The apps vinoPilot installed stay; uninstall those first if you want them gone.

### Other ways in

The one-line installer puts the same two binaries in `~/.local/bin`, no sudo:

```sh
curl -fsSL https://github.com/karanshukla/vinopilot/releases/latest/download/install.sh | bash
```

`| bash -s -- --no-gui` skips the window; `| bash -s -- --uninstall` removes it all.

From crates.io, or a checkout:

```sh
cargo install --locked vinopilot vinopilot-gui && vinopilot-gui --install-desktop
./install.sh                     # in a checkout: builds, then installs as above
```

`--install-desktop` adds the app menu entry and icon (install.sh does it for you); `--uninstall-desktop` takes them out. Uninstall with `vinopilot-gui --uninstall-desktop && cargo uninstall vinopilot vinopilot-gui`, or `./install.sh --uninstall` in the checkout.

x86_64 only for the release binaries, same as the apps it installs.

## The window

Nine cards, three by three: the seven apps, ovfetch and vinoUserInterface. Each has its version, how it's installed (the tag top right), and **Update** or **Install**. Before installing, the switch on the card picks the way in. **Update all** appears once a check finds something. vinoPilot's own version is a button next to its name in the header: it checks for a newer vinoPilot, turns into **update to vX** when there is one, then **Restart into vX** once that's installed. The ⓘ beside it shows how vinoPilot is installed, its checkout, and where the command, the window and the terminal UI are, with their versions. The installer's output streams into a panel underneath, with **Stop**. vinoAuthFace installs system-wide, so its actions go through `pkexec` and ask for your password.

It follows the desktop's light or dark setting, and drops to one column when three don't fit, like when snapped to half a laptop screen:

<p align="center">
  <img src="docs/images/window-light.png" width="64%" alt="The window in the light theme">
  <img src="docs/images/narrow-dark.png" width="27%" alt="The window at 420 px wide: one column of cards">
</p>

## The command

```sh
vinopilot list                    # what's installed, and how (offline)
vinopilot check                   # plus the newest version of each (online)
vinopilot install whisper --via pypi
vinopilot update --all
vinopilot uninstall pop
vinopilot steps search install    # print the shell an action would run, without running it
```

App names work with or without the `vino`. `--via` takes `release` (the default), `source`, `pypi` or `crates`, and `flatpak` for vinoWhisper and vinoSearch: their release's Flatpak bundle, checked against the release's checksums and installed for your user with `flatpak install --user`. Updating downloads the new release's bundle over it; uninstalling leaves the app's data in `~/.var/app`. Exit codes: 0 done, 1 failed, 2 bad usage.

`--json` turns it into an API: `list` and `check` print one document, and actions print one event per line. It's what a frontend in another language should drive instead of reimplementing the catalog; [docs/json-api.md](docs/json-api.md) has the shape.

## Layout

| Path | What |
|---|---|
| `crates/core` | `vinopilot-core`: the catalog (each app's install, update and uninstall steps per way in), detection, version checks, the job runner. No UI. |
| `crates/cli` | `vinopilot`: the command, and the `--json` API |
| `crates/gui` | `vinopilot-gui`: the egui window, drawn with [vinoUserInterface](https://github.com/karanshukla/vinoUserInterface) |
| `install.sh` | the one-line installer, and the source install from a checkout |
| `packaging/` | what the .deb, the .rpm and the Flatpak add around the release's binaries |

| `tui/` | `vinopilot-tui`: the same in a terminal, from PyPI (`uv tool install vinopilot`). A thin [Textual](https://textual.textualize.io/) frontend over `vinopilot --json`; it offers to fetch the command when it isn't there. Updating or uninstalling vinoPilot takes it along. |

## Limits

- **Uninstalling an app removes its downloaded models** and keeps your own data and settings (transcripts, indexes, face templates). Each app's own uninstaller decides what that covers; vinoPilot runs it. Shared folders such as `~/.cache/huggingface` and the runtime ovfetch fetched stay.
- **Only newer vinoWhisper releases have an uninstaller** (`vinowhisper-upgrade --uninstall`), and vinoPilot uses it when it's there. Otherwise vinoPilot's own undoes what the installer writes (the `uv` tool, the units, the tray and window, the menu entries) and deletes the models in `~/.local/share/vinowhisper/models` and `~/.cache/vinowhisper`; transcripts, sessions and settings stay.
- **A vinoSpeak installed from PyPI keeps its models** when uninstalled: there's no checkout or `vinospeak-upgrade` to run its uninstaller from. They're in `~/.cache/vinospeak`.
- **Uninstalling a source install moves its checkout to the trash** (with `gio`, `kioclient` or `trash-put`), unless it has uncommitted, unpushed or stashed work: then it's kept and the uninstall says why. Copies an old `cargo install` left in `~/.cargo/bin` go too.
- **Detection is a best guess for hand installs.** An app with no `-upgrade` script, no uv receipt and no cargo record but a binary in `~/.local/bin` shows as a source install.
- **vinoAuthFace's channel comes from its version.** Release builds are stamped with their tag, checkouts say `dev`.
- Installers that want to ask you something get no terminal; each runs with its non-interactive defaults (vinoWhisper and vinoSpeak with `--gui`).

[vinopilot.ca](https://vinopilot.ca/) has the apps themselves. Issues and PRs welcome, see [CONTRIBUTING.md](CONTRIBUTING.md).
