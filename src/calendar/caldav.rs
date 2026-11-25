//! CalDAV (RFC 4791) reading: discovery through RFC 6764, then one
//! calendar-query REPORT per calendar and month. The events go through the
//! same isolated parser and limits as calendar links.
use super::http::{self, Login, Reply, Request};
use super::subscription::SecretText;
use super::*;

const DAV: &str = "DAV:";
const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
const APPLE: &str = "http://apple.com/ns/ical/";
/// More calendars than this on one account are not listed.
const MAX_FOUND: usize = 64;

/// A calendar found on a CalDAV account. Its address can name the account,
/// so it stays redacted and goes to the keyring with the login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalDavCalendar {
    pub url: SecretText,
    pub name: String,
    /// `#rrggbb`, or empty when the server has no color for it.
    pub color: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Props {
    pub principal: Option<String>,
    pub home: Option<String>,
    pub calendar: bool,
    /// None when the server does not say which components it holds.
    pub events: Option<bool>,
    pub name: Option<String>,
    pub color: Option<String>,
    pub data: Option<String>,
}

fn is(node: &roxmltree::Node, ns: &str, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(ns) && node.tag_name().name() == name
}
fn text(node: roxmltree::Node) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}
fn child_href(node: roxmltree::Node) -> Option<String> {
    node.children()
        .find(|n| is(n, DAV, "href"))
        .map(|n| text(n).trim().to_string())
        .filter(|h| !h.is_empty())
}
fn ok_status(node: roxmltree::Node) -> bool {
    node.children()
        .find(|n| is(n, DAV, "status"))
        .map(text)
        .is_none_or(|status| {
            status
                .split_whitespace()
                .nth(1)
                .is_some_and(|code| code.starts_with('2'))
        })
}

/// Reads a 207 Multi-Status body: each response's href with the properties
/// the server reported as found. Properties with a 404 propstat are ignored.
pub(super) fn multistatus(bytes: &[u8]) -> Result<Vec<(String, Props)>, Problem> {
    let body = std::str::from_utf8(bytes).map_err(|_| Problem::NotCalDav)?;
    // roxmltree refuses DTDs by default, so no entity can expand the reply.
    let doc = roxmltree::Document::parse(body).map_err(|_| Problem::NotCalDav)?;
    let root = doc.root_element();
    if !is(&root, DAV, "multistatus") {
        return Err(Problem::NotCalDav);
    }
    let mut out = vec![];
    for response in root.children().filter(|n| is(n, DAV, "response")) {
        let Some(href) = child_href(response) else {
            continue;
        };
        let mut props = Props::default();
        for propstat in response.children().filter(|n| is(n, DAV, "propstat")) {
            if !ok_status(propstat) {
                continue;
            }
            for prop in propstat.children().filter(|n| is(n, DAV, "prop")) {
                for p in prop.children().filter(|n| n.is_element()) {
                    if is(&p, DAV, "current-user-principal") {
                        props.principal = child_href(p);
                    } else if is(&p, CALDAV, "calendar-home-set") {
                        props.home = child_href(p);
                    } else if is(&p, DAV, "resourcetype") {
                        props.calendar = p.children().any(|n| is(&n, CALDAV, "calendar"));
                    } else if is(&p, CALDAV, "supported-calendar-component-set") {
                        props.events = Some(p.children().any(|n| {
                            is(&n, CALDAV, "comp")
                                && n.attribute("name")
                                    .is_some_and(|v| v.eq_ignore_ascii_case("VEVENT"))
                        }));
                    } else if is(&p, DAV, "displayname") {
                        let name: String = text(p).trim().chars().take(80).collect();
                        props.name = (!name.is_empty()).then_some(name);
                    } else if is(&p, APPLE, "calendar-color") {
                        props.color = color(&text(p));
                    } else if is(&p, CALDAV, "calendar-data") {
                        props.data = Some(text(p));
                    }
                }
            }
        }
        out.push((href, props));
    }
    Ok(out)
}
/// Apple writes `#RRGGBB` or `#RRGGBBAA`; Halcyon keeps `#rrggbb`.
pub(super) fn color(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    (matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex[..6].to_ascii_lowercase()))
}

