use bstr::B;
use nix_elastic_search::response::NixPackage;
use nix_elastic_search::MatchName;
use nix_elastic_search::MatchProgram;
use nix_elastic_search::MatchSearch;
use nix_elastic_search::NixSearchError;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use ordered_float::NotNan;
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

pub fn available_on_this_system(pkg: &NixPackage) -> PackageSupport {
    if pkg.package_platforms.is_empty() {
        PackageSupport::NoneListed
    } else if pkg.package_platforms.iter().any(|val| val == nix_system()) {
        PackageSupport::Supported
    } else {
        PackageSupport::MostLikelyNot
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
) -> Result<Vec<nix_elastic_search::response::NixPackage>, NixSearchError> {
    let (program, name) = match mode {
        SearchMode::Name => (
            None,
            Some(MatchSearch {
                search: query.to_owned(),
            }),
        ),
        SearchMode::Program => (
            Some(MatchProgram {
                program: query.to_owned(),
            }),
            None,
        ),
    };
    let query = nix_elastic_search::Query {
        max_results: 25,
        search_within: nix_elastic_search::SearchWithin::Channel(channel),
        search: name,
        program,
        name: None,
        version: None,
        query_string: None,
    };

    query.send()
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

// retrives the active working system. This call is lazy and will
// not call the shell after the first invocation.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub fn nix_system() -> &'static str {
    // static CURRENT_SYSTEM: Lazy<String> = Lazy::new(|| {
    //     let mut cmd = std::process::Command::new("/nix/var/nix/profiles/default/bin/nix");
    //     cmd.args(&[
    //         "eval",
    //         "--impure",
    //         "--raw",
    //         "--expr",
    //         "builtins.currentSystem",
    //     ]);

    //     dbg!(cmd.output().unwrap().stdout.as_bstr().to_string())
    // });
    // CURRENT_SYSTEM.as_str()
    "aarch64-darwin"
}
#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
pub fn nix_system() -> &'static str {
    // static CURRENT_SYSTEM: Lazy<String> = Lazy::new(|| {
    //     let mut cmd = std::process::Command::new("/nix/var/nix/profiles/default/bin/nix");
    //     cmd.args(&[
    //         "eval",
    //         "--impure",
    //         "--raw",
    //         "--expr",
    //         "builtins.currentSystem",
    //     ]);

    //     dbg!(cmd.output().unwrap().stdout.as_bstr().to_string())
    // });
    // CURRENT_SYSTEM.as_str()
    "arm64-darwin"
}
