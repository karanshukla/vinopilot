//! What vinopilot manages and the shell each action runs. Each app's own
//! installer does the work (and the hash checks); this only knows which one
//! to call, with what, for each way of installing it.

use vino_ui::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id {
    App(App),
    Ovfetch,
    Ui,
}

impl Id {
    pub const ALL: [Id; 10] = [
        Id::App(App::Pilot),
        Id::App(App::Whisper),
        Id::App(App::AuthFace),
        Id::App(App::OpenEffects),
        Id::App(App::Pop),
        Id::App(App::Speak),
        Id::App(App::Search),
        Id::App(App::Watch),
        Id::Ovfetch,
        Id::Ui,
    ];

    /// The name in JSON and on the command line: `vinowhisper`, `ovfetch`.
    pub fn slug(self) -> String {
        self.name().to_lowercase()
    }

    /// A component by its slug or name, with or without the `vino`:
    /// `vinoWhisper`, `vinowhisper` and `whisper` are all vinoWhisper.
    pub fn parse(text: &str) -> Option<Id> {
        let want = text.to_lowercase();
        let bare = |id: Id| id.slug().strip_prefix("vino").map(str::to_string);
        Id::ALL.into_iter().find(|id| {
            id.slug() == want
                || bare(*id).as_deref() == Some(want.as_str())
                || (want == "ui" && *id == Id::Ui)
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Id::App(app) => app.name(),
            Id::Ovfetch => "ovfetch",
            Id::Ui => "vinoUserInterface",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            // Its own card says what it is to the person using it.
            Id::App(App::Pilot) => "This app: installs and updates the others",
            Id::App(app) => app.blurb(),
            Id::Ovfetch => "Fetches the OpenVINO runtime an NPU needs",
            Id::Ui => "The egui library the apps are drawn with",
        }
    }

    /// `owner/name` on GitHub.
    pub fn repo(self) -> &'static str {
        match self {
            // Lowercase like its release URLs and the checkout folder
            // install.sh makes; GitHub ignores case, the folder name doesn't.
            Id::App(App::Pilot) => "karanshukla/vinopilot",
            Id::App(app) => app.repo(),
            Id::Ovfetch => "karanshukla/ovfetch",
            Id::Ui => "karanshukla/vinoUserInterface",
        }
    }

