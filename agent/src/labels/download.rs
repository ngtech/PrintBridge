use anyhow::{bail, ensure, Context, Result};
use reqwest::{redirect::Policy, StatusCode, Url};
use std::{net::{IpAddr, SocketAddr}, time::Duration};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

pub fn allowed_url(value: &str, origins: &[String]) -> Result<Url> {
    ensure!(value.len() <= 8192, "Document URL is too long");
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid document URL"))?;
    ensure!(url.scheme() == "https" && url.port().is_none()
        && url.username().is_empty() && url.password().is_none() && url.fragment().is_none()
        && matches!(url.host(), Some(url::Host::Domain(_)))
        && origins.contains(&url.origin().ascii_serialization()),
        "Document URL must use an approved HTTPS DNS origin");
    Ok(url)
}

pub fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => match ip.octets() {
            [0 | 10 | 127 | 224..=255, ..] | [100, 64..=127, ..] | [169, 254, ..]
            | [172, 16..=31, ..] | [192, 0 | 168, ..] | [192, 88, 99, _]
            | [198, 18 | 19, ..] | [198, 51, 100, _] | [203, 0, 113, _]
            | [168, 63, 129, 16] => false,
            _ => true,
        },
        IpAddr::V6(ip) => match ip.segments() {
            [0x2001, 0..=0x01ff | 0x0db8, ..] | [0x2002 | 0x3fff, ..] => false,
            [0x2000..=0x3fff, ..] => true,
            _ => false,
        },
    }
}

pub fn checked_addresses(addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>> {
    ensure!(!addresses.is_empty() && addresses.len() <= 32
        && addresses.iter().all(|address| public_address(address.ip())),
        "Document host resolves to a blocked address");
    Ok(addresses)
}

fn redirect_target(url: &Url, location: &str, origins: &[String]) -> Result<Url> {
    let location = url.join(location).map_err(|_| anyhow::anyhow!("Invalid document redirect"))?;
    allowed_url(location.as_str(), origins)
}

pub async fn fetch(url: Url, origins: &[String]) -> Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(30), fetch_bounded(url, origins))
        .await.context("Document download timed out")?
}

async fn fetch_bounded(mut url: Url, origins: &[String]) -> Result<Vec<u8>> {
    for hop in 0..=MAX_REDIRECTS {
        let host = url.host_str().context("Document host is missing")?;
        let resolved = tokio::net::lookup_host((host, 443)).await
            .map_err(|_| anyhow::anyhow!("Document DNS lookup failed"))?;
        let addresses = checked_addresses(resolved.collect())?;
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .resolve_to_addrs(host, &addresses)
            .build().map_err(|_| anyhow::anyhow!("Cannot initialize document download"))?;
        let mut response = client.get(url.clone()).send().await
            .map_err(|_| anyhow::anyhow!("Document request failed"))?;
        if matches!(response.status(), StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND
            | StatusCode::SEE_OTHER | StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT) {
            ensure!(hop < MAX_REDIRECTS, "Too many document redirects");
            let location = response.headers().get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok()).context("Invalid document redirect")?;
            url = redirect_target(&url, location, origins)?;
            continue;
        }
        ensure!(response.status() == StatusCode::OK, "Document server returned HTTP {}", response.status());
        ensure!(response.headers().get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/pdf")),
            "Document server must return application/pdf");
        ensure!(response.content_length().map_or(true, |length| length <= MAX_BYTES as u64),
            "Document exceeds 16 MiB");
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| anyhow::anyhow!("Document stream failed"))? {
            ensure!(bytes.len() + chunk.len() <= MAX_BYTES, "Document exceeds 16 MiB");
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }
    bail!("Too many document redirects")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_https_only_at_every_hop() {
        let origins = vec!["https://labels.example.com".to_owned()];
        let base = allowed_url("https://labels.example.com/labels/1?signature=123", &origins).unwrap();
        assert!(redirect_target(&base, "/labels/2", &origins).is_ok());
        for blocked in ["http://labels.example.com/", "https://labels.example.com:444/",
            "https://labels.example.com.evil.example/", "https://name:password@labels.example.com/",
            "https://labels.example.com/#fragment", "https://127.0.0.1/",
            "https://[::ffff:127.0.0.1]/", "//other.example.org/"] {
            assert!(redirect_target(&base, blocked, &origins).is_err());
        }
    }

    #[test]
    fn rejects_private_special_and_mixed_dns_answers() {
        let public = "8.8.8.8:443".parse().unwrap();
        assert!(checked_addresses(vec![public]).is_ok());
        assert!(public_address("2606:4700::1".parse().unwrap()));
        for blocked in ["127.0.0.1:443", "10.0.0.1:443", "172.16.0.1:443", "192.168.1.1:443",
            "169.254.169.254:443", "100.100.100.200:443", "168.63.129.16:443", "[::1]:443",
            "[::ffff:192.168.1.1]:443", "[fd00:ec2::254]:443", "[fe80::1]:443", "[2002::1]:443"] {
            assert!(checked_addresses(vec![public, blocked.parse().unwrap()]).is_err());
        }
    }
}
