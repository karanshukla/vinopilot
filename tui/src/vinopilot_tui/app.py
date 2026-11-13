"""The window, in a terminal: a table of the components, the installer's
output underneath, one key per action. Every action is a `vinopilot` run."""

from __future__ import annotations

import argparse
import os

from rich.text import Text
from textual import work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import (
    Button,
    DataTable,
    Footer,
    Input,
    Label,
    OptionList,
    RichLog,
    Static,
)
from textual.widgets.option_list import Option

from vinopilot_tui import __version__, cli
from vinopilot_tui.cli import CommandError, Event, Job, Pilot

CHANNEL_LABEL = {
    "release": "Release",
    "source": "Source",
    "flatpak": "Flatpak",
    # vinopilot's own release, as a package.
    "deb": "deb package",
    "rpm": "rpm package",
}
# The command's own component: in the status line, not the table.
SELF = "vinopilot"
ACTIONS = ["install", "update", "uninstall"]


def channel_label(component: dict, channel: str) -> str:
    """`Release`, `Source`, `PyPI` or `crates.io`."""
    if channel == "package":
        return {"pypi": "PyPI", "crates.io": "crates.io"}.get(
            component.get("registry") or "", "Package"
        )
    return CHANNEL_LABEL.get(channel, channel)


def tilde(path: str) -> str:
    home = os.path.expanduser("~")
    return "~" + path[len(home) :] if path.startswith(home) else path


def about(c: dict) -> Text:
    """The blurb, or which copy runs when there are two."""
    shadowed = c.get("shadowed") or []
    if not shadowed:
        return Text(c.get("blurb", ""), style="dim")
    s = shadowed[0]
    runs, hidden = tilde(s["runs"]), tilde(s["hidden"])
    return Text(f"Two copies: {runs} runs, {hidden} doesn't", style="yellow")


class ChooseScreen(ModalScreen[str | None]):
    """One of a few options, by id."""

    BINDINGS = [Binding("escape", "dismiss(None)", "Cancel")]

    def __init__(self, title: str, options: list[tuple[str, str]]) -> None:
        super().__init__()
        self.title_text = title
        self.options = options

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label(self.title_text, classes="title")
            yield OptionList(*(Option(label, id=key) for key, label in self.options))
            yield Label("Enter chooses, Esc cancels", classes="hint")

    def on_mount(self) -> None:
        option_list = self.query_one(OptionList)
        option_list.highlighted = 0
        option_list.focus()

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        self.dismiss(event.option.id)


class ConfirmScreen(ModalScreen[bool]):
    BINDINGS = [
        Binding("y", "dismiss(True)", "Yes"),
        Binding("n", "dismiss(False)", "No"),
        Binding("escape", "dismiss(False)", "Cancel", show=False),
    ]

    def __init__(self, question: str, detail: str = "") -> None:
        super().__init__()
        self.question = question
        self.detail = detail

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label(self.question, classes="title")
            if self.detail:
                yield Static(self.detail)
            with Horizontal(classes="buttons"):
                yield Button("Yes", id="yes", variant="error")
                yield Button("No", id="no", variant="primary")

    def on_mount(self) -> None:
        self.query_one("#no", Button).focus()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        self.dismiss(event.button.id == "yes")


class TextScreen(ModalScreen[None]):
    """Read-only text, such as the shell an action runs."""

    BINDINGS = [
        Binding("escape", "dismiss", "Close"),
        Binding("q", "dismiss", "Close", show=False),
    ]

    def __init__(self, title: str, text: str) -> None:
        super().__init__()
        self.title_text = title
        self.text = text

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label(self.title_text, classes="title")
            with VerticalScroll():
                yield Static(Text(self.text))
            yield Label("Esc closes", classes="hint")


class InputScreen(ModalScreen[str | None]):
    BINDINGS = [Binding("escape", "dismiss(None)", "Cancel")]

    def __init__(self, title: str, value: str) -> None:
        super().__init__()
        self.title_text = title
        self.value = value

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label(self.title_text, classes="title")
            yield Input(value=self.value)
            yield Label("Enter saves, Esc cancels", classes="hint")

    def on_mount(self) -> None:
        self.query_one(Input).focus()

    def on_input_submitted(self, event: Input.Submitted) -> None:
        self.dismiss(event.value.strip() or None)


