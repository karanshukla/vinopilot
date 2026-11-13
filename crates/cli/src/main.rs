//! `vinopilot`: the vinoPilot apps from a terminal. With `--json` it is also
//! the API other frontends drive: `list`/`check` print one JSON document,
//! and the actions print one JSON event per line.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use vinopilot_core::catalog::{self, Action, Channel, Id};
use vinopilot_core::job::{Env, Job, Outcome};
use vinopilot_core::status::{self, Update};
use vinopilot_core::{Config, detect, remote};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
vinopilot: install, update and uninstall the vinoPilot apps

Usage: vinopilot [--source-root DIR] [--json] <command>

Commands:
  list                       what's installed, and how (offline)
  check                      list, plus the newest version of each (online)
  install <app> [--via WAY]  WAY: release (default), source, pypi, crates or
                             flatpak (vinowhisper, vinosearch)
  update <app>... | --all    update in the way each is installed
  uninstall <app> [--yes]
  steps <app> <install|update|uninstall> [--via WAY]
                             print the shell an action runs, without running it
  config [source_root [DIR]] show or set where source checkouts go

Apps: vinowhisper vinoauthface vinoopeneffects vinopop vinospeak vinosearch
      vinowatch ovfetch vinouserinterface vinopilot (the 'vino' is optional)

--json: list and check print one JSON document; install, update and
uninstall print one JSON object per line: {\"event\":\"line\",\"text\":...}
for output, then {\"event\":\"done\",...}.
";

struct Args {
    json: bool,
    yes: bool,
    all: bool,
    via: Option<Channel>,
    source_root: Option<PathBuf>,
    rest: Vec<String>,
}

fn parse_args(raw: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        json: false,
        yes: false,
        all: false,
        via: None,
        source_root: None,
        rest: Vec::new(),
    };
    let mut raw = raw.peekable();
    while let Some(arg) = raw.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| raw.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--json" => args.json = true,
            "--yes" | "-y" => args.yes = true,
            "--all" => args.all = true,
            "--via" => {
                let v = value("--via")?;
                args.via = Some(Channel::parse(&v).ok_or_else(|| {
                    format!("--via {v}: use release, source, pypi, crates or flatpak")
                })?);
            }
            "--source-root" => args.source_root = Some(PathBuf::from(value("--source-root")?)),
            "-h" | "--help" => args.rest.insert(0, "help".into()),
            "-V" | "--version" => args.rest.insert(0, "version".into()),
            f if f.starts_with('-') => return Err(format!("unknown option {f} (see --help)")),
            _ => args.rest.push(arg),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("vinopilot: {e}");
            return ExitCode::from(2);
        }
    };
    let mut config = Config::load();
    if let Some(root) = &args.source_root {
        config.source_root = root.clone();
    }
    let command = args.rest.first().map(String::as_str).unwrap_or("help");
    let operands = &args.rest[args.rest.len().min(1)..];
    let result = match command {
        "help" => {
            print!("{HELP}");
            Ok(())
        }
        "version" => {
            println!("vinopilot {VERSION}");
            Ok(())
        }
        "list" => list(&config, &args, false),
        "check" => list(&config, &args, true),
        "install" => one(operands).and_then(|id| install(&config, &args, id)),
        "update" => update(&config, &args, operands),
        "uninstall" => one(operands).and_then(|id| uninstall(&config, &args, id)),
        "steps" => steps(&config, &args, operands),
        "config" => config_cmd(&mut config, &args, operands),
        other => Err(Failure::Usage(format!(
            "unknown command {other} (see --help)"
        ))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(e)) => {
            eprintln!("vinopilot: {e}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(e)) => {
            if !e.is_empty() {
                eprintln!("✗ {e}");
            }
            ExitCode::FAILURE
        }
    }
}

enum Failure {
    Usage(String),
    Failed(String),
}

fn app(name: &str) -> Result<Id, Failure> {
    Id::parse(name).ok_or_else(|| Failure::Usage(format!("no app called {name} (see --help)")))
}

fn one(operands: &[String]) -> Result<Id, Failure> {
    match operands {
        [name] => app(name),
        [] => Err(Failure::Usage("name an app (see --help)".into())),
        _ => Err(Failure::Usage("one app at a time".into())),
    }
}

fn components(config: &Config, online: bool) -> Vec<status::Component> {
    let paths = config.paths();
    let mut local = detect::scan(&paths);
    let mut latest = if online {
        remote::check(&paths)
    } else {
        Default::default()
    };
    Id::ALL
        .into_iter()
        .map(|id| {
            status::component(
                id,
                local.remove(&id).unwrap_or_default(),
                latest.remove(&id),
            )
        })
        .collect()
}

