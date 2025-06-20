#![allow(dead_code)]

//! # Nix Search for Rust
//!
//! This is an (unfortunate, and certainly less well written) rewrite of [nix-search-cli](https://github.com/peterldowns/nix-search-cli),
//! which is meant to be used in library form in rust for seraching nixpkgs, similar
//! to how [nix search](search.nixos.org) on the web works.
//!
//! This is NOT my original work. This is a derivative work of them. I did write the Rust code, but it's really just
//! a port of their existing work. I unfortunately found this easier than bundling the existing binary or linking to it,
//! so I can depend on it directly. I think the work they did is amazing.
//!
//!
//! This is shifting to a sans-io library as the rust ecosystem for web requests appears to still be developing
//! and therefore, depending on your application you might want reqwest or isaac or ureq2 or ureq3, and I could include
//! some of these, but I want to keep compile times light as possible.
//!
//! ## Examples
//!
//! ```rust
//! let search = NixElasticSearch::new();
//!
//! println!("{:#?}",
//!    search.channel("25.05",
//!      Query {
//!        search: None,
//!        program: None,
//!        name: Some(MatchName {
//!            name: "gleam".to_owned(),
//!        }),
//!        version: None,
//!        query_string: None,
//!      }
//! ))
//! ```
//!
//! This is a sans-io library, so the above doesn't directly search,
//! but does produce an object which can produce the necessary information
//! to create an http request, which can then be called.
mod response;

use std::borrow::Cow;

pub use response::{
    ElasticSearchResponseError, ElasticSearchResponseErrorResource, NixPackage, PackageLicense,
    PackageMaintainer,
};

use base64::prelude::*;
use serde_json::json;
use thiserror::Error;
use url::Url;

#[derive(Debug)]
/// An error handling struct which attempts
/// to figure out where exactly a serde error happened
pub struct SerdeNixPackagePath {
    text: String,
}

impl SerdeNixPackagePath {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn get_error_path(&self) -> String {
        let jd = &mut serde_json::Deserializer::from_str(&self.text);
        let result: Result<response::SearchResponse, _> = serde_path_to_error::deserialize(jd);

        match result {
            Ok(_) => "<no path found>".to_owned(),
            Err(err) => err.path().to_string(),
        }
    }
}

/// Entry point for searching.
#[derive(Debug, Clone)]
pub struct NixElasticSearch {
    pub username: String,
    pub password: String,
    pub url_prefix: Url,
    pub elastic_prefix: String,
}

impl Default for NixElasticSearch {
    fn default() -> Self {
        Self {
            username: "aWVSALXpZv".to_owned(),
            password: "X8gPHnzL52wFEekuxsfQ9cSh".to_owned(),
            url_prefix: Url::parse(
                "https://nixos-search-7-1733963800.us-east-1.bonsaisearch.net:443/",
            )
            .unwrap(),
            elastic_prefix: "latest-*-".to_owned(),
        }
    }
}

impl NixElasticSearch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_url_str(&mut self, url: &str) -> Result<(), url::ParseError> {
        match Url::parse(url) {
            Ok(parsed) => {
                self.url_prefix = parsed;
                Ok(())
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    fn channel_url(&self, channel: &str) -> Result<Url, NixSearchError> {
        self.url_prefix
            .join(&format!("/{}nixos-{channel}/", self.elastic_prefix))
            .map_err(|err| NixSearchError::UrlError { source: err })?
            .join("_search")
            .map_err(|err| NixSearchError::UrlError { source: err })
    }

    fn flakes_url(&self) -> Result<Url, NixSearchError> {
        self.url_prefix
            .join(&format!("{}group-manual/", self.elastic_prefix))
            .map_err(|err| NixSearchError::UrlError { source: err })?
            .join("_search")
            .map_err(|err| NixSearchError::UrlError { source: err })
    }

    pub fn channel_is_searchable_request(&self, channel: &str) -> NixElasticSearchHttpRequest {
        use std::borrow::Cow::Borrowed as B;
        use std::borrow::Cow::Owned as O;
        let request_url = self.channel_url(channel).unwrap();

        let auth = BASE64_STANDARD.encode(format!("{}:{}", &self.username, &self.password));

        let headers = vec![
            (B("Authorization"), O(auth)),
            (B("Content-Type"), B("application/json")),
            (B("Accept"), B("application/json")),
        ];
        NixElasticSearchHttpRequest {
            verb: HttpVerb::GET,
            url: request_url,
            headers,
            body: None,
        }
    }

    pub fn channel_is_searchable_response(&self, body: String) -> Result<bool, NixSearchError> {
        let body = serde_json::from_str::<serde_json::Value>(&body).map_err(|err| {
            NixSearchError::DeserializationError {
                path: SerdeNixPackagePath::new(body),
                source: err,
            }
        })?;

        let Some(object) = body.as_object() else {
            return Ok(false);
        };

        Ok(object.keys().len() > 0)
    }

    /// search within a channel, should be denoted like 25.05
    /// NOT nixos-25.05
    pub fn channel(&self, channel: String, query: Query) -> SearchQuery<'_> {
        SearchQuery {
            nes: &self,
            search_within: SearchWithin::Channel(channel),
            query,
        }
    }
    pub fn flakes(&self, query: Query) -> SearchQuery<'_> {
        SearchQuery {
            nes: &self,
            search_within: SearchWithin::Flakes,
            query,
        }
    }
}

