use std::{process::Command, str::FromStr};

use clap::Parser;
use color_eyre::{
    eyre::{bail, ContextCompat},
    owo_colors::OwoColorize,
};
use nix_elastic_search::{MatchSearch, NixElasticSearch, Query};
use nix_elastic_search_ureq::UreqNixSearcher;
use nix_installed_list::{
    get_meta, get_version, manifest_parsed, CachePackage, CachePackageLookup,
    CachePackageLookupKey, CachePackages,
};

pub trait InstalledPackagePrinter {
    fn print_cache_package(&self, package: &CachePackage);
}

pub struct PrettyInstalledPackagePrinter {
    is_last: bool,
}

impl PrettyInstalledPackagePrinter {
    pub fn new() -> Self {
        Self { is_last: false }
    }

    pub fn print_header(&self, package_count: usize) {
        println!(
            "{}\n│",
            format!(" {} installed packages. ", package_count)
                .white()
                .bold()
                .on_blue()
        );
    }
}

impl InstalledPackagePrinter for PrettyInstalledPackagePrinter {
    fn print_cache_package(&self, package: &CachePackage) {
        let last_arg = self.is_last;
        let joiner = if last_arg { "└" } else { "├" };
        let indent = if last_arg { " " } else { "│" };

        println!(
            "{joiner}─{} ",
            format!(
                "{}{}{}{}{}",
                " ".on_bright_blue(),
                package.name.clone().bold().black().on_bright_blue(),
                " @ ".on_bright_blue().black(),
                match &package.version {
                    Some(s) => format!("{}", s.bold().on_bright_blue().black()),
                    None => format!("{}", "<version unknown>".italic().on_bright_blue().black()),
                },
                " ".on_bright_blue()
            ),
        );
        if let Some(description) = &package.meta.get("description").and_then(|d| d.as_str()) {
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
    Search {
        package: String,
        #[clap(long)]
        unstable: bool,
    },
}

/// the idea of this function is that
/// while not all links / repos have
/// an obvious mapping to a fetcher,
/// some definitely do, and it makes
/// sense to map those to those respective
/// fetchers for the sake of a friendly
/// UX.
fn package_prefix_map(input: &str) -> String {
    if input.starts_with("https://github.com/") {
        let input = input.trim_start_matches("https://github.com/");
        format!("github:{}", input)
    } else {
        input.to_string()
    }
}

fn search_package(name: &str, unstable: bool) -> color_eyre::Result<()> {
    let searcher = NixElasticSearch::new();
    let ureq_searcher = UreqNixSearcher::new(searcher);

    let packages = if unstable {
        ureq_searcher.channel(
            "unstable",
            Query {
                max_results: 10,
                search: Some(MatchSearch {
                    search: name.to_owned(),
                }),
                ..Default::default()
            },
        )??
    } else {
        let channel_list = nix_channel_list::get_full_channels()?;
        let mut channel_list = channel_list.into_iter().collect::<Vec<_>>();
        channel_list.sort();
        channel_list.reverse();
        channel_list.truncate(2);
        let Ok([latest, lagging]): Result<[_; 2], _> = channel_list.try_into() else {
            bail!("Only one valid channel was found");
        };

        let search_channel = if ureq_searcher.channel_is_searchable(&latest)?? {
            latest
        } else {
            println!(
                "{}: Latest channel (`{}`) was not searchable. Falling back to `{}`",
                "[WARNING]".yellow(),
                latest,
                lagging
            );
            lagging
        };

        ureq_searcher.channel(
            &search_channel,
            Query {
                max_results: 10,
                search: Some(MatchSearch {
                    search: name.to_owned(),
                }),
                ..Default::default()
            },
        )??
    };

    for package in packages {
        if package.package_pversion.trim().is_empty() {
            println!("{}", package.package_attr_name.white().bold(),);
        } else {
            println!(
                "{} @ {}",
                package.package_attr_name.white().bold(),
                package.package_pversion
            );
        }
        if let Some(desc) = &package.package_description {
            println!(" ↳ {}", desc);
        }
    }

    Ok(())
}
fn main() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::List => {
            let parsed = manifest_parsed()?;

            let mut pkgs = parsed.elements.packages.into_iter().collect::<Vec<_>>();

            pkgs.sort_by(|(name_1, _), (name_2, _)| name_1.cmp(name_2));

            let mut pprinter = PrettyInstalledPackagePrinter::new();
            let mut cache_package_store = CachePackages::load()?;

            let lookup = CachePackageLookup::from_cache_packages(&cache_package_store)?;

            pprinter.print_header(pkgs.len());

            for (i, (pname, package)) in pkgs.iter().enumerate() {
                let pkg = match lookup.lookup(&CachePackageLookupKey::from_package(package)) {
                    Some(v) => v.clone(),
                    None => {
                        let version = get_version(package);
                        let meta = get_meta(package);

                        let toml_meta =
                            toml_edit::DocumentMut::from_str(&toml_edit::ser::to_string(&meta)?)?
                                .as_table()
                                .clone();

                        let pkg = CachePackage {
                            name: pname.clone(),
                            version,
                            meta: toml_meta,
                            store_paths: package.store_paths.clone(),
                            url: package.url.clone(),
                            original_url: package.original_url.clone(),
                            attr_path: package.attr_path.clone(),
                        };
                        cache_package_store.push(pkg.clone());
                        pkg
                    }
                };

                pprinter.is_last = i == pkgs.len() - 1;
                pprinter.print_cache_package(&pkg);
            }

            cache_package_store.write()?;
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
            let package = package_prefix_map(&package);
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
                .all(|c| ('A'..='z').contains(&c) || c == '_' || c == '-' || c == '.');

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
        Cli::Search { package, unstable } => search_package(&package, unstable)?,
    }
    Ok(())
}

pub fn style_src(src: &str) -> String {
    let Some((left, right)) = src.rsplit_once("#") else {
        return format!("{}", src.blue().bold());
    };

    format!("{}#{}", left.blue().bold(), right.white().bold())
}
fn install_package(src: String) -> Result<std::convert::Infallible, color_eyre::eyre::Error> {
    let mut command = Command::new("nix");
    let args = ["profile", "install", &src];
    println!(
        "{} {} profile install {}",
        ">".white().bold(),
        "nix".blue().bold(),
        style_src(&src)
    );
    command.args(args);
    let mut child = command.spawn()?;
    let waited = child.wait()?;

    // we're going to just simply bubble up whatever
    // the process exit code is.
    std::process::exit(waited.code().unwrap_or(0))
}