fn list(config: &Config, args: &Args, online: bool) -> Result<(), Failure> {
    let all = components(config, online);
    if args.json {
        let doc = serde_json::json!({
            "vinopilot": VERSION,
            "source_root": config.source_root,
            "components": all,
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
        return Ok(());
    }
    println!(
        "{:<18} {:<12} {:<10} {}",
        "APP",
        "VERSION",
        "VIA",
        if online { "UPDATE" } else { "" }
    );
    for c in &all {
        let (version, via) = match &c.local.installed {
            Some(i) => (
                i.version.clone().unwrap_or_else(|| "?".into()),
                i.channel.label(Id::parse(&c.id).unwrap()).to_string(),
            ),
            None => ("-".into(), "-".into()),
        };
        let update = match (&c.update, online, c.local.installed.is_some()) {
            (_, false, _) | (_, true, false) => String::new(),
            (Update::Available { to }, ..) => format!("{to} available"),
            (Update::UpToDate, ..) => "up to date".into(),
            (Update::Unknown { reason }, ..) => reason.clone().unwrap_or_else(|| "?".into()),
        };
        println!("{:<18} {:<12} {:<10} {}", c.name, version, via, update);
    }
    for c in &all {
        // vinoPilot's own package is how it's installed, in the row above.
        let packaged = matches!(
            c.local.installed.as_ref().map(|i| i.channel),
            Some(Channel::Deb | Channel::Rpm)
        );
        if let Some(system) = &c.local.system_package
            && !packaged
        {
            println!(
                "\n{} is also a distro package, which vinopilot doesn't remove: {}",
                c.name, system.remove
            );
        }
        for s in &c.local.shadowed {
            println!(
                "\n{} is installed twice: {} runs, {} doesn't; uninstall and reinstall it to keep one",
                c.name,
                s.runs.display(),
                s.hidden.display()
            );
        }
    }
    Ok(())
}

/// Runs an action, printing its output as it comes.
fn run(
    config: &Config,
    args: &Args,
    id: Id,
    channel: Channel,
    action: Action,
) -> Result<(), Failure> {
    let steps = catalog::steps(id, channel, action);
    if steps.is_empty() {
        return Err(Failure::Usage(format!(
            "{} can't be {} from {}",
            id.name(),
            match action {
                Action::Install => "installed",
                Action::Update => "updated",
                Action::Uninstall => "uninstalled",
            },
            channel.label(id)
        )));
    }
    let json = args.json;
    if !json {
        println!("== {} {} ({})", action.verb(), id.name(), channel.label(id));
    }
    let notify: vinopilot_core::job::Notify = Arc::new(move |line| {
        let Some(line) = line else { return };
        let mut out = std::io::stdout().lock();
        if json {
            let _ = writeln!(
                out,
                "{}",
                serde_json::json!({"event": "line", "text": line})
            );
        } else {
            let _ = writeln!(out, "{line}");
        }
    });
    let env = Env::for_id(id, &config.paths());
    let outcome = Job::start(id, channel, action, steps, env, notify).wait();
    if json {
        println!(
            "{}",
            serde_json::json!({"event": "done", "app": id.slug(), "channel": channel, "result": outcome})
        );
    }
    match outcome {
        Outcome::Done => Ok(()),
        Outcome::Failed(why) => Err(Failure::Failed(if json { String::new() } else { why })),
        Outcome::Stopped | Outcome::Running => Err(Failure::Failed("stopped".into())),
    }
}

fn installed_channel(config: &Config, id: Id) -> Option<Channel> {
    detect::scan(&config.paths())
        .remove(&id)
        .and_then(|l| l.installed)
        .map(|i| i.channel)
}

fn install(config: &Config, args: &Args, id: Id) -> Result<(), Failure> {
    let channels = id.channels();
    let channel = args.via.unwrap_or(channels[0]);
    if !channels.contains(&channel) {
        let ways: Vec<_> = channels.iter().map(|c| c.label(id)).collect();
        return Err(Failure::Usage(format!(
            "{} installs from {}",
            id.name(),
            ways.join(", ")
        )));
    }
    run(config, args, id, channel, Action::Install)
}

fn update(config: &Config, args: &Args, operands: &[String]) -> Result<(), Failure> {
    let ids: Vec<Id> = if args.all {
        if !operands.is_empty() {
            return Err(Failure::Usage("--all or app names, not both".into()));
        }
        let todo: Vec<Id> = components(config, true)
            .into_iter()
            .filter(|c| matches!(c.update, Update::Available { .. }))
            .filter_map(|c| Id::parse(&c.id))
            .collect();
        if todo.is_empty() && !args.json {
            println!("Everything installed is up to date.");
        }
        todo
    } else if operands.is_empty() {
        return Err(Failure::Usage("name an app, or --all (see --help)".into()));
    } else {
        operands.iter().map(|n| app(n)).collect::<Result<_, _>>()?
    };
    let mut failed = Vec::new();
    for id in ids {
        let Some(channel) = installed_channel(config, id) else {
            eprintln!("✗ {} isn't installed", id.name());
            failed.push(id.name());
            continue;
        };
        if run(config, args, id, channel, Action::Update).is_err() {
            failed.push(id.name());
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(Failure::Failed(format!(
            "didn't update: {}",
            failed.join(", ")
        )))
    }
}

fn uninstall(config: &Config, args: &Args, id: Id) -> Result<(), Failure> {
    let local = detect::scan(&config.paths())
        .remove(&id)
        .unwrap_or_default();
    let system = local.system_package.map(|p| p.remove);
    let channel = match (args.via, local.installed.map(|i| i.channel), &system) {
        (Some(via), ..) => via,
        (None, Some(found), _) => found,
        (None, None, Some(remove)) => {
            return Err(Failure::Failed(format!(
                "{} is only installed by your package manager: {remove}",
                id.name()
            )));
        }
        (None, None, None) => {
            return Err(Failure::Failed(format!("{} isn't installed", id.name())));
        }
    };
    if !args.yes {
        if !std::io::stdin().is_terminal() {
            return Err(Failure::Usage(
                "pass --yes to uninstall without a terminal".into(),
            ));
        }
        eprintln!("{}", catalog::uninstall_note(id, channel));
        eprint!("Uninstall {} ({})? [y/N] ", id.name(), channel.label(id));
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            return Err(Failure::Failed("left it installed".into()));
        }
    }
    run(config, args, id, channel, Action::Uninstall)?;
    // Uninstalling vinoPilot's own package just removed it.
    let packaged = matches!(channel, Channel::Deb | Channel::Rpm);
    if let Some(remove) = system
        && !args.json
        && !packaged
    {
        println!(
            "{} is still a distro package. Remove it with: {remove}",
            id.name()
        );
    }
    Ok(())
}

fn steps(config: &Config, args: &Args, operands: &[String]) -> Result<(), Failure> {
    let [name, action] = operands else {
        return Err(Failure::Usage(
            "steps <app> <install|update|uninstall>".into(),
        ));
    };
    let id = app(name)?;
    let action = match action.as_str() {
        "install" => Action::Install,
        "update" => Action::Update,
        "uninstall" => Action::Uninstall,
        other => return Err(Failure::Usage(format!("no action called {other}"))),
    };
    let channel = args
        .via
        .or_else(|| installed_channel(config, id))
        .unwrap_or(id.channels()[0]);
    let env = Env::for_id(id, &config.paths());
    println!(
        "# {} {} via {}",
        action.verb(),
        id.name(),
        channel.label(id)
    );
    println!(
        "# CHECKOUT={} REPO_URL={} RELEASE={}",
        env.checkout.display(),
        env.repo_url,
        env.release
    );
    for step in catalog::steps(id, channel, action) {
        println!(
            "# --- as {}",
            if step.root { "root (pkexec)" } else { "you" }
        );
        println!("{}", step.script);
    }
    Ok(())
}

fn config_cmd(config: &mut Config, args: &Args, operands: &[String]) -> Result<(), Failure> {
    match operands {
        [] => {
            if args.json {
                println!("{}", serde_json::json!({"source_root": config.source_root}));
            } else {
                println!("source_root = {}", config.source_root.display());
            }
            Ok(())
        }
        [key] if key == "source_root" => {
            println!("{}", config.source_root.display());
            Ok(())
        }
        [key, value] if key == "source_root" => {
            config.source_root =
                vinopilot_core::config::expand(value, &vinopilot_core::config::home());
            config
                .save()
                .map_err(|e| Failure::Failed(format!("couldn't save the config: {e}")))?;
            println!("source_root = {}", config.source_root.display());
            Ok(())
        }
        _ => Err(Failure::Usage("config [source_root [DIR]]".into())),
    }
}
