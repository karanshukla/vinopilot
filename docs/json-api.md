# The JSON API

`vinopilot --json` is the contract between vinoPilot's core and any frontend not written in Rust (the planned Python TUI). Breaking it needs a changelog entry. `crates/cli/tests/cli.rs` pins it.

## list and check

`vinopilot list --json` (offline) and `vinopilot check --json` (online) print one document:

```json
{
  "vinopilot": "0.1.0",
  "source_root": "/home/you/Development",
  "components": [
    {
      "id": "vinosearch",
      "name": "vinoSearch",
      "blurb": "Find files by what they're about",
      "repo": "karanshukla/vinoSearch",
      "channels": ["release", "source"],
      "registry": null,
      "needs_root": false,
      "installed": { "channel": "release", "version": "8.0.0" },
      "checkout": { "path": "/home/you/Development/vinoSearch", "describe": "v8-9-g1a2b3c4" },
      "system_package": null,
      "shadowed": [],
      "latest": {
        "release": { "value": "v8" },
        "package": null,
        "behind": { "value": 9 }
      },
      "update": { "state": "up_to_date" }
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | what the command line takes: `vinowhisper`, `ovfetch`, `vinouserinterface`, `vinopilot` |
| `channels` | the ways it can be installed, best first: `release`, `source`, `package` (PyPI or crates.io), `flatpak` (the release's Flatpak bundle: vinowhisper and vinosearch) |
| `registry` | `"pypi"` or `"crates.io"` when `channels` has `package`, else `null` |
| `needs_root` | its actions run through `pkexec` |
| `installed` | `null`, or how it's installed and its version (`null` when the binary wouldn't say, `"dev"` for a checkout build). `channel` is one of `channels`, or for vinopilot itself also `deb`, `rpm` or `flatpak`: its release's package, which `update` and `uninstall` go through but `install` doesn't offer. A Flatpak's version is its metainfo's newest release, `null` without one |
| `checkout` | `null`, or the git checkout in the source folder |
| `system_package` | `null`, or the distro packages (rpm or deb) that own its commands in `/usr/bin`: `{"kind": "rpm", "packages": ["vinoopeneffects"], "remove": "sudo dnf remove vinoopeneffects"}`. vinopilot doesn't remove these; `remove` is the command for the user to run |
| `shadowed` | its commands that are in both `~/.local/bin` and `~/.cargo/bin`, say a release install left beside a source build: `[{"command": "vinosearch", "runs": "/home/you/.local/bin/vinosearch", "hidden": "/home/you/.cargo/bin/vinosearch"}]`. `runs` is the one first on `PATH`. Empty when there's one copy |
| `latest` | only from `check`. Each lookup is `null` (not applicable), `{"value": ...}` or `{"error": "sentence"}` |
| `update` | `{"state": "available", "to": "v1.3.1"}`, `{"state": "up_to_date"}` or `{"state": "unknown", "reason": null or "sentence"}`, judged against the source that matches how it's installed |

## install, update, uninstall

One JSON object per line, as the installer runs:

```json
{"event":"line","text":"== Downloading the latest release from https://..."}
{"event":"line","text":"checksum verified"}
{"event":"done","app":"vinowatch","channel":"release","result":{"outcome":"done"}}
```

`result` is `{"outcome": "done"}`, `{"outcome": "stopped"}` or `{"outcome": "failed", "reason": "the installer's ✗ line, or its last"}`. The exit code says the same: 0, or 1. `uninstall` needs `--yes` without a terminal.

## Other commands

- `vinopilot steps APP ACTION [--via WAY]`: the exact shell an action runs, for showing before running.
- `vinopilot config --json`: `{"source_root": "..."}`. `vinopilot config source_root DIR` sets it; the window reads the same file (`~/.config/vinopilot/config`).