    /// The checkout's folder name under the source folder.
    pub fn checkout_name(self) -> &'static str {
        self.repo()
            .rsplit_once('/')
            .map_or(self.repo(), |(_, name)| name)
    }

    pub fn package(self) -> Option<Package> {
        match self {
            Id::App(App::Whisper) => Some(Package::PyPi("vinowhisper")),
            Id::App(App::Speak) => Some(Package::PyPi("vinospeak")),
            Id::Ovfetch => Some(Package::Crate("ovfetch")),
            Id::App(App::Pilot) => Some(Package::Crate("vinopilot")),
            Id::Ui => Some(Package::Library("vino-user-interface")),
            Id::App(_) => None,
        }
    }

    /// The app id of its Flatpak, for the components whose release has one
    /// (`<slug>-x86_64.flatpak`, as vinoPilot's own).
    pub fn flatpak(self) -> Option<&'static str> {
        match self {
            Id::App(App::Pilot) => Some(PILOT_APP_ID),
            Id::App(App::Whisper) => Some("io.github.karanshukla.vinowhisper"),
            Id::App(App::Search) => Some("io.github.karanshukla.vinosearch"),
            _ => None,
        }
    }

    /// Installs under /usr/local and /etc, so every step runs through pkexec.
    pub fn needs_root(self) -> bool {
        self == Id::App(App::AuthFace)
    }

    /// How a source install handles a checkout that isn't on its default
    /// branch.
    pub fn off_branch(self) -> OffBranch {
        match self {
            Id::App(App::Whisper) | Id::App(App::Pop) | Id::Ui => OffBranch::Switch,
            _ => OffBranch::Worktree,
        }
    }

    pub fn channels(self) -> Vec<Channel> {
        let mut all = Vec::new();
        if self != Id::Ui {
            all.push(Channel::Release);
        }
        all.push(Channel::Source);
        if matches!(self.package(), Some(Package::PyPi(_) | Package::Crate(_))) {
            all.push(Channel::Package);
        }
        // vinoPilot's own Flatpak is found, not offered: see `Channel::Deb`.
        if self.flatpak().is_some() && self != Id::App(App::Pilot) {
            all.push(Channel::Flatpak);
        }
        all
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    PyPi(&'static str),
    Crate(&'static str),
    /// On crates.io, but a library: nothing to install, apps depend on it.
    Library(&'static str),
}

impl Package {
    pub fn name(self) -> &'static str {
        match self {
            Package::PyPi(n) | Package::Crate(n) | Package::Library(n) => n,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// The GitHub release's hash-checked installer.
    Release,
    /// A git checkout in the source folder, built locally.
    Source,
    /// PyPI through `uv tool`, or crates.io through `cargo install`.
    Package,
    /// vinoPilot's own release as a `.deb`, system-wide. Like the `.rpm`,
    /// only vinoPilot is found this way, and no `channels()` offers it.
    Deb,
    /// The same as an `.rpm`.
    Rpm,
    /// The release's Flatpak bundle, for the user or system-wide: vinoPilot's
    /// own (found, not offered), and the apps that have one ([`Id::flatpak`]).
    Flatpak,
}

impl Channel {
    /// `release`, `source`, `package` (also `pypi`, `crates`) or `flatpak`;
    /// `deb` and `rpm` for vinoPilot itself.
    pub fn parse(text: &str) -> Option<Channel> {
        match text.to_lowercase().as_str() {
            "release" | "binary" => Some(Channel::Release),
            "source" | "git" => Some(Channel::Source),
            "package" | "pypi" | "crates" | "crates.io" | "cargo" | "uv" => Some(Channel::Package),
            "deb" => Some(Channel::Deb),
            "rpm" => Some(Channel::Rpm),
            "flatpak" => Some(Channel::Flatpak),
            _ => None,
        }
    }

    pub fn label(self, id: Id) -> &'static str {
        match (self, id.package()) {
            (Channel::Release, _) => "Release",
            (Channel::Source, _) => "Source",
            (Channel::Package, Some(Package::PyPi(_))) => "PyPI",
            (Channel::Package, _) => "crates.io",
            (Channel::Deb, _) => "deb package",
            (Channel::Rpm, _) => "rpm package",
            (Channel::Flatpak, _) => "Flatpak",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
    Uninstall,
}

impl Action {
    pub fn verb(self) -> &'static str {
        match self {
            Action::Install => "Installing",
            Action::Update => "Updating",
            Action::Uninstall => "Uninstalling",
        }
    }
}

/// Where a source install builds from when the checkout is on another
/// branch. Either way the checkout's own branch is never pulled; on the
/// default branch both are a plain `git pull --ff-only`. Reaches the
/// prelude's `checkout` as `$OFF_BRANCH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffBranch {
    /// A new path: the default branch in a worktree at `$WORKTREE`, removed
    /// after the install. For installs that copy everything out.
    Worktree,
    /// The checkout's own path, switched to the default branch with its
    /// changes stashed, and left there. For installs that keep running from
    /// it (venv links, launchers pointing into it), where a worktree would
    /// be a second copy the app quietly runs from.
    Switch,
}

impl OffBranch {
    pub fn name(self) -> &'static str {
        match self {
            OffBranch::Worktree => "worktree",
            OffBranch::Switch => "switch",
        }
    }
}

/// One shell script, run with `bash -c` as the user, or as root through
/// pkexec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub script: String,
    pub root: bool,
}

impl Step {
    fn user(script: &str) -> Self {
        Step {
            script: format!("{PRELUDE}\n{script}"),
            root: false,
        }
    }

    fn root(script: &str) -> Self {
        Step {
            script: format!("{PRELUDE}\n{script}"),
            root: true,
        }
    }
}

/// Every step starts with these. `$CHECKOUT`, `$WORKTREE`, `$OFF_BRANCH`,
/// `$REPO_URL` and `$RELEASE` come from the environment (see
/// [`crate::job`]), never from formatting paths into the script.
const PRELUDE: &str = r#"set -euo pipefail
BIN="$HOME/.local/bin"
# release_run ASSET [ARGS...]: the release's own one-line installer, which
# checks what it downloads against the release's SHA256SUMS.
release_run() {
    local f; f="$(mktemp)"
    curl -fsSL --retry 3 --connect-timeout 20 -o "$f" "$RELEASE/$1"
    shift
    bash "$f" "$@" </dev/null
    rm -f "$f"
}
# checkout: the source to install from. The base path is the checkout,
# cloned or fast-forwarded on its default branch. On another branch,
# $OFF_BRANCH picks the path (catalog's OffBranch): checkout_worktree
# points CHECKOUT at a new one for the rest of the step, checkout_switch
# keeps it. checkout_done, a step of its own, ends either.
checkout() {
    if [ ! -d "$CHECKOUT/.git" ]; then
        echo "== git clone $REPO_URL $CHECKOUT"
        mkdir -p "$(dirname "$CHECKOUT")"
        git clone "$REPO_URL" "$CHECKOUT"
        return
    fi
    local main branch
    main="$(default_branch)"
    branch="$(git -C "$CHECKOUT" symbolic-ref --quiet --short HEAD || true)"
    if [ "$branch" != "$main" ]; then
        branch="${branch:-$(git -C "$CHECKOUT" rev-parse --short HEAD)}"
        case "$OFF_BRANCH" in
            worktree) checkout_worktree "$main" "$branch"; return ;;
            switch) checkout_switch "$main" "$branch" ;;
            *) echo "✗ OFF_BRANCH is '$OFF_BRANCH'" >&2; return 1 ;;
        esac
    fi
    drop_worktree
    echo "== git pull in $CHECKOUT"
    git -C "$CHECKOUT" pull --ff-only
}
# checkout_worktree MAIN FROM: MAIN in $WORKTREE, which becomes CHECKOUT.
checkout_worktree() {
    echo "== $CHECKOUT is on $2, not $1: installing $1 from $WORKTREE"
    git -C "$CHECKOUT" fetch origin "$1"
    drop_worktree
    mkdir -p "$(dirname "$WORKTREE")"
    git -C "$CHECKOUT" worktree add --detach "$WORKTREE" "origin/$1"
    CHECKOUT="$WORKTREE"
}
# checkout_switch MAIN FROM: the checkout onto MAIN, stashing what FROM has
# that isn't committed. It stays on MAIN, then is pulled as on the base path.
checkout_switch() {
    echo "== $CHECKOUT is on $2, not $1: switching it to $1, the install runs from it"
    local back="git switch $2"
    git -C "$CHECKOUT" show-ref --verify --quiet "refs/heads/$2" || back="git switch --detach $2"
    if [ -n "$(git -C "$CHECKOUT" status --porcelain)" ]; then
        git -C "$CHECKOUT" stash push --include-untracked -m "vinopilot: $2 before switching to $1"
        back="$back && git stash pop"
    fi
    git -C "$CHECKOUT" switch "$1"
    echo "Back to your work: $back (in $CHECKOUT). The installed app then runs that code."
}
# The branch origin's HEAD names, "main" when git can't tell.
default_branch() {
    local ref
    ref="$(git -C "$CHECKOUT" symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null)" ||
        { git -C "$CHECKOUT" remote set-head origin --auto >/dev/null 2>&1 &&
            ref="$(git -C "$CHECKOUT" symbolic-ref --quiet --short refs/remotes/origin/HEAD)"; } ||
        ref=origin/main
    echo "${ref#origin/}"
}
# Where the step's install runs from: $WORKTREE when checkout used one.
source_dir() { if [ -e "$WORKTREE/.git" ]; then echo "$WORKTREE"; else echo "$CHECKOUT"; fi; }
drop_worktree() {
    [ -e "$WORKTREE" ] || return 0
    git -C "$CHECKOUT" worktree remove --force "$WORKTREE" 2>/dev/null || rm -rf "$WORKTREE" ||
        { echo "✗ Couldn't remove $WORKTREE" >&2; return 0; }
    git -C "$CHECKOUT" worktree prune
    echo "Removed $WORKTREE"
}
# After an install from $WORKTREE: records its commit as
# refs/vinopilot/installed, which `check` counts from, and removes it.
# After one from the checkout, drops the record.
checkout_done() {
    if [ -e "$WORKTREE/.git" ]; then
        git -C "$CHECKOUT" update-ref refs/vinopilot/installed "$(git -C "$WORKTREE" rev-parse HEAD)"
        drop_worktree
    else
        git -C "$CHECKOUT" update-ref -d refs/vinopilot/installed 2>/dev/null || true
    fi
}
need() { command -v "$1" >/dev/null || { echo "✗ $1 isn't installed: $2" >&2; exit 1; }; }
need_uv() { need uv "install it from https://docs.astral.sh/uv/, then try again"; }
need_cargo() { need cargo "install Rust from https://rustup.rs, then try again"; }
# drop_cargo_copies BIN...: what an earlier `cargo install` left in
# ~/.cargo/bin. Left there it still counts as installed, and on many PATHs
# it runs instead of the release in ~/.local/bin.
drop_cargo_copies() {
    local b
    for b in "$@"; do
        [ -e "$HOME/.cargo/bin/$b" ] || continue
        if command -v cargo >/dev/null && cargo uninstall "$b" 2>/dev/null; then
            echo "cargo uninstall $b"
        else
            rm -f "$HOME/.cargo/bin/$b"
            echo "Removed $HOME/.cargo/bin/$b"
        fi
    done
}
# follow_tui upgrade|uninstall: vinopilot-tui (PyPI's vinopilot) is a
# frontend to this command, so it updates and goes with it. Nothing when it
# isn't a uv tool.
follow_tui() {
    command -v uv >/dev/null || return 0
    [ -f "$(uv tool dir)/vinopilot/uv-receipt.toml" ] || return 0
    echo "== uv tool $1 vinopilot"
    uv tool "$1" vinopilot
}
# The checkout, moved to the trash. Fails, keeping it, when it holds work
# that's nowhere else.
trash_checkout() {
    [ -d "$CHECKOUT/.git" ] || { echo "No checkout at $CHECKOUT"; return 0; }
    drop_worktree
    local why=""
    if [ -n "$(git -C "$CHECKOUT" status --porcelain)" ]; then
        why="it has uncommitted changes"
    elif [ -n "$(git -C "$CHECKOUT" log --branches --not --remotes --oneline)" ]; then
        why="it has commits that aren't pushed"
    elif [ -n "$(git -C "$CHECKOUT" stash list)" ]; then
        why="it has stashed changes"
    fi
    [ -z "$why" ] || { echo "✗ Kept $CHECKOUT: $why" >&2; return 1; }
    if command -v gio >/dev/null && gio trash -- "$CHECKOUT"; then :
    elif command -v kioclient >/dev/null && kioclient move "$CHECKOUT" trash:/; then :
    elif command -v trash-put >/dev/null && trash-put -- "$CHECKOUT"; then :
    else
        echo "✗ Kept $CHECKOUT: no gio, kioclient or trash-put to move it to the trash with" >&2
        return 1
    fi
    echo "Moved $CHECKOUT to the trash"
}"#;