const DISCOVER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="http://apple.com/ns/ical/">
  <d:prop>
    <d:current-user-principal/>
    <c:calendar-home-set/>
    <d:resourcetype/>
    <d:displayname/>
    <a:calendar-color/>
    <c:supported-calendar-component-set/>
  </d:prop>
</d:propfind>"#;

fn status(reply: &Reply) -> Result<(), Problem> {
    match reply.status.as_u16() {
        207 => Ok(()),
        401 => Err(Problem::Credentials),
        403 if reply.sent_login => Err(Problem::Credentials),
        // A web page, a missing path or a refused method: not CalDAV here.
        200..=499 => Err(Problem::NotCalDav),
        _ => Err(Problem::Network),
    }
}
async fn propfind(
    url: &reqwest::Url,
    login: &Login,
    depth: &'static str,
) -> Result<(reqwest::Url, Vec<(String, Props)>), Problem> {
    let reply = http::send(Request {
        method: reqwest::Method::from_bytes(b"PROPFIND").map_err(|_| Problem::NotCalDav)?,
        url: url.clone(),
        login: Some(login),
        headers: &[("Depth", depth), ("Accept", "application/xml, text/xml")],
        body: Some(DISCOVER.into()),
        limit: http::MAX_BYTES,
    })
    .await?;
    status(&reply)?;
    Ok((reply.url.clone(), multistatus(&reply.bytes)?))
}

/// Two hosts belong to one provider when their last two labels match, as
/// with iCloud's `caldav.icloud.com` and `p42-caldav.icloud.com`.
fn same_site(a: &reqwest::Url, b: &reqwest::Url) -> bool {
    let site = |u: &reqwest::Url| {
        let host = u.host_str()?.to_ascii_lowercase();
        if host.parse::<std::net::IpAddr>().is_ok() || host.starts_with('[') {
            return Some(host);
        }
        let labels: Vec<_> = host.trim_end_matches('.').rsplit('.').take(2).collect();
        Some(labels.join("."))
    };
    site(a).is_some() && site(a) == site(b)
}
/// Resolves an href from a reply. The login follows it only to the same
/// provider, and never from HTTPS to HTTP.
fn follow(base: &reqwest::Url, href: &str) -> Result<reqwest::Url, Problem> {
    let next = base.join(href).map_err(|_| Problem::NotCalDav)?;
    let same_origin = next.scheme() == base.scheme()
        && next.host_str() == base.host_str()
        && next.port_or_known_default() == base.port_or_known_default();
    if !next.username().is_empty() || next.password().is_some() || next.fragment().is_some() {
        return Err(Problem::NotCalDav);
    }
    if same_origin || (next.scheme() == "https" && same_site(base, &next)) {
        Ok(next)
    } else {
        Err(Problem::NotCalDav)
    }
}

pub(super) fn calendars(base: &reqwest::Url, found: Vec<(String, Props)>) -> Vec<CalDavCalendar> {
    let mut out: Vec<CalDavCalendar> = vec![];
    for (href, props) in found {
        if !props.calendar || props.events == Some(false) {
            continue;
        }
        let Ok(url) = follow(base, &href) else {
            continue;
        };
        let url = SecretText(url.into());
        if out.iter().any(|c| c.url.0 == url.0) {
            continue;
        }
        let fallback = decode(
            href.trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default(),
        )
        .chars()
        .take(80)
        .collect::<String>();
        out.push(CalDavCalendar {
            url,
            name: props.name.unwrap_or(if fallback.is_empty() {
                "Calendar".into()
            } else {
                fallback
            }),
            color: props.color.unwrap_or_default(),
        });
        if out.len() >= MAX_FOUND {
            break;
        }
    }
    out
}

