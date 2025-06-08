use nix_elastic_search::{
    HttpVerb, NixElasticSearch, NixElasticSearchHttpRequest, NixPackage, NixSearchError, Query,
};

fn http_query(
    NixElasticSearchHttpRequest {
        verb,
        url,
        headers,
        body,
    }: NixElasticSearchHttpRequest,
) -> Result<String, ureq::Error> {
    match verb {
        HttpVerb::GET => {
            let mut req = ureq::get(url.to_string());

            for (key, value) in headers {
                req = req.header(&*key, &*value);
            }

            let sent = req.call()?;
            Ok(sent.into_body().read_to_string()?)
        }

        HttpVerb::POST => {
            let mut req = ureq::post(url.to_string());
            for (key, value) in headers {
                req = req.header(&*key, &*value);
            }
            let sent = match body {
                Some(body) => req.send(body),
                None => req.send_empty(),
            }?;

            Ok(sent.into_body().read_to_string()?)
        }
    }
}

pub struct UreqNixSearcher {
    nix_elastic_search: NixElasticSearch,
}

impl UreqNixSearcher {
    pub fn new(nix_elastic_search: NixElasticSearch) -> Self {
        Self { nix_elastic_search }
    }

    /// checks if a channel is searchable using ureq
    pub fn channel_is_searchable(
        &self,
        channel: &str,
    ) -> Result<Result<bool, NixSearchError>, ureq::Error> {
        let body = http_query(
            self.nix_elastic_search
                .channel_is_searchable_request(channel),
        )?;

        Ok(self.nix_elastic_search.channel_is_searchable_response(body))
    }

    pub fn channel(
        &self,
        channel: &str,
        query: Query,
    ) -> Result<Result<Vec<NixPackage>, NixSearchError>, ureq::Error> {
        let channel_query = self.nix_elastic_search.channel(channel.to_owned(), query);
        let req = channel_query.search_request();

        let body = http_query(req)?;
        Ok(channel_query.search_response(body))
    }
}
