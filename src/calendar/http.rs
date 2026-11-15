//! Requests for calendar sources: HTTPS anywhere, plain HTTP only on the
//! local network. Redirects are followed by hand so every hop is checked,
//! never downgrades to HTTP, and keeps the login only on the same host.
use super::subscription::SecretText;
use super::*;
use std::net::{IpAddr, SocketAddr};

/// A username and password for HTTP Basic. Both stay in the keyring and
/// print as `[redacted]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    pub username: SecretText,
    pub password: SecretText,
}
impl Login {
    pub(super) fn is_empty(&self) -> bool {
        self.username.0.is_empty() && self.password.0.is_empty()
    }
}

pub(super) const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

/// Host names that may only exist on a home or office network. A single
/// label, such as `nas`, can't be a public DNS name either.
fn local_name(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || !host.contains('.')
        || [".localhost", ".local", ".lan", ".home.arpa", ".internal"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}
/// Loopback, private, unique local, link local and carrier grade NAT
/// (Tailscale) addresses.
pub(super) fn local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || a == 10
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 100 && (64..=127).contains(&b))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return local_ip(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}
/// Whether plain HTTP to this URL may even be tried, before any lookup: the
/// host must be a local address or a local name. Names are still resolved
/// and every address checked before connecting.
pub(super) fn local_host(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(ip) => local_ip(ip),
        Err(_) => local_name(bare),
    }
}
/// Every resolved address must be local, so a local looking name that
/// points at a public server is refused.
pub(super) fn all_local(addrs: &[SocketAddr]) -> bool {
    !addrs.is_empty() && addrs.iter().all(|addr| local_ip(addr.ip()))
}

/// Parses an address typed by a person. `webcal` becomes HTTPS, a bare host
/// becomes HTTPS, and credentials or fragments in the address are refused.
pub(super) fn parse(value: &str, bare_host: bool) -> Result<reqwest::Url, Problem> {
    if value.len() > 8192 {
        return Err(Problem::FeedUrl);
    }
    let value = value.trim();
    let value = if let Some(rest) = value.strip_prefix("webcal://") {
        format!("https://{rest}")
    } else if bare_host && !value.contains("://") {
        format!("https://{value}")
    } else {
        value.into()
    };
    let url = reqwest::Url::parse(&value).map_err(|_| Problem::FeedUrl)?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Problem::FeedUrl);
    }
    if url.scheme() == "http" && !local_host(&url) {
        return Err(Problem::PublicHttp);
    }
    Ok(url)
}
/// True when a typed address will be read over plain HTTP on the local
/// network, so Settings can say the connection is not encrypted.
pub fn plain_local(value: &str, bare_host: bool) -> bool {
    parse(value, bare_host).is_ok_and(|url| url.scheme() == "http")
}

/// Where a request may connect. Plain HTTP is pinned to the addresses that
/// were checked, so a second lookup can't swap in a public server.
async fn target(url: &reqwest::Url) -> Result<Option<(String, Vec<SocketAddr>)>, Problem> {
    if url.scheme() == "https" {
        return Ok(None);
    }
    if url.scheme() != "http" || !local_host(url) {
        return Err(Problem::PublicHttp);
    }
    let host = url.host_str().ok_or(Problem::FeedUrl)?;
    let port = url.port_or_known_default().ok_or(Problem::FeedUrl)?;
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let addrs: Vec<SocketAddr> = if let Ok(ip) = bare.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(
            Duration::from_secs(8),
            tokio::net::lookup_host((bare, port)),
        )
        .await
        .map_err(|_| Problem::Network)?
        .map_err(|_| Problem::Network)?
        .take(32)
        .collect()
    };
    if !all_local(&addrs) {
        return Err(Problem::PublicHttp);
    }
    Ok(Some((bare.to_string(), addrs)))
}

/// What happens to a redirect: the next address, and whether the login may
/// be sent there. HTTPS never moves to HTTP, and the login stays with the
/// host (and port) that it was typed for.
pub(super) fn redirect(
    from: &reqwest::Url,
    location: &str,
    login: bool,
) -> Result<(reqwest::Url, bool), Problem> {
    let next = from.join(location).map_err(|_| Problem::InvalidFeed)?;
    if !matches!(next.scheme(), "https" | "http")
        || !next.username().is_empty()
        || next.password().is_some()
    {
        return Err(Problem::InvalidFeed);
    }
    if from.scheme() == "https" && next.scheme() == "http" {
        return Err(Problem::InsecureRedirect);
    }
    let same_host = from.host_str() == next.host_str()
        && from.port_or_known_default() == next.port_or_known_default();
    Ok((next, login && same_host))
}

pub(super) struct Reply {
    pub status: reqwest::StatusCode,
    /// The address that answered, after redirects. Relative links in a
    /// CalDAV reply resolve against it.
    pub url: reqwest::Url,
    pub bytes: Vec<u8>,
    /// Whether the login was still being sent when this reply came.
    pub sent_login: bool,
}

pub(super) struct Request<'a> {
    pub method: reqwest::Method,
    pub url: reqwest::Url,
    pub login: Option<&'a Login>,
    pub headers: &'a [(&'static str, &'static str)],
    pub body: Option<String>,
}

