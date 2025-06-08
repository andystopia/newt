use nix_elastic_search::NixPackage;

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
struct QueryQuality {
    dist: usize,
    proportionality: isize,
}

fn search_by_name_metric(query: &str, name: &str) -> QueryQuality {
    QueryQuality {
        // sort first by the longest common subsequence between the queries
        dist: longest_common_subsequence_length(query.as_bytes(), name.as_bytes()),
        // next sort how many characters are different between the queries.
        proportionality: -(query.len().abs_diff(name.len()) as isize),
    }
}

pub fn sort_packages(search_text: &String, pkgs: &mut Vec<NixPackage>) {
    pkgs.sort_by_cached_key(|v| {
        let has_exact_binary_match = v.package_programs.contains(search_text);
        (
            has_exact_binary_match,
            search_by_name_metric(&search_text.to_owned(), &v.package_attr_name),
        )
    });

    pkgs.reverse();

    for pkg in pkgs {
        pkg.package_programs.sort_unstable();
    }
}