/// vinoWhisper's own uninstaller is `vinowhisper-upgrade --uninstall`, in
/// releases that have it. Without it, this undoes what its installer and
/// vinowhisper-setup write, then deletes the models and their cache once the
/// server is stopped. Transcripts, sessions and settings stay.
const WHISPER_UNINSTALL: &str = r#"if [ -x "$BIN/vinowhisper-upgrade" ] && grep -q -- '--uninstall' "$BIN/vinowhisper-upgrade"; then
    "$BIN/vinowhisper-upgrade" --uninstall </dev/null
    exit
fi
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
systemctl --user disable --now vinowhisper-server.socket vinowhisper-server.service 2>/dev/null || true
rm -f "$units/vinowhisper-server.service" "$units/vinowhisper-server.socket"
systemctl --user daemon-reload 2>/dev/null || true
if command -v uv >/dev/null && uv tool list 2>/dev/null | grep -q '^vinowhisper '; then
    uv tool uninstall vinowhisper
fi
# A checkout's install links its .venv's commands into ~/.local/bin.
for f in "$BIN"/vinowhisper-*; do
    if [ -L "$f" ]; then
        case "$(readlink "$f")" in */.venv/bin/vinowhisper-*) rm -f "$f" ;; esac
    fi
done
rm -f "$BIN/vinowhisper-tray" "$BIN/vinowhisper-gui" "$BIN/vinowhisper-upgrade"
rm -f "${XDG_DATA_HOME:-$HOME/.local/share}"/applications/io.github.karanshukla.vinowhisper*.desktop
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}"/autostart/io.github.karanshukla.vinowhisper*.desktop
rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/vinowhisper/models" "${XDG_CACHE_HOME:-$HOME/.cache}/vinowhisper"
echo "Removed vinoWhisper and its models. Your transcripts, sessions and settings are still in ~/.local/share/vinowhisper and ~/.config/vinowhisper."
"#;

const SPEAK_SOURCE: &str = r#"need_uv
cd "$CHECKOUT"
uv export --locked --no-dev --no-emit-project --no-hashes -o constraints.txt
uv tool install --force --constraints constraints.txt .
rm -f constraints.txt "$BIN/vinospeak-upgrade"
if command -v cargo >/dev/null; then gui/install.sh </dev/null; else echo "no cargo: skipped the window"; fi
"#;

const SPEAK_UNINSTALL: &str = r#"if [ -x "$BIN/vinospeak-upgrade" ]; then
    "$BIN/vinospeak-upgrade" --uninstall </dev/null
else
    if [ -x "$CHECKOUT/gui/install.sh" ]; then "$CHECKOUT/gui/install.sh" --uninstall </dev/null || true; fi
    if command -v uv >/dev/null && uv tool list 2>/dev/null | grep -q '^vinospeak '; then uv tool uninstall vinospeak; fi
    # The checkout's installer removes the models too.
    if [ -f "$CHECKOUT/install.sh" ]; then bash "$CHECKOUT/install.sh" --uninstall </dev/null; fi
fi
drop_cargo_copies vinospeak-gui
"#;

/// vinoOpenEffects' uninstaller is `vinoopeneffects-upgrade --uninstall` in
/// releases that have it. Before that it shipped only in the repo: use the
/// checkout's, or a shallow clone's.
const OPENEFFECTS_UNINSTALL: &str = r#"if [ -x "$BIN/vinoopeneffects-upgrade" ] && grep -q -- '--uninstall' "$BIN/vinoopeneffects-upgrade"; then
    bash "$BIN/vinoopeneffects-upgrade" --uninstall </dev/null
elif [ -x "$CHECKOUT/scripts/uninstall.sh" ]; then
    "$CHECKOUT/scripts/uninstall.sh"
else
    tmp="$(mktemp -d)"
    git clone --depth 1 "$REPO_URL" "$tmp/src"
    "$tmp/src/scripts/uninstall.sh"
    rm -rf "$tmp"
fi
drop_cargo_copies vinoopeneffects vinoopeneffects-ctl vinoopeneffects-daemon
"#;

const OVFETCH_RELEASE: &str = r#"need sha256sum "install coreutils"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
for f in ovfetch-x86_64-linux SHA256SUMS; do
    curl -fsSL --retry 3 --connect-timeout 20 -o "$tmp/$f" "$RELEASE/$f"
done
(cd "$tmp" && awk '$2 == "ovfetch-x86_64-linux" || $2 == "*ovfetch-x86_64-linux"' SHA256SUMS > want \
    && [ "$(wc -l < want)" -eq 1 ] && sha256sum -c --strict --quiet want) \
    || { echo "✗ ovfetch-x86_64-linux doesn't match SHA256SUMS. Nothing was installed." >&2; exit 1; }
echo "checksum verified"
install -Dm755 "$tmp/ovfetch-x86_64-linux" "$BIN/ovfetch"
"$BIN/ovfetch" --version
"#;

/// vinoPilot's Flatpak, and the menu entry's name.
pub const PILOT_APP_ID: &str = "io.github.karanshukla.vinopilot";

/// Before a package step: `fetch ASSET` downloads that file of the release
/// into `$tmp` and checks it against the release's SHA256SUMS, as install.sh
/// does the tarball, or against its own `ASSET.sha256` in a release without
/// one (vinoSearch's). `as_root` is pkexec for the one command that installs
/// a system package; the download and the check stay the user's.
/// `flatpak_scope` is where `$APP_ID` is installed, and the user's
/// installation for a first install: no password, like the other ways in.
const PACKAGE: &str = r#"tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fetch() {
    local code=0
    curl -fsSL --retry 3 --connect-timeout 20 -o "$tmp/$1" "$RELEASE/$1" || code=$?
    [ "$code" -ne 22 ] || { echo "✗ The latest release has no $1" >&2; exit 1; }
    [ "$code" -eq 0 ] || { echo "✗ Couldn't download $1" >&2; exit 1; }
    curl -fsSL --retry 3 --connect-timeout 20 -o "$tmp/SHA256SUMS" "$RELEASE/SHA256SUMS" 2>/dev/null \
        || curl -fsSL --retry 3 --connect-timeout 20 -o "$tmp/SHA256SUMS" "$RELEASE/$1.sha256" \
        || { echo "✗ The release has no checksum for $1. Nothing was installed." >&2; exit 1; }
    (cd "$tmp" && awk -v f="$1" '$2 == f || $2 == "*" f { print $1 "  " f }' SHA256SUMS > want \
        && [ "$(wc -l < want)" -eq 1 ] && sha256sum -c --strict --quiet want) \
        || { echo "✗ $1 doesn't match its checksum. Nothing was installed." >&2; exit 1; }
    echo "checksum verified"
    # The package manager may read it as another user.
    chmod 755 "$tmp"; chmod 644 "$tmp/$1"
}
as_root() {
    if [ "$(id -u)" -eq 0 ]; then "$@"; return; fi
    need pkexec "it asks for your password; or run this as root: $*"
    echo "== $* (asks for your password)"
    pkexec "$@"
}
flatpak_scope() {
    if flatpak info --user "$APP_ID" >/dev/null 2>&1; then echo --user
    elif flatpak info --system "$APP_ID" >/dev/null 2>&1; then echo --system
    else echo --user
    fi
}
"#;