/// Sends one request and follows up to five redirects. Errors never carry
/// the address, the login or the server's reply.
pub(super) async fn send(request: Request<'_>) -> Result<Reply, Problem> {
    let Request {
        method,
        mut url,
        login,
        headers,
        body,
    } = request;
    let mut with_login = login.is_some_and(|l| !l.is_empty());
    for _ in 0..=MAX_REDIRECTS {
        let mut builder = reqwest::Client::builder()
            .referer(false) // A private address must not leak to another site.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8));
        if let Some((host, addrs)) = target(&url).await? {
            builder = builder.resolve_to_addrs(&host, &addrs);
        }
        let client = builder.build().map_err(|_| Problem::Network)?;
        let mut call = client.request(method.clone(), url.clone());
        for (name, value) in headers {
            call = call.header(*name, *value);
        }
        if let Some(body) = &body {
            call = call
                .header("Content-Type", "application/xml; charset=utf-8")
                .body(body.clone());
        }
        if with_login && let Some(login) = login {
            call = call.basic_auth(&login.username.0, Some(&login.password.0));
        }
        let mut response = call.send().await.map_err(|_| Problem::Network)?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(Problem::InvalidFeed)?
                .to_string();
            let (next, keep) = redirect(&url, &location, with_login)?;
            url = next;
            with_login = keep;
            continue;
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_BYTES as u64)
        {
            return Err(Problem::FeedLimit);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Problem::Network)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
                return Err(Problem::FeedLimit);
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(Reply {
            status,
            url,
            bytes,
            sent_login: with_login,
        });
    }
    Err(Problem::InvalidFeed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses_and_names() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.20",
            "100.64.0.1",
            "100.127.255.254",
            "::1",
            "fd12:3456::1",
            "fe80::1",
            "::ffff:192.168.1.2",
        ] {
            assert!(local_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "169.254.169.254",
            "2606:4700::1111",
            "::ffff:1.1.1.1",
        ] {
            assert!(!local_ip(ip.parse().unwrap()), "{ip}");
        }
        for name in [
            "localhost",
            "nas",
            "nas.local",
            "radicale.lan",
            "box.home.arpa",
            "cal.internal",
        ] {
            assert!(local_name(name), "{name}");
        }
        for name in ["example.com", "local.example.com", "lan.io"] {
            assert!(!local_name(name), "{name}");
        }
    }

    #[test]
    fn http_is_only_for_the_local_network() {
        for public in [
            "http://example.com/cal.ics",
            "http://8.8.8.8/cal.ics",
            "http://[2606:4700::1111]/cal.ics",
        ] {
            assert_eq!(parse(public, false).unwrap_err(), Problem::PublicHttp);
            assert!(!plain_local(public, false));
        }
        for private in [
            "http://192.168.1.5:5232/user/cal/",
            "http://10.0.0.2/cal.ics",
            "http://[fd00::5]/cal.ics",
            "http://nas.local/cal.ics",
            "http://localhost:5232/",
        ] {
            assert_eq!(parse(private, false).unwrap().scheme(), "http");
            assert!(plain_local(private, false));
        }
        assert_eq!(
            parse("caldav.fastmail.com", true).unwrap().as_str(),
            "https://caldav.fastmail.com/"
        );
        assert!(!plain_local("caldav.fastmail.com", true));
        assert_eq!(
            parse("https://user:pass@cal.example.com/", true).unwrap_err(),
            Problem::FeedUrl
        );
    }

    #[test]
    fn a_name_resolving_to_a_public_address_is_refused() {
        let public: Vec<SocketAddr> = vec![
            "192.168.1.2:80".parse().unwrap(),
            "93.184.216.34:80".parse().unwrap(),
        ];
        assert!(!all_local(&public), "one public address refuses the name");
        assert!(!all_local(&[]));
        assert!(all_local(&[
            "192.168.1.2:80".parse().unwrap(),
            "[fd00::2]:80".parse().unwrap()
        ]));
    }

    #[tokio::test]
    async fn local_names_are_resolved_before_connecting() {
        let url = reqwest::Url::parse("http://localhost:5232/").unwrap();
        let (host, addrs) = target(&url).await.unwrap().unwrap();
        assert_eq!(host, "localhost");
        assert!(addrs.iter().all(|a| a.ip().is_loopback()));
        let url = reqwest::Url::parse("http://example.com/").unwrap();
        assert_eq!(target(&url).await.unwrap_err(), Problem::PublicHttp);
        let url = reqwest::Url::parse("https://example.com/").unwrap();
        assert!(target(&url).await.unwrap().is_none());
    }

    #[test]
    fn redirects_never_downgrade_and_keep_the_login_on_its_host() {
        let https = reqwest::Url::parse("https://cal.example.com/dav/").unwrap();
        assert_eq!(
            redirect(&https, "http://cal.example.com/dav/", true).unwrap_err(),
            Problem::InsecureRedirect
        );
        assert_eq!(
            redirect(&https, "http://192.168.1.2/dav/", true).unwrap_err(),
            Problem::InsecureRedirect
        );
        let (next, keep) = redirect(&https, "/remote.php/dav/", true).unwrap();
        assert_eq!(next.as_str(), "https://cal.example.com/remote.php/dav/");
        assert!(keep);
        let (_, keep) = redirect(&https, "https://other.example.net/dav/", true).unwrap();
        assert!(!keep, "a new host must not receive the login");
        let (_, keep) = redirect(&https, "https://cal.example.com:8443/dav/", true).unwrap();
        assert!(!keep, "a new port must not receive the login");
        assert!(redirect(&https, "https://u:p@cal.example.com/", true).is_err());
        assert!(redirect(&https, "file:///etc/passwd", true).is_err());
    }

    #[test]
    fn logins_never_print() {
        let login = Login {
            username: SecretText("alex@example.com".into()),
            password: SecretText("hunter2-app-password".into()),
        };
        let text = format!("{login:?}");
        assert!(!text.contains("alex@example.com") && !text.contains("hunter2"));
    }
}
