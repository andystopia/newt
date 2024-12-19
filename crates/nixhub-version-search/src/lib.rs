use scraper::Selector;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackageVersionSearchError {
    #[error("Failed to search for package version. Network error: {0}")]
    UreqError(#[from] ureq::Error),
    #[error("Failed to parse package version. IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Selector error: {0}")]
    SelectorError(String),
}

pub fn search_package(exact_name: &str) -> Result<String, PackageVersionSearchError> {
    let res = ureq::get(&format!("https://nixhub.io/packages/{exact_name}"))
        .call()?
        .into_string()?;
    Ok(res)
}

#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub struct VersionLookup {
    pub version: String,
    pub commit: String,
}

pub fn scrape_package_version(
    package_name: &str,
) -> Result<Vec<VersionLookup>, PackageVersionSearchError> {
    let html = search_package(package_name)?;
    let scraper = scraper::Html::parse_document(&html);

    // nixhub.io puts all nixpkgs versions in the
    // article sections of the page. a little strange,
    // but hey, I'm not going to question it too much.
    let article_selector = Selector::parse("article")
        .map_err(|e| e.to_string())
        .map_err(PackageVersionSearchError::SelectorError)?;

    let versions = scraper.select(&article_selector);

    let mut out_versions = Vec::new();

    for version in versions {
        let header_selector = Selector::parse("header > h3")
            .map_err(|e| e.to_string())
            .map_err(PackageVersionSearchError::SelectorError)?;

        let mut headers = version.select(&header_selector);

        let header = headers.next().unwrap();
        let just_version_child = header.children().skip(1).next().unwrap();
        let version_text = 
            just_version_child
                .value()
                .as_text()
                .unwrap()
                .text
                .to_string();
        let ref_selector = Selector::parse("div:first-of-type > p > span:first-of-type")
            .map_err(|e| e.to_string())
            .map_err(PackageVersionSearchError::SelectorError)?;

        let mut refs = version.select(&ref_selector);
        let re = refs.next().unwrap();
        
        let commit_text = re.text().collect::<Vec<_>>().join("");

        out_versions.push(VersionLookup {
            version: version_text,
            commit: commit_text,
        });
    }
    Ok(out_versions)
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_search_package() -> Result<(), Box<dyn Error>> {
        let res = search_package("lazygit")?;
        println!("{}", res);
        Ok(())
    }

    #[test]
    fn test_scraper() -> Result<(), Box<dyn Error>> {
        dbg!(scrape_package_version("lazygit")?);
        Ok(())
    }
}
