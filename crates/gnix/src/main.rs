use std::{collections::HashMap, path::PathBuf, process::Command, str::FromStr};

use clap::Parser;
use color_eyre::{
    eyre::{bail, ContextCompat},
    owo_colors::OwoColorize,
};
use nix_installed_list::{get_meta, get_version, manifest_parsed};
use toml_edit::{Array, ArrayOfTables};

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
}

fn setup_cache_dir() -> color_eyre::Result<PathBuf> {
    let Some(home_dir) = std::env::var_os("HOME") else {
        bail!("$HOME env var not set");
    };

    let home_dir_path = PathBuf::from(home_dir);
    let cache_dir_path = home_dir_path.join(".cache/");
    let gnix_cache_dir = cache_dir_path.join("gnix/");

    if !cache_dir_path.exists() {
        std::fs::create_dir(&cache_dir_path)?;
    }

    if !gnix_cache_dir.exists() {
        std::fs::create_dir(&gnix_cache_dir)?;
    }

    Ok(gnix_cache_dir)
}

#[derive(Clone, Debug)]
pub struct CachePackage {
    name: String,
    version: Option<String>,
    meta: toml_edit::Table,
    store_paths: Vec<String>,
    url: String,
    original_url: String,
    attr_path: String,
}

#[derive(Clone, Debug)]
pub struct CachePackages {
    pub packages: Vec<CachePackage>,
}

impl CachePackages {
    pub fn new() -> Self {
        Self {
            packages: Vec::new(),
        }
    }

    pub fn load() -> color_eyre::Result<Self> {
        let cache_dir = setup_cache_dir()?;

        let file = cache_dir.join("profile-installed.toml");

        if !file.exists() {
            return Ok(Self::new());
        }

        let mut de_packages = Vec::new();

        let file_contents = std::fs::read_to_string(file)?;

        let toml_doc = toml_edit::DocumentMut::from_str(&file_contents)?;

        let Some(packages) = toml_doc.get("package") else {
            return Ok(Self::new());
        };

        let Some(packages) = packages.as_array_of_tables() else {
            return Ok(Self::new());
        };

        for package in packages {
            let name = package
                .get("name")
                .context("key `name` must exist on every package in the cache.")?
                .as_str()
                .context("key `name` was expected to be a string but it was not")?;
            let version = package
                .get("version")
                .map(|v| {
                    v.as_str()
                        .context("key `version` was expected to be a string but it was not")
                })
                .transpose()?;
            let meta = package
                .get("meta")
                .context("key `meta` must exist on every package in the cache.")?
                .as_table()
                .context("key `meta` was expected to be a table but it was not")?;
            let store_paths = package
                .get("store_paths")
                .context("key `store_paths` must exist on every package in the cache.")?
                .as_array()
                .context("key `store_paths` was expected to be an array but it was not")?
                .into_iter()
                .map(|v|
                    v.as_str()
                    .context("every entry in store paths array must be a string, but at least one was not")
                    .map(ToOwned::to_owned)
                ).collect::<color_eyre::Result<Vec<String>>>()?;

            let url = package
                .get("url")
                .context("key `url` must exist on every package in the cache.")?
                .as_str()
                .context("key `url` was expected to be a string but it was not")?;
            let original_url = package
                .get("original_url")
                .context("key `original_url` must exist on every package in the cache.")?
                .as_str()
                .context("key `original_url` was expected to be a string but it was not")?;
            let attr_path = package
                .get("attr_path")
                .context("key `attr_path` must exist on every package in the cache.")?
                .as_str()
                .context("key `attr_path` was expected to be a string but it was not")?;

            let cache_package = CachePackage {
                name: name.to_owned(),
                version: version.map(ToOwned::to_owned),
                meta: meta.clone(),
                store_paths,
                url: url.to_owned(),
                original_url: original_url.to_owned(),
                attr_path: attr_path.to_owned(),
            };

            de_packages.push(cache_package);
        }

        Ok(Self {
            packages: de_packages,
        })
    }

    pub fn write(&self) -> color_eyre::Result<()> {
        let mut document = toml_edit::DocumentMut::new();

        let mut packages = ArrayOfTables::new();

        for CachePackage {
            name,
            version,
            store_paths,
            url,
            meta,
            original_url,
            attr_path,
        } in &self.packages
        {
            let mut package_toml = toml_edit::Table::new();

            package_toml.insert("name", name.into());
            package_toml.insert(
                "version",
                version.as_ref().map(Into::into).unwrap_or_default(),
            );
            package_toml.insert(
                "store_paths",
                toml_edit::Item::Value(toml_edit::Value::Array(Array::from_iter(store_paths))),
            );
            package_toml.insert("meta", toml_edit::Item::Table(meta.clone()));
            package_toml.insert("url", url.into());
            package_toml.insert("original_url", original_url.into());
            package_toml.insert("attr_path", attr_path.into());

            packages.push(package_toml);
        }

        document["package"] = toml_edit::Item::ArrayOfTables(packages);

        let cache_dir = setup_cache_dir()?;

        let outfile = cache_dir.join("profile-installed.toml");

        std::fs::write(outfile, document.to_string())?;
        Ok(())
    }

    pub fn push(&mut self, value: CachePackage) {
        self.packages.push(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CachePackageLookupKey {
    attr_path: String,
    url: String,
}

impl CachePackageLookupKey {
    pub fn from_package(package: &nix_installed_list::Package) -> Self {
        Self {
            attr_path: package.attr_path.clone(),
            url: package.url.clone(),
        }
    }
}
pub struct CachePackageLookup {
    lookup: HashMap<CachePackageLookupKey, CachePackage>,
}

impl CachePackageLookup {
    pub fn from_cache_packages(cache_packages: &CachePackages) -> color_eyre::Result<Self> {
        let mut lookup = HashMap::new();

        for package in &cache_packages.packages {
            let key = CachePackageLookupKey {
                attr_path: package.attr_path.clone(),
                url: package.url.clone(),
            };

            lookup.insert(key, package.clone());
        }

        Ok(Self { lookup })
    }

    pub fn lookup(&self, key: &CachePackageLookupKey) -> Option<&CachePackage> {
        self.lookup.get(key)
    }
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
