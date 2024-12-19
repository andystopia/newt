use clap::Parser;
use color_eyre::owo_colors::OwoColorize;
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
        n: Option<usize>
    }
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
                        " ".on_blue(),
                        pname.clone().bold().on_blue(),
                        " @ ".on_blue(),
                        match version {
                            Some(s) => format!("{}", s.bold().on_blue()),
                            None => format!("{}", "<version unknown>".italic().on_blue()),
                        },
                        " ".on_blue()
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
        },
        Cli::ListChannels { n } => {
            let channel_list = nix_channel_list::get_full_channels()?;
            let mut channel_list = channel_list.into_iter().collect::<Vec<_>>();
            channel_list.sort();
            channel_list.reverse();
            let n = n.unwrap_or(5);
            for channel in channel_list.iter().take(n) {
                println!("{}", channel);
            }
        },

    }
    Ok(())
}
