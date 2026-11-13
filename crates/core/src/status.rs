//! What a row says: installed or not, and whether there's something newer
//! for the way it's installed. The CLI and the window both read this.

use serde::Serialize;

use crate::catalog::{Channel, Id, Package};
use crate::detect::{Installed, Local};
use crate::remote::Latest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Update {
    /// Not checked yet, or the check failed.
    Unknown {
        reason: Option<String>,
    },
    UpToDate,
    /// A newer version for the way it's installed: a tag, a version, or
    /// `N commits`.
    Available {
        to: String,
    },
}

/// One component, as `vinopilot list --json` prints it.
#[derive(Debug, Clone, Serialize)]
pub struct Component {
    pub id: String,
    pub name: &'static str,
    pub blurb: &'static str,
    pub repo: &'static str,
    pub channels: Vec<Channel>,
    /// `"pypi"` or `"crates.io"` when there's a `package` channel, so a
    /// frontend can name the registry without knowing the catalog.
    pub registry: Option<&'static str>,
    pub needs_root: bool,
    #[serde(flatten)]
    pub local: Local,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<Latest>,
    pub update: Update,
}

pub fn update(installed: Option<&Installed>, latest: Option<&Latest>) -> Update {
    let (Some(installed), Some(latest)) = (installed, latest) else {
        return Update::Unknown { reason: None };
    };
    let version = installed.version.as_deref().unwrap_or("");
    let newer = |found: &Option<Result<String, String>>| match found {
        Some(Ok(newest)) if vino_ui::update::is_newer(version, newest) => {
            Update::Available { to: newest.clone() }
        }
        Some(Ok(_)) if vino_ui::update::version_parts(version).is_none() => Update::Unknown {
            reason: Some(format!(
                "{} isn't a release version",
                if version.is_empty() { "?" } else { version }
            )),
        },
        Some(Ok(_)) => Update::UpToDate,
        Some(Err(why)) => Update::Unknown {
            reason: Some(why.clone()),
        },
        None => Update::Unknown { reason: None },
    };
    match installed.channel {
        // A package or a Flatpak is a file of the release.
        Channel::Release | Channel::Deb | Channel::Rpm | Channel::Flatpak => newer(&latest.release),
        Channel::Package => newer(&latest.package),
        Channel::Source => match &latest.behind {
            Some(Ok(0)) => Update::UpToDate,
            Some(Ok(1)) => Update::Available {
                to: "1 commit".into(),
            },
            Some(Ok(n)) => Update::Available {
                to: format!("{n} commits"),
            },
            Some(Err(why)) => Update::Unknown {
                reason: Some(why.clone()),
            },
            None => Update::Unknown {
                reason: Some("no checkout to compare".into()),
            },
        },
    }
}

pub fn component(id: Id, local: Local, latest: Option<Latest>) -> Component {
    let update = update(local.installed.as_ref(), latest.as_ref());
    Component {
        id: id.slug(),
        name: id.name(),
        blurb: id.blurb(),
        repo: id.repo(),
        channels: id.channels(),
        registry: id.package().map(|p| match p {
            Package::PyPi(_) => "pypi",
            Package::Crate(_) | Package::Library(_) => "crates.io",
        }),
        needs_root: id.needs_root(),
        local,
        latest,
        update,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(channel: Channel, version: &str) -> Installed {
        Installed {
            channel,
            version: Some(version.into()),
        }
    }

    #[test]
    fn releases_compare_tags_with_versions() {
        let latest = Latest {
            release: Some(Ok("v8".into())),
            ..Default::default()
        };
        assert_eq!(
            update(Some(&at(Channel::Release, "7.0.0")), Some(&latest)),
            Update::Available { to: "v8".into() }
        );
        assert_eq!(
            update(Some(&at(Channel::Release, "8.0.0")), Some(&latest)),
            Update::UpToDate
        );
    }

    #[test]
    fn packages_compare_with_the_registry_not_the_release() {
        let latest = Latest {
            release: Some(Ok("v2.0.0".into())),
            package: Some(Ok("1.3.1".into())),
            behind: None,
        };
        assert_eq!(
            update(Some(&at(Channel::Package, "1.3.1")), Some(&latest)),
            Update::UpToDate
        );
    }

    #[test]
    fn a_dev_build_is_never_offered_a_release() {
        let latest = Latest {
            release: Some(Ok("v12".into())),
            ..Default::default()
        };
        assert!(matches!(
            update(Some(&at(Channel::Release, "dev")), Some(&latest)),
            Update::Unknown { .. }
        ));
    }

    #[test]
    fn checkouts_count_commits_behind() {
        let latest = Latest {
            behind: Some(Ok(3)),
            ..Default::default()
        };
        assert_eq!(
            update(Some(&at(Channel::Source, "dev")), Some(&latest)),
            Update::Available {
                to: "3 commits".into()
            }
        );
    }

    #[test]
    fn nothing_checked_is_unknown() {
        assert_eq!(
            update(Some(&at(Channel::Release, "1.0.0")), None),
            Update::Unknown { reason: None }
        );
        assert_eq!(
            update(None, Some(&Latest::default())),
            Update::Unknown { reason: None }
        );
    }
}
