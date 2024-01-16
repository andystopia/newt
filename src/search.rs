use bstr::B;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use ordered_float::NotNan;
use scraper::Selector;
use serde::Deserialize;
use serde::Serialize;
use snafu::OptionExt;
use snafu::ResultExt;
use strsim::levenshtein;
use strsim::normalized_levenshtein;
use url::Url;

use crate::nix;
use crate::DeserializeSnafu;
use crate::ProcessSnafu;
use crate::ProgramError;
use crate::SearchMode;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PackageSupport {
    Supported,
    MostLikelyNot,
    NoneListed,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct PackageSearchValue {
    #[serde(rename = "type")]
    pub type_field: String,
    pub package_pname: String,
    pub package_attr_name: String,
    pub package_attr_set: String,
    pub package_outputs: Vec<String>,
    pub package_description: String,
    pub package_programs: Vec<String>,
    pub package_homepage: Vec<String>,
    pub package_pversion: String,
    pub package_platforms: Vec<String>,
    pub package_position: String,
    pub package_license: Vec<PackageLicense>,
    pub flake_name: String,
    pub flake_description: String,
    pub flake_resolved: FlakeResolved,
    pub versions: Option<Vec<PackageVersion>>,
}

impl PackageSearchValue {
    pub fn available_on_this_system(&self) -> PackageSupport {
        if self.package_platforms.is_empty() {
            PackageSupport::NoneListed
        } else if self.package_platforms.iter().any(|val| val == nix_system()) {
            PackageSupport::Supported
        } else {
            PackageSupport::MostLikelyNot
        }
    }

    pub fn compute_versions(&mut self) {
        self.versions
            .get_or_insert_with(|| lookup_package_versions(&self.package_pname).unwrap());
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct PackageLicense {
    #[serde(rename = "fullName")]
    pub full_name: String,
    pub url: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct FlakeResolved {
    #[serde(rename = "type")]
    pub type_field: String,
    pub owner: String,
    pub repo: String,
    pub url: String,
}

use bstr::ByteSlice;
pub fn search(
    query: &str,
    mode: SearchMode,
    channel: String,
) -> Result<Vec<PackageSearchValue>, ProgramError> {
    let mut cmd = nix();
    cmd.args(&["run", "github:peterldowns/nix-search-cli", "--", "--json"]);
    let name_binding = ["--search", query];
    let program_binding = ["--program", query];
    cmd.args(match mode {
        SearchMode::Name => &name_binding,
        SearchMode::Program => &program_binding,
    });
    cmd.args(&[format!("--channel={}", channel)]);

    let output = cmd.output().context(ProcessSnafu {
        goal: format!("to search for `{query}`"),
        command: "nix-search-cli",
    })?;

    if !output.status.success() {
        return Err(ProgramError::BadExitCode {
            goal: format!("to search nixpkgs for `{query}`"),
            command: "nix-search-cli".to_owned(),
            stderr: output.stderr.as_bstr().to_string(),
            exit_code: output.status.code().unwrap(),
        });
    }

    let out = output
        .stdout
        .as_slice()
        .lines()
        .map(|line| serde_json::from_slice::<PackageSearchValue>(line))
        .map(|res| -> Result<_, ProgramError> {
            let res = res.context(DeserializeSnafu {
                goal: "to parse the results of a search",
            })?;
            Ok(res)
        })
        .collect::<Result<Vec<PackageSearchValue>, ProgramError>>()?;
    Ok(out)
}

fn longest_common_subsequence_length(seq1: &[u8], seq2: &[u8]) -> usize {
    let mut dp = vec![vec![0; seq2.len() + 1]; seq1.len() + 1];

    for i in 1..=seq1.len() {
        for j in 1..=seq2.len() {
            if seq1[i - 1] == seq2[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    dp[seq1.len()][seq2.len()]
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryQuality {
    dist: usize,
    proportionality: isize,
}

pub fn search_by_name_metric(query: &str, name: &str) -> QueryQuality {
    QueryQuality {
        // sort first by the longest common subsequence between the queries
        dist: longest_common_subsequence_length(query.as_bytes(), name.as_bytes()),
        // next sort how many characters are different between the queries.
        proportionality: -(query.len().abs_diff(name.len()) as isize),
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVersion {
    pub version: String,
    pub revision: String,
    pub date: String,
}

pub fn lookup_package_versions(package_name: &str) -> Result<Vec<PackageVersion>, snafu::Whatever> {
    let url = Url::parse_with_params(
        "https://lazamar.co.uk/nix-versions/",
        [("channel", "nixpkgs-unstable"), ("package", package_name)],
    )
    .whatever_context("couldn't not build version query url")?;

    let site = reqwest::blocking::get(url.clone())
        .whatever_context(format!("could not retrieve {url:?}"))?;

    let site_text = site
        .text()
        .whatever_context("could not get version site html")?;

    let parsed = scraper::Html::parse_document(&site_text);

    let select = Selector::parse("html > body > section > table > tbody").unwrap();
    let mut parse = parsed.select(&select);

    let element = parse.next().whatever_context("no table found in version website; we expect the version website to have a table, but it didn't here.")?;
    let row_selector = Selector::parse("tr").unwrap();

    let mut versions = Vec::new();
    for row in element.select(&row_selector) {
        let version = row.text().nth(1).map(ToOwned::to_owned);
        let revision = row.text().nth(2).map(ToOwned::to_owned);
        let date = row.text().nth(3).map(ToOwned::to_owned);

        if let Some(((version, revision), date)) = version.zip(revision).zip(date) {
            versions.push(PackageVersion {
                version,
                revision,
                date,
            });
        }
    }
    Ok(versions)
}

// retrives the active working system. This call is lazy and will
#[test]
pub fn test_page_lookup() {
    dbg!(lookup_package_versions("hello"));
}
// not call the shell after the first invocation.
pub fn nix_system() -> &'static str {
    static CURRENT_SYSTEM: Lazy<String> = Lazy::new(|| {
        let mut cmd = std::process::Command::new("nix");
        cmd.args(&[
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ]);

        dbg!(cmd.output().unwrap().stdout.as_bstr().to_string())
    });
    CURRENT_SYSTEM.as_str()
}