/// Percent decoding for a calendar's folder name, used when it has no
/// display name.
fn decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = segment.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Finds the calendars on an account. The address can be the server, the
/// account's DAV root or one calendar.
pub async fn discover(server: &SecretText, login: &Login) -> Result<Vec<CalDavCalendar>, Problem> {
    let start = http::parse(&server.0, true)?;
    let mut known = start.clone();
    known.set_path("/.well-known/caldav");
    known.set_query(None);
    let candidates = if matches!(start.path(), "" | "/") {
        [known, start]
    } else {
        [start, known]
    };
    let mut problem = Problem::NotCalDav;
    for candidate in candidates {
        match discover_at(&candidate, login).await {
            Ok(found) => return Ok(found),
            Err(p @ (Problem::Credentials | Problem::NoCalendars | Problem::PublicHttp)) => {
                return Err(p);
            }
            Err(Problem::NotCalDav) => {}
            Err(p) => problem = p,
        }
    }
    Err(problem)
}
async fn discover_at(url: &reqwest::Url, login: &Login) -> Result<Vec<CalDavCalendar>, Problem> {
    let (base, found) = propfind(url, login, "0").await?;
    // A calendar's own address.
    if found.first().is_some_and(|(_, p)| p.calendar) {
        let found = calendars(&base, found.into_iter().take(1).collect());
        return if found.is_empty() {
            Err(Problem::NoCalendars)
        } else {
            Ok(found)
        };
    }
    let first = found.into_iter().next().map(|(_, p)| p).unwrap_or_default();
    let home = if let Some(home) = first.home {
        follow(&base, &home)?
    } else if let Some(principal) = first.principal {
        let principal = follow(&base, &principal)?;
        let (base, found) = propfind(&principal, login, "0").await?;
        let home = found
            .into_iter()
            .find_map(|(_, p)| p.home)
            .ok_or(Problem::NotCalDav)?;
        follow(&base, &home)?
    } else {
        // Possibly the calendar home itself, such as a Nextcloud or Radicale
        // user folder typed in full.
        base.clone()
    };
    let (base, found) = propfind(&home, login, "1").await?;
    let found = calendars(&base, found);
    if found.is_empty() {
        Err(Problem::NoCalendars)
    } else {
        Ok(found)
    }
}
/// The calendar-query body for one month window, in UTC.
pub(super) fn report_body(month: &str) -> Result<String, Problem> {
    let (from, to) = google::window(month)?;
    let utc = |value: &str| {
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|t| {
                t.with_timezone(&chrono::Utc)
                    .format("%Y%m%dT%H%M%SZ")
                    .to_string()
            })
            .map_err(|_| Problem::InvalidResponse)
    };
    Ok(format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop><c:calendar-data/></d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT">
        <c:time-range start="{}" end="{}"/>
      </c:comp-filter>
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#,
        utc(&from)?,
        utc(&to)?
    ))
}

/// Joins the calendar objects of a REPORT into one VCALENDAR for the parser:
/// each object's components, without its own VCALENDAR lines.
pub(super) fn merge(objects: &[String]) -> Vec<u8> {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for object in objects {
        for line in object.lines() {
            let trimmed = line.trim_end_matches('\r');
            let upper = trimmed.trim().to_ascii_uppercase();
            if upper == "BEGIN:VCALENDAR"
                || upper == "END:VCALENDAR"
                || upper.starts_with("VERSION:")
                || upper.starts_with("PRODID:")
                || upper.starts_with("CALSCALE:")
            {
                continue;
            }
            out.push_str(trimmed);
            out.push_str("\r\n");
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    out.into_bytes()
}

/// Reads one month of a calendar as iCalendar text.
pub(super) async fn month(
    url: &SecretText,
    login: &Login,
    month: &str,
) -> Result<Vec<u8>, Problem> {
    let url = http::parse(&url.0, false)?;
    let reply = http::send(Request {
        method: reqwest::Method::from_bytes(b"REPORT").map_err(|_| Problem::NotCalDav)?,
        url,
        login: Some(login),
        headers: &[("Depth", "1"), ("Accept", "application/xml, text/xml")],
        body: Some(report_body(month)?),
        limit: http::MAX_BYTES,
    })
    .await?;
    status(&reply)?;
    let found = multistatus(&reply.bytes)?;
    let objects: Vec<String> = found.into_iter().filter_map(|(_, p)| p.data).collect();
    let merged = merge(&objects);
    if merged.len() > http::MAX_BYTES {
        return Err(Problem::FeedLimit);
    }
    Ok(merged)
}
