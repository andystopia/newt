use serde::Deserialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChannelRequestError {
    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },
    #[error("Ureq error: {source}")]
    Ureq {
        #[from]
        source: ureq::Error,
    },
    #[error("Xml error: {source}")]
    XmlError {
        #[from]
        source: quick_xml::de::DeError,
    },
}

pub fn get_channel_text() -> Result<String, ChannelRequestError> {
    let channel_details = ureq::get("https://nix-channels.s3.amazonaws.com/?delimiter=/")
        .call()?
        .into_string()?;
    Ok(channel_details)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListBucketResult {
    common_prefixes: Vec<CommonPrefix>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommonPrefix {
    prefix: String,
}

pub fn get_channel_list() -> Result<ListBucketResult, ChannelRequestError> {
    let channel_details = get_channel_text()?;
    let parsed = quick_xml::de::from_str(&channel_details)?;
    Ok(parsed)
}

pub fn get_full_channels() -> Result<Vec<String>, ChannelRequestError> {
    let channel_list = get_channel_list()?;
    let mut channels = Vec::new();
    for prefix in channel_list.common_prefixes {
        let name = prefix.prefix;
        if name.starts_with("nixos-") {
            let name = name.trim_end_matches('/');
            let name_parts = name.split('-').collect::<Vec<&str>>();

            let ["nixos", year_month] = name_parts.as_slice() else {
                continue;
            };

            let date_parts = year_month.split('.').count();

            if date_parts != 2 {
                continue;
            }

            channels.push(year_month.to_string());
        }
    }
    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_channel_text() {
        let channel_text = get_channel_text().unwrap();
        assert!(channel_text.contains("nixos-21.05"));
    }

    #[test]
    fn test_get_channel_list() -> Result<(), Box<dyn std::error::Error>> {
        let channel_list = get_channel_list()?;

        dbg!(channel_list);
        Ok(())
    }

    #[test]
    fn test_simple_channels() -> Result<(), Box<dyn std::error::Error>> {
        let mut channel_list = get_full_channels()?;

        channel_list.sort();

        dbg!(channel_list);
        Ok(())
    }
}