pub struct SearchQuery<'nes> {
    nes: &'nes NixElasticSearch,
    search_within: SearchWithin,
    query: Query,
}

impl<'nes> SearchQuery<'nes> {
    fn get_url(&self) -> Result<Url, NixSearchError> {
        match &self.search_within {
            SearchWithin::Channel(channel) => self.nes.channel_url(channel),
            SearchWithin::Flakes => self.nes.flakes_url(),
        }
    }

    pub fn search_request(&self) -> NixElasticSearchHttpRequest {
        use std::borrow::Cow::Borrowed as B;
        use std::borrow::Cow::Owned as O;
        let request_url = self.get_url().unwrap();

        let auth = BASE64_STANDARD.encode(format!("{}:{}", self.nes.username, self.nes.password));
        let auth_line = format!("Basic {}", auth);
        let headers = vec![
            (B("Authorization"), O(auth_line)),
            (B("Content-Type"), B("application/json")),
            (B("Accept"), B("application/json")),
        ];
        NixElasticSearchHttpRequest {
            verb: HttpVerb::POST,
            url: request_url,
            headers,
            body: Some(self.query.payload().to_string()),
        }
    }

    pub fn search_response(&self, body: String) -> Result<Vec<NixPackage>, NixSearchError> {
        let read = match serde_json::from_str::<response::SearchResponse>(&body) {
            Ok(r) => r,
            Err(err) => {
                return Err(NixSearchError::DeserializationError {
                    path: SerdeNixPackagePath::new(body),
                    source: err,
                })
            }
        };

        match read {
            response::SearchResponse::Error { error, status } => {
                Err(NixSearchError::ElasticSearchError { error, status })
            }
            response::SearchResponse::Success { packages } => Ok(packages),
        }
    }
}

/// chose whether to search in flakes or by channel
enum SearchWithin {
    /// should be something like 23.11 (not nixos-23.11)
    Channel(String),
    Flakes,
}

#[derive(Debug, Error)]
/// the possible errors that can happen in this library
pub enum NixSearchError {
    #[error("serde_json (used to parse json) encounted an unexpected error: {source}, at path: {}", path.get_error_path())]
    DeserializationError {
        path: SerdeNixPackagePath,
        source: serde_json::Error,
    },
    #[error("the elastic search endpoint had a server error")]
    ElasticSearchError {
        error: ElasticSearchResponseError,
        status: i64,
    },

    #[error("invalid package name error. failed to create url for: {package_name}")]
    InvalidPackageNameError {
        package_name: String,
        #[source]
        source: url::ParseError,
    },

    #[error("Invalid Nix Search Url")]
    UrlError {
        #[source]
        source: url::ParseError,
    },
}

pub type Result<T, E = NixSearchError> = ::std::result::Result<T, E>;

#[derive(Debug)]
pub enum HttpVerb {
    GET,
    POST,
}

#[derive(Debug)]
pub struct NixElasticSearchHttpRequest {
    pub verb: HttpVerb,
    pub url: Url,
    pub headers: Vec<(Cow<'static, str>, Cow<'static, str>)>,
    pub body: Option<String>,
}

#[derive(Default)]
pub struct Query {
    pub max_results: u32,

    pub search: Option<MatchSearch>,
    pub program: Option<MatchProgram>,
    pub name: Option<MatchName>,
    pub version: Option<MatchVersion>,
    pub query_string: Option<MatchQueryString>,
}