const PILOT_DEB: &str = r#"fetch vinopilot-x86_64.deb
as_root apt-get install -y "$tmp/vinopilot-x86_64.deb""#;

const PILOT_RPM: &str = r#"[ ! -e /run/ostree-booted ] || { echo "✗ This system layers packages with rpm-ostree, which vinoPilot doesn't drive: replace the vinopilot package with the new release's .rpm, or use the Flatpak" >&2; exit 1; }
fetch vinopilot-x86_64.rpm
if command -v dnf >/dev/null; then as_root dnf install -y "$tmp/vinopilot-x86_64.rpm"
elif command -v zypper >/dev/null; then as_root zypper --non-interactive install --allow-unsigned-rpm "$tmp/vinopilot-x86_64.rpm"
else as_root rpm -U "$tmp/vinopilot-x86_64.rpm"
fi"#;

const PILOT_RPM_UNINSTALL: &str = r#"if command -v dnf >/dev/null; then as_root dnf remove -y vinopilot
elif command -v zypper >/dev/null; then as_root zypper --non-interactive remove vinopilot
else as_root rpm -e vinopilot
fi"#;

/// A bundle with a newer commit installs over the old one; the same commit
/// is an error to Flatpak, `--reinstall` or not (1.14, 2026-11-09), and
/// nothing to do here. A running window keeps its files until it closes:
/// Flatpak removes a deployment that's in use afterwards.
///
/// `-y`, not `--noninteractive`: the latter also stops polkit from asking
/// for a password, and a system-wide bundle install always needs one
/// (`install-bundle` is `auth_admin`, and Flatpak's rule for the admin group
/// doesn't cover it), so it failed with "not allowed for user".
const FLATPAK_INSTALL: &str = r#"need flatpak "install it from https://flatpak.org/setup, then try again"
fetch "$ASSET"
if out="$(LC_ALL=C flatpak install "$(flatpak_scope)" -y "$tmp/$ASSET" 2>&1)"; then
    echo "$out"
else
    echo "$out"
    case "$out" in
        *"already installed"*) echo "This release's Flatpak is the one installed" ;;
        *) exit 1 ;;
    esac
fi"#;

/// Leaves `~/.var/app/$APP_ID` alone: what the app keeps there is the
/// user's to delete.
const FLATPAK_UNINSTALL: &str = r#"need flatpak "it's what this was installed with"
flatpak uninstall "$(flatpak_scope)" -y "$APP_ID""#;

/// A Flatpak step: the package prelude, with `$APP_ID` and `$ASSET` (the
/// bundle's name in the release) set for `body`.
fn flatpak_step(id: Id, app_id: &str, body: &str) -> Step {
    Step::user(&format!(
        "{PACKAGE}APP_ID={app_id}\nASSET={}-x86_64.flatpak\n{body}",
        id.slug()
    ))
}

/// What uninstalling `id` installed through `channel` removes and what it
/// keeps, in a sentence or two for the confirmation. Each app's own
/// uninstaller deletes its downloaded models and keeps the user's data.
pub fn uninstall_note(id: Id, channel: Channel) -> String {
    if let (Channel::Flatpak, Some(app_id)) = (channel, id.flatpak())
        && id != Id::App(App::Pilot)
    {
        return format!(
            "Removes the app. Its downloaded models, your data and settings stay in ~/.var/app/{app_id}."
        );
    }
    let what = match id {
        Id::App(App::Pilot) => "Removes this app. The apps it installed stay.",
        Id::App(App::Watch) => "Removes the app.",
        Id::App(_) => "Removes the app and its downloaded models. Your own data and settings stay.",
        Id::Ovfetch => "Removes ovfetch. The runtime it fetched stays.",
        Id::Ui => return "Moves the checkout to the trash.".into(),
    };
    if channel == Channel::Source {
        format!("{what} Moves the checkout to the trash.")
    } else {
        what.into()
    }
}

/// The steps for `action` on `id` installed (or to be installed) through
/// `channel`. Empty when that combination doesn't exist. Updating or
/// uninstalling vinoPilot takes the terminal UI along. Uninstalling a
/// source install ends by moving its checkout to the trash; installing or
/// updating one ends with `checkout_done`.
pub fn steps(id: Id, channel: Channel, action: Action) -> Vec<Step> {
    let mut steps = app_steps(id, channel, action);
    if id == Id::App(App::Pilot) && !steps.is_empty() {
        match action {
            Action::Install => {}
            Action::Update => steps.push(Step::user("follow_tui upgrade")),
            Action::Uninstall => steps.push(Step::user("follow_tui uninstall")),
        }
    }
    if id != Id::Ui
        && channel == Channel::Source
        && action == Action::Uninstall
        && !steps.is_empty()
    {
        steps.push(Step::user("trash_checkout"));
    }
    if channel == Channel::Source && action != Action::Uninstall && !steps.is_empty() {
        steps.push(Step::user("checkout_done"));
    }
    steps
}