class MissingScreen(ModalScreen[bool]):
    """No `vinopilot` command: offer the release installer, or leave."""

    BINDINGS = [Binding("escape", "dismiss(False)", "Quit", show=False)]

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label("The vinopilot command isn't installed", classes="title")
            yield Static(
                "This terminal UI drives the vinopilot command, which does the "
                "installing. Get it from the latest release? Its installer checks "
                "the download against the release's SHA256SUMS and puts the command "
                "in ~/.local/bin, no sudo.\n\n"
                f"Or in a shell:\n  curl -fsSL {cli.INSTALLER} | bash -s -- --no-gui"
            )
            with Horizontal(classes="buttons"):
                yield Button("Install", id="install", variant="primary")
                yield Button("Quit", id="quit")
            yield RichLog(id="missing-log", markup=False, highlight=False, wrap=True)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "quit":
            self.dismiss(False)
            return
        event.button.disabled = True
        self.install()

    @work(thread=True, exclusive=True)
    def install(self) -> None:
        log = self.query_one(RichLog)
        job = cli.installer()
        for event in job.events():
            if event.kind == "line":
                self.app.call_from_thread(log.write, event.text)
            elif event.outcome == "done" and cli.find():
                self.app.call_from_thread(self.dismiss, True)
                return
            else:
                why = event.reason or "the command still isn't there"
                self.app.call_from_thread(log.write, f"✗ Not installed: {why}")
        self.app.call_from_thread(self.query_one("#install", Button).__setattr__, "disabled", False)


