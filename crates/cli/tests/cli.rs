//! The CLI's contract with scripts and other frontends: exit codes and the
//! `--json` shape. Change these only on purpose, with a changelog entry.

use std::process::{Command, Output};

fn vinopilot(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vinopilot"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .output()
        .unwrap()
}

fn empty_home(name: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("vinopilot-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

#[test]
fn list_json_names_every_component_and_its_ways_in() {
    let home = empty_home("list");
    let out = vinopilot(&home, &["list", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["vinopilot"], env!("CARGO_PKG_VERSION"));
    assert_eq!(doc["source_root"], home.join("src").to_str().unwrap());
    let components = doc["components"].as_array().unwrap();
    let ids: Vec<&str> = components
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    for id in [
        "vinopilot",
        "vinowhisper",
        "vinoauthface",
        "vinoopeneffects",
        "vinopop",
        "vinospeak",
        "vinosearch",
        "vinowatch",
        "ovfetch",
        "vinouserinterface",
    ] {
        assert!(ids.contains(&id), "{id} missing from {ids:?}");
    }
    for c in components {
        for key in [
            "name",
            "blurb",
            "repo",
            "channels",
            "registry",
            "needs_root",
            "installed",
            "checkout",
            "system_package",
            "shadowed",
            "update",
        ] {
            assert!(c.get(key).is_some(), "{} has no {key}", c["id"]);
        }
        assert!(c.get("latest").is_none(), "list must not go online");
        if c["id"] != "vinoauthface" {
            assert!(
                c["installed"].is_null(),
                "{} found in an empty home",
                c["id"]
            );
        }
    }
    let whisper = components
        .iter()
        .find(|c| c["id"] == "vinowhisper")
        .unwrap();
    assert_eq!(
        whisper["channels"],
        serde_json::json!(["release", "source", "package", "flatpak"])
    );
    let search = components.iter().find(|c| c["id"] == "vinosearch").unwrap();
    assert_eq!(
        search["channels"],
        serde_json::json!(["release", "source", "flatpak"])
    );
    assert_eq!(whisper["registry"], "pypi");
    let ovfetch = components.iter().find(|c| c["id"] == "ovfetch").unwrap();
    assert_eq!(ovfetch["registry"], "crates.io");
    let pop = components.iter().find(|c| c["id"] == "vinopop").unwrap();
    assert!(pop["registry"].is_null());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn steps_prints_the_script_without_running_it() {
    let home = empty_home("steps");
    let out = vinopilot(&home, &["steps", "watch", "install", "--via", "release"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("release_run install.sh"), "{text}");
    assert!(
        text.contains("RELEASE=https://github.com/karanshukla/vinoWatch/releases/latest/download")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn usage_errors_exit_2() {
    let home = empty_home("usage");
    for args in [
        &["install", "nope"][..],
        &["install"],
        &["install", "pop", "--via", "pypi"],
        &["install", "pop", "--via", "flatpak"],
        &["frobnicate"],
        &["--bogus"],
    ] {
        let out = vinopilot(&home, args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn uninstalling_what_isnt_there_fails_without_running_anything() {
    let home = empty_home("uninstall");
    let out = vinopilot(&home, &["uninstall", "watch", "--yes"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("isn't installed"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn version() {
    let home = empty_home("version");
    let out = vinopilot(&home, &["--version"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("vinopilot {}", env!("CARGO_PKG_VERSION"))
    );
    std::fs::remove_dir_all(home).unwrap();
}
