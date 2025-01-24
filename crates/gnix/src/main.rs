use std::process::Command;

use clap::Parser;
use color_eyre::{eyre::ContextCompat, owo_colors::OwoColorize};
use nix_installed_list::{get_meta, get_version, manifest_parsed};

#[derive(Parser, Debug)]
pub enum Cli {
    #[clap(about = "List installed packages")]
    List,

    /// list available channels from the nixpkgs
    /// repository. this command only shows "fully-fledged"
    /// distributions, so small, and darwin channels
    /// are not displayed. Unstable is assumed to always
    /// exist, so it is not printed in this list, only
    /// nixos-XX.YY channels are shown.
    ListChannels {
        /// the most recent n packages will be shown,
        /// by default, this is 5.
        n: Option<usize>,
    },

    /// install, by default, installs from the nixpkgs
    /// repository, but if the package characters don't
    /// match [0-9-_\.A-z], then the package will
    /// attempt to install from spec passed
    Install {
        package: String,
        #[clap(long)]
        unstable: bool,
    },
    /// Install a package from a nixpkgs PR.
    /// This probably won't be used crazy commonly, but
    /// exists out of convenience.
    InstallNixpkgsPr { pr: u64, package: String },
    /// uninstall a packge by name
    Uninstall { package: String },
}

fn main() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::List => {
            let parsed = manifest_parsed().unwrap();

            let mut pkgs = parsed.elements.packages.into_iter().collect::<Vec<_>>();
            pkgs.sort_by_key(|k| k.0.clone());

            for (i, (pname, package)) in pkgs.iter().enumerate() {
                let version = get_version(package);
                let meta = get_meta(package);

                let description = meta.get("description").and_then(|d| d.as_str());

                let last_arg = i == pkgs.len() - 1;
                let joiner = if last_arg { "└" } else { "├" };
                let indent = if last_arg { " " } else { "│" };

                println!(
                    "{joiner}─{} ",
                    format!(
                        "{}{}{}{}{}",
                        " ".on_bright_blue(),
                        pname.clone().bold().black().on_bright_blue(),
                        " @ ".on_bright_blue().black(),
                        match version {
                            Some(s) => format!("{}", s.bold().on_bright_blue().black()),
                            None =>
                                format!("{}", "<version unknown>".italic().on_bright_blue().black()),
                        },
                        " ".on_bright_blue()
                    ),
                );
                if let Some(description) = description {
                    println!("{indent}  ├─ {}", description.white().bold());
                }
                println!(
                    "{indent}  └─ {}#{}",
                    match package.original_url.split_once(':') {
                        Some((first, second)) => format!(
                            "{} {}",
                            format!("{}:", first).purple().bold(),
                            second.underline()
                        ),
                        None => format!("{}", package.original_url),
                    },
                    package.attr_path.italic()
                );

                if !last_arg {
                    println!("{indent}");
                }
            }
        }
        Cli::ListChannels { n } => {
            let channel_list = nix_channel_list::get_full_channels()?;
            let mut channel_list = channel_list.into_iter().collect::<Vec<_>>();
            channel_list.sort();
            channel_list.reverse();
            let n = n.unwrap_or(5);
            for channel in channel_list.iter().take(n) {
                println!("{}", channel);
            }
        }
        Cli::Install { package, unstable } => {
            let selected_channel = if unstable {
                "unstable".to_owned()
            } else {
                let channel_list = nix_channel_list::get_full_channels()?;
                let mut channel_list = channel_list.into_iter().collect::<Vec<_>>();
                channel_list.sort();

                let latest_channel = channel_list
                    .pop()
                    .context("there should be at least one valid channel")?;
                latest_channel
            };

            let augment = package
                .chars()
                .all(|c| ('A'..'z').contains(&c) || c == '_' || c == '-' || c == '.');

            let src = if augment {
                format!("nixpkgs/nixos-{selected_channel}#{package}")
            } else {
                package
            };
            install_package(src)?;
        }
        Cli::InstallNixpkgsPr { pr, package } => {
            let nixpkgs_ref = format!("github:nixos/nixpkgs/pull/{pr}/head#{package}");
            install_package(nixpkgs_ref)?;
        }
        Cli::Uninstall { package } => {
            let mut command = Command::new("nix");
            command.args(&["profile", "remove", &package]);
            let mut child = command.spawn()?;

            let waited = child.wait()?;

            std::process::exit(waited.code().unwrap_or(0))
        }
    }
    Ok(())
}

fn install_package(src: String) -> Result<std::convert::Infallible, color_eyre::eyre::Error> {
    let mut command = Command::new("nix");
    let args = ["profile", "install", &src];
    println!(
        "{}",
        format!("nix {} ", args.join(" ")).black().on_bright_blue()
    );
    command.args(args);
    let mut child = command.spawn()?;
    let waited = child.wait()?;

    // we're going to just simply bubble up whatever
    // the process exit code is.
    std::process::exit(waited.code().unwrap_or(0))
}
