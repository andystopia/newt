use std::{borrow::Cow, collections::HashMap, path::PathBuf, str::FromStr};

use snafu::{OptionExt, ResultExt, Snafu};
use toml_edit::{Array, ArrayOfTables};

#[derive(Debug, Snafu)]
pub enum NixInstalledListCacheError {
    #[snafu(display("User must have a HOME directory env var set"))]
    MissingHomeDir {},

    #[snafu(display("failed to create source cache path dir path"))]
    FailedToCreateCacheDirPath { source: std::io::Error },
    #[snafu(display("failed to create source cache dir path"))]
    FailedToCreateCacheDir { source: std::io::Error },

    #[snafu(display("Installed Packages cache file failed to read"))]
    InstalledPackagesCacheFileFailedToRead { source: std::io::Error },

    #[snafu(display("Installed Packages cache file failed to deserialize into a TOML"))]
    InstalledPackagesCacheFileFailedToDeserializeToml { source: toml_edit::TomlError },

    #[snafu(display(
        "The package key `{key}` was expected to in every entry in the cache file, but it was not"
    ))]
    MissingPackageKey { key: Cow<'static, str> },

    #[snafu(display(
        "The package key `{key}` expected to have value of type `{ty}`, but it did not"
    ))]
    IncorrectPackageValueType {
        key: Cow<'static, str>,
        ty: Cow<'static, str>,
    },

    #[snafu(display("all store paths in cache must be strings, the store_paths must be an array of strings, but at least one was not"))]
    StorePathArrayContainedNonString,

    #[snafu(display("failed to write cache to cache file"))]
    FailedToWriteCache { source: std::io::Error },
}

pub fn setup_cache_dir() -> Result<PathBuf, NixInstalledListCacheError> {
    let home_dir = std::env::var_os("HOME").context(MissingHomeDirSnafu {})?;

    let home_dir_path = PathBuf::from(home_dir);
    let cache_dir_path = home_dir_path.join(".cache/");
    let gnix_cache_dir = cache_dir_path.join("gnix/");

    if !cache_dir_path.exists() {
        std::fs::create_dir(&cache_dir_path).context(FailedToCreateCacheDirPathSnafu {})?;
    }

    if !gnix_cache_dir.exists() {
        std::fs::create_dir(&gnix_cache_dir).context(FailedToCreateCacheDirPathSnafu {})?;
    }

    Ok(gnix_cache_dir)
}

#[derive(Clone, Debug)]
pub struct CachePackage {
    pub name: String,
    pub version: Option<String>,
    pub meta: toml_edit::Table,
    pub store_paths: Vec<String>,
    pub url: String,
    pub original_url: String,
    pub attr_path: String,
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

    pub fn load() -> Result<Self, NixInstalledListCacheError> {
        let cache_dir = setup_cache_dir()?;

        let file = cache_dir.join("profile-installed.toml");

        if !file.exists() {
            return Ok(Self::new());
        }

        let mut de_packages = Vec::new();

        let file_contents =
            std::fs::read_to_string(file).context(InstalledPackagesCacheFileFailedToReadSnafu)?;

        let toml_doc = toml_edit::DocumentMut::from_str(&file_contents)
            .context(InstalledPackagesCacheFileFailedToDeserializeTomlSnafu)?;

        let Some(packages) = toml_doc.get("package") else {
            return Ok(Self::new());
        };

        let Some(packages) = packages.as_array_of_tables() else {
            return Ok(Self::new());
        };

        for package in packages {
            let name = package
                .get("name")
                .context(MissingPackageKeySnafu { key: "name" })?
                .as_str()
                .context(IncorrectPackageValueTypeSnafu {
                    key: "name",
                    ty: "string",
                })?;

            let version = package
                .get("version")
                .map(|v| {
                    v.as_str().context(IncorrectPackageValueTypeSnafu {
                        key: "version",
                        ty: "string",
                    })
                })
                .transpose()?;
            let meta = package
                .get("meta")
                .context(MissingPackageKeySnafu { key: "meta" })?
                .as_table()
                .context(IncorrectPackageValueTypeSnafu {
                    key: "meta",
                    ty: "table",
                })?;
            let store_paths = package
                .get("store_paths")
                .context(MissingPackageKeySnafu { key: "store_paths" })?
                .as_array()
                .context(IncorrectPackageValueTypeSnafu {
                    key: "store_paths",
                    ty: "array",
                })?
                .into_iter()
                .map(|v| {
                    v.as_str()
                        .map(ToOwned::to_owned)
                        .context(StorePathArrayContainedNonStringSnafu)
                })
                .collect::<Result<Vec<String>, NixInstalledListCacheError>>()?;

            let url = package
                .get("url")
                .context(MissingPackageKeySnafu { key: "url" })?
                .as_str()
                .context(IncorrectPackageValueTypeSnafu {
                    key: "url",
                    ty: "string",
                })?;
            let original_url = package
                .get("original_url")
                .context(MissingPackageKeySnafu {
                    key: "original_url",
                })?
                .as_str()
                .context(IncorrectPackageValueTypeSnafu {
                    key: "url",
                    ty: "string",
                })?;
            let attr_path = package
                .get("attr_path")
                .context(MissingPackageKeySnafu { key: "attr_path" })?
                .as_str()
                .context(IncorrectPackageValueTypeSnafu {
                    key: "attr_path",
                    ty: "string",
                })?;

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

    pub fn write(&self) -> Result<(), NixInstalledListCacheError> {
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

        std::fs::write(outfile, document.to_string()).context(FailedToWriteCacheSnafu)?;
        Ok(())
    }

    pub fn push(&mut self, value: CachePackage) {
        self.packages.push(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CachePackageLookupKey {
    pub attr_path: String,
    pub url: String,
}

impl CachePackageLookupKey {
    pub fn from_package(package: &crate::Package) -> Self {
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
    pub fn from_cache_packages(
        cache_packages: &CachePackages,
    ) -> Result<Self, NixInstalledListCacheError> {
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