impl Query {
    fn payload(&self) -> serde_json::Value {
        let starting_payload = json!({
           "match": {
                "type": "package",
            }
        });

        let must = [
            Some(starting_payload),
            self.search.as_ref().map(MatchSearch::to_json),
            self.program.as_ref().map(MatchProgram::to_json),
            self.name.as_ref().map(MatchName::to_json),
            self.version.as_ref().map(MatchVersion::to_json),
            self.query_string.as_ref().map(MatchQueryString::to_json),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        json!({
            "from": 0,
            "size": self.max_results,
            "sort": {
                "_score":            "desc",
                "package_attr_name": "desc",
                "package_pversion":  "desc",
            },
            "query": {
                "bool": {
                    "must": must,
                },
            }
        })
    }
}

/// search by search string (like search.nixos.org -- I believe)
pub struct MatchSearch {
    pub search: String,
}

impl MatchSearch {
    pub fn to_json(&self) -> serde_json::Value {
        let multi_match_name = format!("multi_match_{}", self.search.replace(' ', "_"));
        let initial_query = json!({
                "multi_match": {
                    "type":  "cross_fields",
                    "_name": multi_match_name,
                    "query": self.search,
                    "fields": [
                        "package_attr_name^9",
                        "package_attr_name.*^5.3999999999999995",
                        "package_programs^9",
                        "package_programs.*^5.3999999999999995",
                        "package_pname^6",
                        "package_pname.*^3.5999999999999996",
                        "package_description^1.3",
                        "package_description.*^0.78",
                        "package_pversion^1.3",
                        "package_pversion.*^0.78",
                        "package_longDescription^1",
                        "package_longDescription.*^0.6",
                        "flake_name^0.5",
                        "flake_name.*^0.3",
                        "flake_resolved.*^99",
                    ]
            }
        });

        let queries = std::iter::once(initial_query)
            .chain(self.search.split(' ').map(|split| {
                json!( {
                        "wildcard": {
                            "package_attr_name": {
                                "value": format!("*{}*", split),
                                "case_insensitive": true,
                            },
                        }
                    }
                )
            }))
            .collect::<Vec<_>>();

        json!({
            "dis_max":  {
                "tie_breaker": 0.7,
                "queries": queries,
            }
        })
    }
}

/// search by name
pub struct MatchName {
    pub name: String,
}

impl MatchName {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "dis_max": {
                "tie_breaker": 0.7,
                "queries": [
                    {
                        "wildcard": {
                            "package_attr_name": {
                                "value": format!("{}*", self.name),
                            }
                        }
                    },
                    {
                        "match": {
                            "package_programs": self.name,
                        }
                    }
                ]
            }
        })
    }
}

/// search by programs
pub struct MatchProgram {
    pub program: String,
}

impl MatchProgram {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "dis_max": {
                "tie_breaker": 0.7,
                "queries": [
                    {
                        "wildcard": {
                            "package_programs": {
                                "value": format!("{}*", self.program),
                            }
                        }
                    },
                    {
                        "match": {
                            "package_programs": self.program,
                        }
                    }
                ]
            }
        })
    }
}

/// search by versions
pub struct MatchVersion {
    pub version: String,
}

impl MatchVersion {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "dis_max": {
                "tie_breaker": 0.7,
                "queries": [
                    {
                        "wildcard": {
                            "package_pversion": {
                                "value": format!("{}*", self.version),
                            }
                        }
                    },
                    {
                        "match": {
                            "package_pversion": self.version,
                        }
                    }
                ]
            }
        })
    }
}

/// search by query string
pub struct MatchQueryString {
    pub query_string: String,
}

impl MatchQueryString {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "query_string": {
                "query": self.query_string,
            }
        })
    }
}

#[cfg(test)]
mod test {

    // #[test]
    // fn test_search() {
    //     let query = Query {
    //         max_results: 20,
    //         search_within: SearchWithin::Channel("23.11".to_owned()),

    //         search: None,
    //         program: None,
    //         name: Some(MatchName {
    //             name: "cargo".to_owned(),
    //         }),
    //         version: None,
    //         query_string: None,
    //     };

    //     let results = query.send().unwrap();

    //     let res = results
    //         .into_iter()
    //         .map(|p| {
    //             format!(
    //                 "{}: {}",
    //                 p.package_attr_name,
    //                 p.package_description.unwrap_or_default()
    //             )
    //         })
    //         .collect::<Vec<_>>();

    //     println!("{:?}", res);
    // }

    // #[test]
    // fn test_search_name() {
    //     let query = Query {
    //         max_results: 10,
    //         search_within: SearchWithin::Channel("23.11".to_owned()),
    //         search: None,
    //         program: None,
    //         name: Some(MatchName {
    //             name: "rust".to_owned(),
    //         }),
    //         version: None,
    //         query_string: None,
    //     };

    //     query.send().unwrap();
    // }
}
