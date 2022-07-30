use std::str;

use anyhow::{bail, format_err, Result};

use crate::{cargo, context::Context, version::Version};

pub(crate) struct Rustup {
    pub(crate) version: u32,
}

impl Rustup {
    pub(crate) fn new() -> Self {
        // If failed to determine rustup version, assign 0 to skip all version-dependent decisions.
        let version = minor_version()
            .map_err(|e| warn!("unable to determine rustup version: {:#}", e))
            .unwrap_or(0);

        Self { version }
    }
}

pub(crate) fn version_range(range: &str, step: Option<&str>, cx: &Context) -> Result<Vec<String>> {
    let check = |version: &Version| {
        if version.major != 1 {
            bail!("major version must be 1");
        }
        if let Some(patch) = version.patch {
            warn!(
                "--version-range always selects the latest patch release per minor release, \
                     not the specified patch release `{}`",
                patch
            );
        }
        Ok(())
    };

    let mut split = range.splitn(2, "..");
    let start = match split.next().unwrap() {
        "" => {
            let mut rust_version = None;
            for id in cx.workspace_members() {
                let v = cx.rust_version(id);
                if v.is_none() || v == rust_version {
                    // no-op
                } else if rust_version.is_none() {
                    rust_version = v;
                } else {
                    bail!("automatic detection of the lower bound of the version range is not yet supported when the minimum supported Rust version of the crates in the workspace do not match")
                }
            }
            match rust_version {
                Some(v) => v.parse()?,
                None => bail!("no rust-version field in Cargo.toml is specified"),
            }
        }
        s => s.parse()?,
    };
    check(&start)?;

    let end = match split.next() {
        Some("") | None => {
            install_toolchain("stable", None, false)?;
            cargo::minor_version(cmd!("rustup", "run", "stable", "cargo"))?
        }
        Some(end) => {
            let end = end.parse()?;
            check(&end)?;
            end.minor
        }
    };

    let step = step.map(str::parse::<u8>).transpose()?.unwrap_or(1);
    if step == 0 {
        bail!("--version-step cannot be zero");
    }

    let versions: Vec<_> =
        (start.minor..=end).step_by(step as _).map(|minor| format!("1.{}", minor)).collect();
    if versions.is_empty() {
        bail!("specified version range `{}` is empty", range);
    }
    Ok(versions)
}

pub(crate) fn install_toolchain(
    mut toolchain: &str,
    target: Option<&str>,
    print_output: bool,
) -> Result<()> {
    if toolchain.starts_with('+') {
        toolchain = &toolchain[1..];
    }

    if target.is_none()
        && cmd!("rustup", "run", toolchain, "cargo", "--version").run_with_output().is_ok()
    {
        // Do not run `rustup toolchain install` if the toolchain already has installed.
        return Ok(());
    }

    // In Github Actions and Azure Pipelines, --no-self-update is necessary
    // because the windows environment cannot self-update rustup.exe.
    let mut cmd = cmd!("rustup", "toolchain", "add", toolchain, "--no-self-update");
    if let Some(target) = target {
        cmd.args(&["--target", target]);
    }

    if print_output {
        // The toolchain installation can take some time, so we'll show users
        // the progress.
        cmd.run()
    } else {
        // However, in certain situations, it may be preferable not to display it.
        cmd.run_with_output().map(drop)
    }
}

fn minor_version() -> Result<u32> {
    let mut cmd = cmd!("rustup", "--version");
    let output = cmd.read()?;

    let version = (|| {
        let mut output = output.split(' ');
        if output.next()? != "rustup" {
            return None;
        }
        output.next()
    })()
    .ok_or_else(|| format_err!("unexpected output from {}: {}", cmd, output))?;
    let version: Version = version.parse()?;
    if version.major != 1 || version.patch.is_none() {
        bail!("unexpected output from {}: {}", cmd, output);
    }

    Ok(version.minor)
}