class PilotApp(App[None]):
    TITLE = "vinoPilot"
    CSS_PATH = "app.tcss"
    BINDINGS = [
        Binding("c", "check", "Check for updates"),
        Binding("i", "install", "Install"),
        Binding("u", "update", "Update"),
        Binding("a", "update_all", "Update all"),
        Binding("p", "pilot", "vinoPilot"),
        Binding("x", "uninstall", "Uninstall"),
        Binding("s", "steps", "Steps"),
        Binding("r", "source_root", "Source folder"),
        Binding("escape", "stop", "Stop", show=False),
        Binding("q", "quit", "Quit"),
    ]

    def __init__(self) -> None:
        super().__init__()
        self.pilot: Pilot | None = None
        self.version = ""
        self.components: list[dict] = []
        self.me: dict = {}
        # vinoPilot updated itself; p restarts into the new one.
        self.updated = False
        self.restart = False
        self.checked = False
        self.scanning = False
        self.job: Job | None = None
        self.status_text = ""

    def compose(self) -> ComposeResult:
        yield Static("", id="status")
        yield DataTable(id="table", cursor_type="row", zebra_stripes=True)
        yield RichLog(id="log", markup=False, highlight=False, wrap=True)
        yield Footer()

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("App", "Version", "Via", "Update", "About")
        found = cli.find()
        if found is None:
            self.push_screen(MissingScreen(), self.after_missing)
        else:
            self.attach(found)

    def after_missing(self, installed: bool | None) -> None:
        found = cli.find() if installed else None
        if found is None:
            self.exit()
        else:
            self.log_line("✓ Installed the vinopilot command")
            self.attach(found)

    def attach(self, binary: str) -> None:
        self.pilot = Pilot(binary)
        try:
            self.version = self.pilot.version()
        except (CommandError, OSError) as e:
            self.log_line(f"✗ {binary} won't run: {e}")
            return
        self.scan(False)

    # --- the table ---------------------------------------------------------

    @work(thread=True, exclusive=True, group="scan")
    def scan(self, online: bool) -> None:
        assert self.pilot is not None
        self.call_from_thread(self.set_scanning, True, online)
        try:
            doc = self.pilot.check() if online else self.pilot.list()
        except (CommandError, OSError) as e:
            self.call_from_thread(self.log_line, f"✗ {e}")
            self.call_from_thread(self.set_scanning, False, online)
            return
        self.call_from_thread(self.show, doc, online)

    def set_scanning(self, scanning: bool, online: bool) -> None:
        self.scanning = scanning
        if scanning:
            self.set_status("Checking for updates…" if online else "Looking…")
        else:
            self.set_status()

    def show(self, doc: dict, online: bool) -> None:
        everything = list(doc.get("components", []))
        self.me = next((c for c in everything if c["id"] == SELF), {})
        self.components = [c for c in everything if c["id"] != SELF]
        self.checked = self.checked or online
        self.source_root = str(doc.get("source_root", ""))
        table = self.query_one(DataTable)
        row = table.cursor_row
        table.clear()
        for c in self.components:
            table.add_row(*self.cells(c), key=c["id"])
        if self.components:
            table.move_cursor(row=min(row, len(self.components) - 1))
        self.set_scanning(False, online)

    def cells(self, c: dict) -> tuple:
        installed = c.get("installed")
        update = c.get("update") or {}
        if installed:
            version = installed.get("version") or "?"
            via = channel_label(c, installed["channel"])
        else:
            version, via = "-", Text("not installed", style="dim")
        if not self.checked:
            state: str | Text = ""
        elif not installed:
            latest = (c.get("latest") or {}).get("release") or {}
            state = Text(f"latest {latest['value']}", style="dim") if "value" in latest else ""
        elif update.get("state") == "available":
            state = Text(f"{update['to']} available", style="bold")
        elif update.get("state") == "up_to_date":
            state = Text("up to date", style="dim")
        else:
            state = Text(update.get("reason") or "?", style="dim")
        return (c["name"], version, via, state, about(c))

    def set_status(self, busy: str = "") -> None:
        parts = [self.self_status()]
        if getattr(self, "source_root", ""):
            parts.append(f"source folder {tilde(self.source_root)}")
        if busy:
            parts.append(busy + ("" if self.job is None else "  (Esc stops)"))
        elif self.checked:
            n = sum(
                1 for c in self.components if (c.get("update") or {}).get("state") == "available"
            )
            parts.append(
                f"{n} update{'s' if n != 1 else ''} available" if n else "no updates found"
            )
        self.status_text = " · ".join(parts)
        self.query_one("#status", Static).update(self.status_text)

    def self_status(self) -> str:
        """vinoPilot's version, and what p does with it."""
        mine = f"vinoPilot {self.version}" if self.version else "vinoPilot"
        installed = (self.me.get("installed") or {}).get("version")
        if self.updated and installed and installed != self.version:
            return f"vinoPilot {installed} installed, p restarts into it"
        update = self.me.get("update") or {}
        if update.get("state") == "available" and self.me.get("installed"):
            return f"{mine}, p updates to {update['to']}"
        if update.get("state") == "up_to_date":
            return f"{mine} (up to date)"
        return mine

    def selected(self) -> dict | None:
        table = self.query_one(DataTable)
        if not self.components or table.cursor_row >= len(self.components):
            self.notify("Nothing selected", severity="warning")
            return None
        return self.components[table.cursor_row]

    def ready(self) -> bool:
        """Whether an action can start now."""
        if self.pilot is None:
            self.notify("No vinopilot command", severity="error")
            return False
        if self.job is not None:
            self.notify(f"{self.job.label} is still running (Esc stops it)", severity="warning")
            return False
        if self.scanning:
            self.notify("Still looking", severity="warning")
            return False
        return True

    # --- actions -----------------------------------------------------------

    def action_check(self) -> None:
        if self.ready():
            self.scan(True)

    def action_install(self) -> None:
        if not self.ready() or (c := self.selected()) is None:
            return
        if c.get("installed"):
            self.notify(f"{c['name']} is already installed; use Update", severity="warning")
            return
        channels = c.get("channels") or []
        if not channels:
            self.notify(f"{c['name']} has nothing to install", severity="warning")
            return
        if len(channels) == 1:
            self.run_job(self.pilot.install(c["id"], channels[0]))
            return
        options = [(ch, channel_label(c, ch)) for ch in channels]
        self.push_screen(
            ChooseScreen(f"Install {c['name']} from", options),
            lambda via: via and self.run_job(self.pilot.install(c["id"], via)),
        )

    def action_update(self) -> None:
        if not self.ready() or (c := self.selected()) is None:
            return
        if not c.get("installed"):
            self.notify(f"{c['name']} isn't installed; use Install", severity="warning")
            return
        self.run_job(self.pilot.update(c["id"]))

    def action_pilot(self) -> None:
        """vinoPilot itself: check, then update, then restart."""
        installed = (self.me.get("installed") or {}).get("version")
        if self.updated and installed and installed != self.version and self.job is None:
            self.restart = True
            self.exit()
            return
        if not self.ready():
            return
        update = self.me.get("update") or {}
        if update.get("state") == "available" and self.me.get("installed"):
            self.run_job(self.pilot.update(SELF))
        else:
            self.scan(True)

    def action_update_all(self) -> None:
        if self.ready():
            self.run_job(self.pilot.update_all())

    def action_uninstall(self) -> None:
        if not self.ready() or (c := self.selected()) is None:
            return
        installed = c.get("installed")
        if not installed:
            self.notify(f"{c['name']} isn't installed", severity="warning")
            return
        via = channel_label(c, installed["channel"])
        self.push_screen(
            ConfirmScreen(f"Uninstall {c['name']} ({via})?", "y uninstalls, n keeps it"),
            lambda yes: yes and self.run_job(self.pilot.uninstall(c["id"])),
        )

    def action_steps(self) -> None:
        if self.pilot is None or (c := self.selected()) is None:
            return
        options = [
            (a, f"{a} ({channel_label(c, ch)})" if (ch := self.channel_for(c, a)) else a)
            for a in ACTIONS
        ]
        self.push_screen(
            ChooseScreen(f"Show the steps for {c['name']}", options),
            lambda action: action and self.show_steps(c, action),
        )

    def channel_for(self, c: dict, action: str) -> str | None:
        """The channel `vinopilot steps` would pick: how it's installed, else the first."""
        installed = c.get("installed")
        if installed:
            return installed["channel"]
        channels = c.get("channels") or []
        return channels[0] if channels and action == "install" else None

    def show_steps(self, c: dict, action: str) -> None:
        assert self.pilot is not None
        try:
            text = self.pilot.steps(c["id"], action)
        except (CommandError, OSError) as e:
            self.log_line(f"✗ {e}")
            return
        self.push_screen(TextScreen(f"{action} {c['name']}: what would run", text))

    def action_source_root(self) -> None:
        if not self.ready():
            return
        self.push_screen(
            InputScreen("Where source checkouts go", getattr(self, "source_root", "")),
            self.set_source_root,
        )

    def set_source_root(self, path: str | None) -> None:
        if not path or self.pilot is None:
            return
        try:
            self.pilot.set_source_root(path)
        except (CommandError, OSError) as e:
            self.log_line(f"✗ {e}")
            return
        self.log_line(f"Source folder: {path}")
        self.scan(self.checked)

    def action_stop(self) -> None:
        if self.job is not None:
            self.job.stop()
            self.log_line("Stopping…")

    def action_quit(self) -> None:
        if self.job is not None:
            self.job.stop()
        self.exit()

    # --- jobs --------------------------------------------------------------

    @work(thread=True, exclusive=True, group="job")
    def run_job(self, job: Job) -> None:
        self.job = job
        self.call_from_thread(self.set_status, job.label)
        self.call_from_thread(self.log_line, f"== {job.label}")
        for event in job.events():
            if event.kind == "line":
                self.call_from_thread(self.log_line, event.text)
            else:
                self.call_from_thread(self.log_done, event)
        self.job = None
        self.call_from_thread(self.scan, self.checked)

    def log_done(self, event: Event) -> None:
        what = f" {event.app}" if event.app else ""
        if event.outcome == "done":
            self.log_line(f"✓ Done{what}")
            if event.app == SELF:
                self.updated = True
        elif event.outcome == "stopped":
            self.log_line(f"Stopped{what}")
        else:
            self.log_line(f"✗ Failed{what}: {event.reason or 'see above'}")

    def log_line(self, text: str) -> None:
        self.query_one("#log", RichLog).write(text)


def main() -> None:
    parser = argparse.ArgumentParser(prog="vinopilot-tui", description=__doc__.splitlines()[0])
    parser.add_argument("--version", action="version", version=f"vinopilot-tui {__version__}")
    parser.parse_args()
    app = PilotApp()
    app.run()
    if app.restart:
        # The update replaced this script too; start the new one in its place.
        os.execvp("vinopilot-tui", ["vinopilot-tui"])


if __name__ == "__main__":
    main()