fn app_steps(id: Id, channel: Channel, action: Action) -> Vec<Step> {
    use Action::*;
    use Channel::*;
    let s = Step::user;
    let r = Step::root;
    // A release's Flatpak bundle: checked, then installed over the old one.
    // Before the apps' own arms, whose `_` uninstalls are for the home.
    if let (Flatpak, Some(app_id)) = (channel, id.flatpak()) {
        let body = match action {
            Install | Update => FLATPAK_INSTALL,
            Uninstall => FLATPAK_UNINSTALL,
        };
        return vec![flatpak_step(id, app_id, body)];
    }
    match (id, channel, action) {
        // vinoWhisper: install.sh runs vinowhisper-setup on a first install.
        (Id::App(App::Whisper), Release, Install) => vec![s("release_run install.sh --gui --yes")],
        (Id::App(App::Whisper), Release, Update) => vec![s(
            r#"if [ -x "$BIN/vinowhisper-upgrade" ]; then "$BIN/vinowhisper-upgrade" </dev/null; else release_run install.sh; fi"#,
        )],
        (Id::App(App::Whisper), Source, Install | Update) => {
            vec![s(
                r#"checkout; "$CHECKOUT/scripts/install.sh" --yes --gui </dev/null"#,
            )]
        }
        (Id::App(App::Whisper), Package, Install) => vec![s(
            r#"need_uv; uv tool install --force vinowhisper; rm -f "$BIN/vinowhisper-upgrade"; "$BIN/vinowhisper-setup" --gui --yes </dev/null"#,
        )],
        (Id::App(App::Whisper), Package, Update) => vec![s(
            "need_uv; uv tool upgrade vinowhisper; systemctl --user try-restart vinowhisper-server.service 2>/dev/null || true",
        )],
        (Id::App(App::Whisper), _, Uninstall) => vec![s(WHISPER_UNINSTALL)],

        // vinoSpeak
        (Id::App(App::Speak), Release, Install) => vec![s("release_run install.sh --gui")],
        (Id::App(App::Speak), Release, Update) => vec![s(
            r#"if [ -x "$BIN/vinospeak-upgrade" ]; then "$BIN/vinospeak-upgrade" </dev/null; else release_run install.sh; fi"#,
        )],
        (Id::App(App::Speak), Source, Install | Update) => {
            vec![s(&format!("checkout\n{SPEAK_SOURCE}"))]
        }
        (Id::App(App::Speak), Package, Install) => {
            vec![s(
                r#"need_uv; uv tool install --force --python 3.14 vinospeak; rm -f "$BIN/vinospeak-upgrade""#,
            )]
        }
        (Id::App(App::Speak), Package, Update) => vec![s("need_uv; uv tool upgrade vinospeak")],
        (Id::App(App::Speak), _, Uninstall) => vec![s(SPEAK_UNINSTALL)],

        // vinoPop: get.sh unpacks the release and runs its install.sh.
        (Id::App(App::Pop), Release, Install) => vec![s("release_run get.sh")],
        (Id::App(App::Pop), Release, Update) => vec![s(
            r#"if [ -x "$BIN/vinopop-upgrade" ]; then "$BIN/vinopop-upgrade" </dev/null; else release_run get.sh; fi"#,
        )],
        (Id::App(App::Pop), Release, Uninstall) => vec![s(
            r#"if [ -x "$BIN/vinopop-upgrade" ]; then "$BIN/vinopop-upgrade" --uninstall </dev/null; else release_run get.sh --uninstall; fi"#,
        )],
        (Id::App(App::Pop), Source, Install | Update) => {
            vec![s(r#"checkout; bash "$CHECKOUT/install.sh" </dev/null"#)]
        }
        (Id::App(App::Pop), Source, Uninstall) => {
            vec![s(r#"bash "$CHECKOUT/install.sh" --uninstall </dev/null"#)]
        }

        // vinoSearch: the installer passes --uninstall through to the
        // release's scripts/install.sh.
        (Id::App(App::Search), Release, Install) => vec![s("release_run install.sh")],
        (Id::App(App::Search), Release, Update) => vec![s(
            r#"if [ -x "$BIN/vinosearch-upgrade" ]; then "$BIN/vinosearch-upgrade" </dev/null; else release_run install.sh; fi"#,
        )],
        (Id::App(App::Search), Release, Uninstall) => vec![s(
            "release_run install.sh --uninstall; drop_cargo_copies vinosearch vinosearch-settings",
        )],
        (Id::App(App::Search), Source, Install | Update) => {
            vec![s(
                r#"checkout; need_cargo; "$CHECKOUT/scripts/install.sh" </dev/null"#,
            )]
        }
        (Id::App(App::Search), Source, Uninstall) => {
            vec![s(
                r#""$CHECKOUT/scripts/install.sh" --uninstall </dev/null; drop_cargo_copies vinosearch vinosearch-settings"#,
            )]
        }

        // vinoWatch: vinowatch-upgrade is a copy of install.sh.
        (Id::App(App::Watch), Release, Install) => vec![s("release_run install.sh")],
        (Id::App(App::Watch), Release, Update) => vec![s(
            r#"if [ -x "$BIN/vinowatch-upgrade" ]; then sh "$BIN/vinowatch-upgrade" </dev/null; else release_run install.sh; fi"#,
        )],
        (Id::App(App::Watch), Release, Uninstall) => {
            vec![s(
                "release_run install.sh --uninstall; drop_cargo_copies vinowatch",
            )]
        }
        (Id::App(App::Watch), Source, Install | Update) => {
            vec![s(
                r#"checkout; need_cargo; sh "$CHECKOUT/install.sh" </dev/null"#,
            )]
        }
        (Id::App(App::Watch), Source, Uninstall) => {
            vec![s(
                r#"sh "$CHECKOUT/install.sh" --uninstall </dev/null; drop_cargo_copies vinowatch"#,
            )]
        }

        // vinoOpenEffects
        (Id::App(App::OpenEffects), Release, Install) => vec![s("release_run install.sh")],
        (Id::App(App::OpenEffects), Release, Update) => vec![s(
            r#"if [ -x "$BIN/vinoopeneffects-upgrade" ]; then bash "$BIN/vinoopeneffects-upgrade" </dev/null; else release_run install.sh; fi"#,
        )],
        (Id::App(App::OpenEffects), Source, Install | Update) => {
            vec![s(
                r#"checkout; need_cargo; "$CHECKOUT/scripts/install.sh" </dev/null"#,
            )]
        }
        (Id::App(App::OpenEffects), _, Uninstall) => vec![s(OPENEFFECTS_UNINSTALL)],

        // vinoAuthFace: root throughout; the checkout is pulled as the user
        // so it stays the user's.
        (Id::App(App::AuthFace), Release, Install) => vec![r("release_run install.sh")],
        (Id::App(App::AuthFace), Release, Update) => vec![r(
            r#"if [ -x /usr/local/bin/vinoauthface-upgrade ]; then /usr/local/bin/vinoauthface-upgrade </dev/null; else release_run install.sh; fi"#,
        )],
        (Id::App(App::AuthFace), Source, Install | Update) => vec![
            s("checkout"),
            r(r#"cd "$(source_dir)"; bash ./deploy.sh </dev/null"#),
        ],
        (Id::App(App::AuthFace), _, Uninstall) => vec![r(
            r#"if [ -x /usr/local/bin/vinoauthface-uninstall ]; then /usr/local/bin/vinoauthface-uninstall; else bash "$CHECKOUT/uninstall.sh"; fi"#,
        )],

        // ovfetch: a bare binary in the release, a bin crate on crates.io.
        // Each install drops the other channel's copy, which would otherwise
        // shadow it or be what detection finds.
        (Id::Ovfetch, Release, Install | Update) => {
            vec![s(&format!("{OVFETCH_RELEASE}drop_cargo_copies ovfetch\n"))]
        }
        (Id::Ovfetch, Release, Uninstall) => {
            vec![s(
                r#"rm -f "$BIN/ovfetch"; echo "Removed $BIN/ovfetch"; drop_cargo_copies ovfetch"#,
            )]
        }
        (Id::Ovfetch, Source, Install | Update) => {
            vec![s(
                r#"checkout; need_cargo; cargo install --locked --force --path "$CHECKOUT"; rm -f "$BIN/ovfetch""#,
            )]
        }
        (Id::Ovfetch, Package, Install | Update) => {
            vec![s(
                r#"need_cargo; cargo install --locked --force ovfetch; rm -f "$BIN/ovfetch""#,
            )]
        }
        (Id::Ovfetch, Source | Package, Uninstall) => {
            vec![s(r#"drop_cargo_copies ovfetch; rm -f "$BIN/ovfetch""#)]
        }

        // vinoPilot: its install.sh builds from a checkout when run from one.
        (Id::App(App::Pilot), Release, Install | Update) => vec![s(
            r#"if [ -x "$BIN/vinopilot-upgrade" ]; then bash "$BIN/vinopilot-upgrade" </dev/null; else release_run install.sh; fi
drop_cargo_copies vinopilot vinopilot-gui"#,
        )],
        (Id::App(App::Pilot), Release, Uninstall) => vec![s(
            "release_run install.sh --uninstall; drop_cargo_copies vinopilot vinopilot-gui",
        )],
        (Id::App(App::Pilot), Source, Install | Update) => {
            vec![s(
                r#"checkout; need_cargo; bash "$CHECKOUT/install.sh" </dev/null; drop_cargo_copies vinopilot vinopilot-gui"#,
            )]
        }
        (Id::App(App::Pilot), Source, Uninstall) => {
            vec![s(
                r#"bash "$CHECKOUT/install.sh" --uninstall </dev/null; drop_cargo_copies vinopilot vinopilot-gui"#,
            )]
        }
        (Id::App(App::Pilot), Package, Install | Update) => {
            vec![s(
                r#"need_cargo; cargo install --locked --force vinopilot vinopilot-gui
rm -f "$BIN/vinopilot" "$BIN/vinopilot-gui" "$BIN/vinopilot-upgrade"
"$HOME/.cargo/bin/vinopilot-gui" --install-desktop"#,
            )]
        }
        (Id::App(App::Pilot), Package, Uninstall) => vec![s(
            "need_cargo; vinopilot-gui --uninstall-desktop || true; for c in vinopilot vinopilot-gui; do cargo uninstall \"$c\" || true; done",
        )],

        // vinoPilot's packages: the release's file for that package manager,
        // checked, then installed over the old one.
        (Id::App(App::Pilot), Deb, Install | Update) => {
            vec![s(&format!("{PACKAGE}{PILOT_DEB}"))]
        }
        (Id::App(App::Pilot), Deb, Uninstall) => {
            vec![s(&format!("{PACKAGE}as_root apt-get remove -y vinopilot"))]
        }
        (Id::App(App::Pilot), Rpm, Install | Update) => {
            vec![s(&format!("{PACKAGE}{PILOT_RPM}"))]
        }
        (Id::App(App::Pilot), Rpm, Uninstall) => {
            vec![s(&format!("{PACKAGE}{PILOT_RPM_UNINSTALL}"))]
        }

        // vinoUserInterface: a library, so "installed" means a checkout that builds.
        (Id::Ui, Source, Install | Update) => vec![s(
            r#"checkout; need_cargo; cd "$CHECKOUT"; cargo build --locked --example gallery; echo "Run it: cargo run --example gallery (in $CHECKOUT)""#,
        )],
        (Id::Ui, Source, Uninstall) => vec![s("trash_checkout")],

        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIONS: [Action; 3] = [Action::Install, Action::Update, Action::Uninstall];

    #[test]
    fn every_offered_channel_can_install_update_and_uninstall() {
        for id in Id::ALL {
            for channel in id.channels() {
                for action in ACTIONS {
                    assert!(
                        !steps(id, channel, action).is_empty(),
                        "{} {:?} {:?}",
                        id.name(),
                        channel,
                        action
                    );
                }
            }
        }
    }

    #[test]
    fn only_authface_asks_for_root() {
        for id in Id::ALL {
            for channel in id.channels() {
                for action in ACTIONS {
                    let root = steps(id, channel, action).iter().any(|s| s.root);
                    assert_eq!(
                        root,
                        id.needs_root(),
                        "{} {channel:?} {action:?}",
                        id.name()
                    );
                }
            }
        }
    }

    #[test]
    fn uninstalling_from_source_trashes_the_checkout_last() {
        let ends_trashing = |id: Id, channel| {
            steps(id, channel, Action::Uninstall)
                .last()
                .is_some_and(|s| !s.root && s.script.ends_with("trash_checkout"))
        };
        for id in Id::ALL {
            assert!(ends_trashing(id, Channel::Source), "{}", id.name());
            assert!(!ends_trashing(id, Channel::Release), "{}", id.name());
            assert!(!ends_trashing(id, Channel::Package), "{}", id.name());
        }
    }

    #[test]
    fn the_terminal_ui_follows_vinopilots_updates_and_uninstall() {
        let follows = |id, channel, action| {
            steps(id, channel, action)
                .iter()
                .any(|s| s.script.contains("\nfollow_tui "))
        };
        for id in Id::ALL {
            for channel in id.channels() {
                let pilot = id == Id::App(App::Pilot);
                assert!(!follows(id, channel, Action::Install), "{}", id.name());
                assert_eq!(follows(id, channel, Action::Update), pilot, "{}", id.name());
                assert_eq!(
                    follows(id, channel, Action::Uninstall),
                    pilot,
                    "{}",
                    id.name()
                );
            }
        }
    }

    #[test]
    fn installing_ovfetch_or_vinopilot_drops_the_other_channels_copy() {
        let script = |id, channel, action| -> String {
            steps(id, channel, action)
                .iter()
                .map(|s| s.script.as_str())
                .collect()
        };
        for (id, bin) in [(Id::Ovfetch, "ovfetch"), (Id::App(App::Pilot), "vinopilot")] {
            for action in [Action::Install, Action::Update] {
                let release = script(id, Channel::Release, action);
                assert!(
                    release.contains(&format!("drop_cargo_copies {bin}")),
                    "{bin} release {action:?}"
                );
                let package = script(id, Channel::Package, action);
                assert!(
                    package.contains(&format!("rm -f \"$BIN/{bin}\"")),
                    "{bin} package {action:?}"
                );
            }
        }
        assert!(
            script(Id::Ovfetch, Channel::Source, Action::Install)
                .contains(r#"rm -f "$BIN/ovfetch""#)
        );
        assert!(
            script(Id::App(App::Pilot), Channel::Source, Action::Install)
                .contains("drop_cargo_copies vinopilot")
        );
    }

    /// Every channel a component can be found installed through.
    fn channels(id: Id) -> Vec<Channel> {
        let mut all = id.channels();
        if id == Id::App(App::Pilot) {
            all.extend([Channel::Deb, Channel::Rpm, Channel::Flatpak]);
        }
        all
    }

    #[test]
    fn vinopilots_packages_update_from_the_release_checked_and_ask_for_root_themselves() {
        for (channel, asset) in [
            (Channel::Deb, "vinopilot-x86_64.deb"),
            (Channel::Rpm, "vinopilot-x86_64.rpm"),
            (Channel::Flatpak, "vinopilot-x86_64.flatpak"),
        ] {
            assert!(!Id::App(App::Pilot).channels().contains(&channel));
            for action in ACTIONS {
                let steps = steps(Id::App(App::Pilot), channel, action);
                assert!(!steps.is_empty(), "{channel:?} {action:?}");
                // Only the package manager runs as root, not the whole step.
                assert!(steps.iter().all(|s| !s.root), "{channel:?} {action:?}");
            }
            let update = &steps(Id::App(App::Pilot), channel, Action::Update)[0].script;
            assert!(
                update.contains(&format!("fetch {asset}"))
                    || update.contains(&format!("ASSET={asset}\nneed flatpak")),
                "{channel:?}"
            );
            assert!(update.contains("sha256sum -c"), "{channel:?}");
            // No other component has them.
            assert!(steps(Id::Ovfetch, channel, Action::Update).is_empty());
        }
    }

    #[test]
    fn vinowhisper_and_vinosearch_also_install_from_their_flatpak() {
        for id in Id::ALL {
            let offered = id.channels().contains(&Channel::Flatpak);
            let has = matches!(id, Id::App(App::Whisper | App::Search));
            assert_eq!(offered, has, "{}", id.name());
            // The default way in stays the first.
            assert_ne!(id.channels()[0], Channel::Flatpak);
            if !has {
                continue;
            }
            let app_id = id.flatpak().unwrap();
            assert_eq!(app_id, format!("io.github.karanshukla.{}", id.slug()));
            for action in ACTIONS {
                let steps = steps(id, Channel::Flatpak, action);
                assert_eq!(steps.len(), 1, "{} {action:?}", id.name());
                let script = &steps[0].script;
                assert!(script.contains(&format!("APP_ID={app_id}\n")));
                // Never the home's uninstaller: that isn't how it got here.
                assert!(!script.contains("-upgrade"), "{} {action:?}", id.name());
                assert_eq!(
                    script.contains("flatpak uninstall"),
                    action == Action::Uninstall
                );
            }
            let note = uninstall_note(id, Channel::Flatpak);
            assert!(note.contains(&format!("~/.var/app/{app_id}")), "{note}");
        }
    }

    /// Runs `id`'s Flatpak install against a `file://` release holding
    /// `sums` as the checksum file `sums_name`, with `flatpak` stubbed to
    /// write its arguments down. What it was run with, or the step's stderr.
    fn install_flatpak(id: Id, sums_name: &str, good: bool) -> Result<String, String> {
        use std::fs;
        let asset = format!("{}-x86_64.flatpak", id.slug());
        let root = std::env::temp_dir().join(format!(
            "vinopilot-flatpak-step-{}-{sums_name}-{good}-{}",
            id.slug(),
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let (release, stubs, home) = (root.join("release"), root.join("stubs"), root.join("home"));
        for dir in [&release, &stubs, &home] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(release.join(&asset), "a bundle").unwrap();
        let sum = std::process::Command::new("sha256sum")
            .arg(release.join(&asset))
            .output()
            .unwrap();
        let hash = String::from_utf8_lossy(&sum.stdout)[..64].to_string();
        let hash = if good { hash } else { "0".repeat(64) };
        fs::write(release.join(sums_name), format!("{hash}  {asset}\n")).unwrap();
        // `info` fails: installed nowhere yet.
        let stub = stubs.join("flatpak");
        fs::write(
            &stub,
            "#!/bin/sh\n[ \"$1\" != info ] || exit 1\necho \"$@\" >> \"$HOME/ran\"\n",
        )
        .unwrap();
        make_executable(&stub);
        let step = &steps(id, Channel::Flatpak, Action::Install)[0];
        let out = std::process::Command::new("bash")
            .args(["-c", &step.script])
            .env_clear()
            .env("HOME", &home)
            .env("RELEASE", format!("file://{}", release.display()))
            .env("PATH", format!("{}:/usr/bin:/bin", stubs.display()))
            .output()
            .unwrap();
        let ran = fs::read_to_string(home.join("ran")).unwrap_or_default();
        fs::remove_dir_all(root).unwrap();
        if out.status.success() {
            Ok(ran)
        } else {
            assert_eq!(ran, "", "flatpak ran after a failed check");
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    #[test]
    fn a_flatpak_is_checked_then_installed_for_the_user() {
        let has = |tool: &str| {
            std::process::Command::new(tool)
                .arg("--version")
                .output()
                .is_ok()
        };
        if !["bash", "curl", "sha256sum"].into_iter().all(has) {
            return;
        }
        // vinoWhisper's release has SHA256SUMS, vinoSearch's a .sha256 per file.
        for (id, sums) in [
            (Id::App(App::Whisper), "SHA256SUMS".to_string()),
            (
                Id::App(App::Search),
                "vinosearch-x86_64.flatpak.sha256".to_string(),
            ),
        ] {
            let ran = install_flatpak(id, &sums, true).unwrap();
            let asset = format!("{}-x86_64.flatpak", id.slug());
            assert!(ran.starts_with("install --user -y "), "{ran}");
            assert!(ran.trim_end().ends_with(&asset), "{ran}");
            let why = install_flatpak(id, &sums, false).unwrap_err();
            assert!(why.contains("doesn't match its checksum"), "{why}");
        }
    }

    #[test]
    fn a_release_without_the_flatpak_says_so() {
        let has = |tool: &str| {
            std::process::Command::new(tool)
                .arg("--version")
                .output()
                .is_ok()
        };
        if !["bash", "curl"].into_iter().all(has) {
            return;
        }
        let step = &steps(Id::App(App::Search), Channel::Flatpak, Action::Install)[0];
        let out = std::process::Command::new("bash")
            .args(["-c", &step.script])
            .env("RELEASE", "file:///nowhere/at/all")
            .output()
            .unwrap();
        assert!(!out.status.success());
        let why = String::from_utf8_lossy(&out.stderr);
        assert!(why.contains("✗ "), "{why}");
    }

    #[test]
    fn a_library_has_no_release_or_package_install() {
        assert_eq!(Id::Ui.channels(), vec![Channel::Source]);
    }

    #[test]
    fn components_parse_by_slug_name_or_bare_name() {
        for id in Id::ALL {
            assert_eq!(Id::parse(&id.slug()), Some(id));
            assert_eq!(Id::parse(id.name()), Some(id));
        }
        assert_eq!(Id::parse("whisper"), Some(Id::App(App::Whisper)));
        assert_eq!(Id::parse("ui"), Some(Id::Ui));
        assert_eq!(Id::parse("nope"), None);
    }

    #[test]
    fn checkout_names_match_the_repos() {
        assert_eq!(Id::App(App::Search).checkout_name(), "vinoSearch");
        assert_eq!(Id::Ui.checkout_name(), "vinoUserInterface");
    }

    #[test]
    fn the_uninstall_note_names_the_models_only_for_apps_that_have_them() {
        for id in Id::ALL {
            for channel in id.channels() {
                let note = uninstall_note(id, channel);
                let models = matches!(id, Id::App(app) if app != App::Pilot && app != App::Watch);
                assert_eq!(note.contains("models"), models, "{} {channel:?}", id.name());
                assert_eq!(
                    note.contains("trash"),
                    channel == Channel::Source,
                    "{} {channel:?}",
                    id.name()
                );
                assert_eq!(
                    note.matches("trash").count(),
                    usize::from(channel == Channel::Source)
                );
            }
        }
    }

    /// Runs vinoWhisper's uninstall step in a home of its own, with
    /// `systemctl` and `uv` stubbed out so nothing reaches the real session.
    fn uninstall_whisper(name: &str, setup: impl FnOnce(&std::path::Path)) -> std::path::PathBuf {
        use std::fs;
        let root =
            std::env::temp_dir().join(format!("vinopilot-whisper-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let stubs = root.join("stubs");
        fs::create_dir_all(&stubs).unwrap();
        for stub in ["systemctl", "uv"] {
            let path = stubs.join(stub);
            fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            make_executable(&path);
        }
        let data = root.join("data/vinowhisper");
        let cache = root.join("cache/vinowhisper");
        for dir in [
            data.join("models/whisper-small.en"),
            data.join("transcripts"),
            data.join("sessions"),
            cache.join("export-tmp"),
            home.join(".config/vinowhisper"),
            home.join(".local/bin"),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(data.join("models/whisper-small.en/model.xml"), "x").unwrap();
        fs::write(data.join("transcripts/2026-11-11-120000.txt"), "x").unwrap();
        setup(&home);
        let step = &steps(Id::App(App::Whisper), Channel::Release, Action::Uninstall)[0];
        let out = std::process::Command::new("bash")
            .args(["-c", &step.script])
            .env_clear()
            .env("HOME", &home)
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("PATH", format!("{}:/usr/bin:/bin", stubs.display()))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        root
    }

    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn uninstalling_vinowhisper_deletes_its_models_and_keeps_transcripts_and_settings() {
        if std::process::Command::new("bash")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let root = uninstall_whisper("inline", |_| {});
        assert!(!root.join("data/vinowhisper/models").exists());
        assert!(!root.join("cache/vinowhisper").exists());
        assert!(
            root.join("data/vinowhisper/transcripts/2026-11-11-120000.txt")
                .exists()
        );
        assert!(root.join("data/vinowhisper/sessions").exists());
        assert!(root.join("home/.config/vinowhisper").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn vinowhispers_own_uninstaller_runs_when_it_has_one() {
        if std::process::Command::new("bash")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        // An upgrade script that knows --uninstall is run with it, and the
        // inline steps aren't.
        let root = uninstall_whisper("own", |home| {
            let upgrade = home.join(".local/bin/vinowhisper-upgrade");
            std::fs::write(
                &upgrade,
                "#!/bin/sh\n# --uninstall: remove it\necho \"$@\" > \"$HOME/ran\"\n",
            )
            .unwrap();
            make_executable(&upgrade);
        });
        let ran = std::fs::read_to_string(root.join("home/ran")).unwrap();
        assert_eq!(ran.trim(), "--uninstall");
        assert!(root.join("data/vinowhisper/models").exists());
        std::fs::remove_dir_all(&root).unwrap();

        // An older one that doesn't is left alone, and the inline steps run.
        let root = uninstall_whisper("old", |home| {
            let upgrade = home.join(".local/bin/vinowhisper-upgrade");
            std::fs::write(&upgrade, "#!/bin/sh\necho \"$@\" > \"$HOME/ran\"\n").unwrap();
            make_executable(&upgrade);
        });
        assert!(!root.join("home/ran").exists());
        assert!(!root.join("home/.local/bin/vinowhisper-upgrade").exists());
        assert!(!root.join("data/vinowhisper/models").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_script_parses() {
        let Ok(bash) = std::process::Command::new("bash").arg("--version").output() else {
            return;
        };
        assert!(bash.status.success());
        for id in Id::ALL {
            for channel in channels(id) {
                for action in ACTIONS {
                    for step in steps(id, channel, action) {
                        let out = std::process::Command::new("bash")
                            .args(["-n", "-c", &step.script])
                            .output()
                            .unwrap();
                        assert!(
                            out.status.success(),
                            "{} {channel:?} {action:?}: {}",
                            id.name(),
                            String::from_utf8_lossy(&out.stderr)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn installs_that_run_from_the_checkout_switch_it() {
        let switch: Vec<_> = Id::ALL
            .into_iter()
            .filter(|id| id.off_branch() == OffBranch::Switch)
            .collect();
        assert_eq!(switch, [Id::App(App::Whisper), Id::App(App::Pop), Id::Ui]);
    }

    /// The prelude's checkout on a real repo: a checkout on a feature branch
    /// is left alone, the default branch is built in the worktree, and
    /// checkout_done records it and removes the worktree unless kept.
    #[test]
    fn a_checkout_off_the_default_branch_installs_from_a_worktree() {
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("vinopilot-worktree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let run_as = |off_branch: OffBranch, script: &str| {
            let out = std::process::Command::new("bash")
                .args(["-c", &format!("{PRELUDE}\n{script}")])
                .current_dir(&tmp)
                .env("CHECKOUT", tmp.join("checkout"))
                .env("WORKTREE", tmp.join("share/worktrees/checkout"))
                .env("OFF_BRANCH", off_branch.name())
                .env("REPO_URL", tmp.join("origin"))
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            assert!(
                out.status.success(),
                "{script}\n{stdout}{}",
                String::from_utf8_lossy(&out.stderr)
            );
            stdout
        };
        let run = |script: &str| run_as(OffBranch::Worktree, script);
        run(
            r#"git init -q -b main origin && git -C origin commit -q --allow-empty -m one
checkout
git -C "$CHECKOUT" switch -q -c feature && echo work > "$CHECKOUT/wip"
git -C origin commit -q --allow-empty -m two"#,
        );

        let out =
            run(r#"checkout; echo "building in $CHECKOUT"; git -C "$CHECKOUT" log -1 --format=%s"#);
        assert!(out.contains("is on feature, not main"), "{out}");
        assert!(out.contains("share/worktrees/checkout\n"), "{out}");
        assert!(out.ends_with("two\n"), "{out}");
        assert_eq!(
            run(r#"source_dir; git -C "$CHECKOUT" branch --show-current; cat "$CHECKOUT/wip""#),
            format!(
                "{}\nfeature\nwork\n",
                tmp.join("share/worktrees/checkout").display()
            )
        );
        run("checkout_done");
        assert!(!tmp.join("share/worktrees/checkout").exists());
        assert_eq!(
            run(r#"git -C "$CHECKOUT" log -1 --format=%s refs/vinopilot/installed"#),
            "two\n"
        );

        // Switch: the checkout itself goes to main, its work stashed, and
        // the record goes.
        let out = run_as(OffBranch::Switch, "checkout; checkout_done");
        assert!(
            out.contains("Back to your work: git switch feature && git stash pop"),
            "{out}"
        );
        let out = run(
            r#"git -C "$CHECKOUT" branch --show-current; git -C "$CHECKOUT" log -1 --format=%s
git -C "$CHECKOUT" stash list --format=%s; [ -e "$CHECKOUT/wip" ] || echo stashed
git -C "$CHECKOUT" rev-parse -q --verify refs/vinopilot/installed || echo gone"#,
        );
        assert_eq!(
            out,
            "main\ntwo\nOn feature: vinopilot: feature before switching to main\nstashed\ngone\n"
        );
        assert!(!tmp.join("share/worktrees/checkout").exists());
        // On main already, either is a plain pull.
        let out = run_as(OffBranch::Switch, "checkout");
        assert!(out.contains("== git pull in"), "{out}");
        assert!(!out.contains("switching"), "{out}");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
