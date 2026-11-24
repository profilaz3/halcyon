//! CalDAV, password feeds, plain HTTP on the local network and calendar
//! files, against a small HTTP server on 127.0.0.1. Nothing here touches the
//! network beyond loopback, the real keyring or the person's settings.
use super::caldav::{self, CalDavCalendar};
use super::http::Login;
use super::subscription::{
    Added, Feed, Origin, SecretText, holiday_feed, parsed, parsed_file, read_source,
};
use super::*;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    host: String,
    authorization: Option<String>,
    depth: Option<String>,
    body: String,
}
struct Answer {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
}
fn answer(status: u16, body: impl Into<String>) -> Answer {
    Answer {
        status,
        headers: vec![],
        body: body.into(),
    }
}
fn moved(location: String) -> Answer {
    Answer {
        status: 301,
        headers: vec![("Location", location)],
        body: String::new(),
    }
}
type Handler = Arc<dyn Fn(&Seen, u16) -> Answer + Send + Sync>;

/// Serves `handler` on 127.0.0.1 until the test ends, one request per
/// connection, and records every request.
async fn serve(handler: Handler) -> (u16, Arc<Mutex<Vec<Seen>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let handler = handler.clone();
            let log = log.clone();
            tokio::spawn(async move {
                let mut buffer = vec![];
                let mut chunk = [0u8; 4096];
                let header_end = loop {
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
                let mut lines = head.split("\r\n");
                let mut first = lines.next().unwrap_or_default().split(' ');
                let method = first.next().unwrap_or_default().to_string();
                let path = first.next().unwrap_or_default().to_string();
                let header = |name: &str| {
                    head.split("\r\n").skip(1).find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case(name)
                            .then(|| value.trim().to_string())
                    })
                };
                let length: usize = header("content-length")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                while buffer.len() < header_end + length {
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                }
                let request = Seen {
                    method,
                    path,
                    host: header("host").unwrap_or_default(),
                    authorization: header("authorization"),
                    depth: header("depth"),
                    body: String::from_utf8_lossy(&buffer[header_end..]).to_string(),
                };
                log.lock().unwrap().push(request.clone());
                let reply = handler(&request, port);
                let mut out = format!(
                    "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
                    reply.status,
                    reply.body.len()
                );
                if reply.status == 207 {
                    out.push_str("Content-Type: application/xml; charset=utf-8\r\n");
                }
                for (key, value) in reply.headers {
                    out.push_str(&format!("{key}: {value}\r\n"));
                }
                out.push_str("\r\n");
                out.push_str(&reply.body);
                let _ = socket.write_all(out.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (port, seen)
}

fn login() -> Login {
    Login {
        username: SecretText("alex".into()),
        password: SecretText("app-password-1234".into()),
    }
}
/// `Basic base64("alex:app-password-1234")`.
fn basic() -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    format!("Basic {}", STANDARD.encode("alex:app-password-1234"))
}
fn multistatus(responses: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:ic="http://apple.com/ns/ical/">{responses}</d:multistatus>"#
    )
}
fn ok(href: &str, props: &str) -> String {
    format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
    )
}
const HOME_LISTING: &str = concat!(
    "<d:response><d:href>/dav/calendars/alex/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
    "<d:response><d:href>/dav/calendars/alex/work/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>Work &amp; travel</d:displayname><ic:calendar-color>#FF2968FF</ic:calendar-color><cal:supported-calendar-component-set><cal:comp name=\"VEVENT\"/></cal:supported-calendar-component-set></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
    "<d:response><d:href>/dav/calendars/alex/tasks/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>Tasks</d:displayname><cal:supported-calendar-component-set><cal:comp name=\"VTODO\"/></cal:supported-calendar-component-set></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
    "<d:response><d:href>/dav/calendars/alex/home%20life/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><cal:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:propstat><d:prop><d:displayname/><ic:calendar-color/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response>",
);

/// A server that answers discovery the way Nextcloud and Radicale do:
/// `/.well-known/caldav` moves to the DAV root, which names the principal,
/// which names the calendar home.
fn account_server(password_ok: bool) -> Handler {
    Arc::new(move |seen: &Seen, port| {
        if seen.authorization.as_deref() != Some(basic().as_str()) || !password_ok {
            return answer(401, "");
        }
        match (seen.method.as_str(), seen.path.as_str()) {
            ("PROPFIND", "/.well-known/caldav") => moved(format!("http://127.0.0.1:{port}/dav/")),
            ("PROPFIND", "/dav/") => answer(
                207,
                multistatus(&ok(
                    "/dav/",
                    "<d:current-user-principal><d:href>/dav/principals/alex/</d:href></d:current-user-principal><d:resourcetype><d:collection/></d:resourcetype>",
                )),
            ),
            ("PROPFIND", "/dav/principals/alex/") => answer(
                207,
                multistatus(&ok(
                    "/dav/principals/alex/",
                    "<cal:calendar-home-set><d:href>/dav/calendars/alex/</d:href></cal:calendar-home-set>",
                )),
            ),
            ("PROPFIND", "/dav/calendars/alex/") if seen.depth.as_deref() == Some("1") => {
                answer(207, multistatus(HOME_LISTING))
            }
            ("PROPFIND", "/dav/calendars/alex/work/") => answer(
                207,
                multistatus(&ok(
                    "/dav/calendars/alex/work/",
                    "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>Work</d:displayname><ic:calendar-color>#00AA55</ic:calendar-color>",
                )),
            ),
            _ => answer(404, ""),
        }
    })
}

#[test]
fn multistatus_reads_names_colors_and_ignores_missing_properties() {
    let found = caldav::multistatus(multistatus(HOME_LISTING).as_bytes()).unwrap();
    assert_eq!(found.len(), 4);
    assert!(!found[0].1.calendar);
    assert_eq!(found[1].1.name.as_deref(), Some("Work & travel"));
    assert_eq!(found[1].1.color.as_deref(), Some("#ff2968"));
    assert_eq!(found[1].1.events, Some(true));
    assert_eq!(found[2].1.events, Some(false));
    assert!(found[3].1.calendar);
    assert_eq!(found[3].1.name, None);
    assert_eq!(found[3].1.color, None);
    // Not a multistatus, and an entity expansion attempt.
    assert_eq!(
        caldav::multistatus(b"<html><body>Hello</body></html>").unwrap_err(),
        Problem::NotCalDav
    );
    let bomb = r#"<?xml version="1.0"?><!DOCTYPE d [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;">]><d:multistatus xmlns:d="DAV:">&b;</d:multistatus>"#;
    assert_eq!(
        caldav::multistatus(bomb.as_bytes()).unwrap_err(),
        Problem::NotCalDav
    );
}

#[tokio::test]
async fn discovery_follows_well_known_principal_and_home() {
    let (port, seen) = serve(account_server(true)).await;
    let found = caldav::discover(&SecretText(format!("http://127.0.0.1:{port}")), &login())
        .await
        .unwrap();
    assert_eq!(
        found,
        vec![
            CalDavCalendar {
                url: SecretText(format!("http://127.0.0.1:{port}/dav/calendars/alex/work/")),
                name: "Work & travel".into(),
                color: "#ff2968".into(),
            },
            CalDavCalendar {
                url: SecretText(format!(
                    "http://127.0.0.1:{port}/dav/calendars/alex/home%20life/"
                )),
                name: "home life".into(),
                color: String::new(),
            },
        ]
    );
    let seen = seen.lock().unwrap();
    let paths: Vec<_> = seen.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/.well-known/caldav",
            "/dav/",
            "/dav/principals/alex/",
            "/dav/calendars/alex/"
        ]
    );
    assert!(seen.iter().all(|s| s.method == "PROPFIND"));
    assert!(
        seen.iter()
            .all(|s| s.authorization.as_deref() == Some(basic().as_str()))
    );
    assert!(seen[0].body.contains("current-user-principal"));
    assert!(seen[0].body.contains("calendar-color"));
    assert_eq!(seen[3].depth.as_deref(), Some("1"));
    // The password never appears in an address or in Debug output.
    assert!(seen.iter().all(|s| !s.path.contains("app-password")));
    assert!(!format!("{found:?}").contains("/dav/calendars"));
}

#[tokio::test]
async fn a_calendar_address_is_accepted_directly() {
    let (port, _) = serve(account_server(true)).await;
    let found = caldav::discover(
        &SecretText(format!("http://127.0.0.1:{port}/dav/calendars/alex/work/")),
        &login(),
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Work");
    assert_eq!(found[0].color, "#00aa55");
}

#[tokio::test]
async fn wrong_passwords_other_servers_and_empty_accounts_say_so() {
    let (port, _) = serve(account_server(false)).await;
    assert_eq!(
        caldav::discover(&SecretText(format!("http://127.0.0.1:{port}")), &login())
            .await
            .unwrap_err(),
        Problem::Credentials
    );
    let (port, _) = serve(Arc::new(|_: &Seen, _| answer(200, "<html>Welcome</html>"))).await;
    assert_eq!(
        caldav::discover(&SecretText(format!("http://127.0.0.1:{port}")), &login())
            .await
            .unwrap_err(),
        Problem::NotCalDav
    );
    let (port, _) = serve(Arc::new(|seen: &Seen, _| {
        if seen.path == "/.well-known/caldav" {
            return answer(404, "");
        }
        answer(
            207,
            multistatus(&ok(
                "/",
                "<cal:calendar-home-set><d:href>/home/</d:href></cal:calendar-home-set>",
            )),
        )
    }))
    .await;
    assert_eq!(
        caldav::discover(&SecretText(format!("http://127.0.0.1:{port}")), &login())
            .await
            .unwrap_err(),
        Problem::NoCalendars
    );
    assert_eq!(
        caldav::discover(&SecretText("http://example.com".into()), &login())
            .await
            .unwrap_err(),
        Problem::PublicHttp
    );
}

#[test]
fn a_reply_cannot_send_the_login_to_another_provider() {
    let found = caldav::multistatus(
        multistatus(&format!(
            "{}{}",
            ok(
                "https://evil.example.net/cal/",
                "<d:resourcetype><cal:calendar/></d:resourcetype>"
            ),
            ok(
                "https://p42-caldav.icloud.com/123/calendars/home/",
                "<d:resourcetype><cal:calendar/></d:resourcetype>"
            ),
        ))
        .as_bytes(),
    )
    .unwrap();
    let base = reqwest::Url::parse("https://caldav.icloud.com/123/calendars/").unwrap();
    let kept = caldav::calendars(&base, found);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        kept[0].url.0,
        "https://p42-caldav.icloud.com/123/calendars/home/"
    );
}

const EVENT_A: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Fixture//EN\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261014T090000Z\r\nDTEND:20261014T100000Z\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const EVENT_B: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE:20261020\r\nDTEND;VALUE=DATE:20261021\r\nSUMMARY:Holiday\r\nRRULE:FREQ=WEEKLY;COUNT=2\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

fn calendar_server() -> Handler {
    Arc::new(|seen: &Seen, _| {
        if seen.authorization.as_deref() != Some(basic().as_str()) {
            return answer(401, "");
        }
        if seen.method != "REPORT" || seen.path != "/dav/calendars/alex/work/" {
            return answer(405, "");
        }
        answer(
            207,
            multistatus(&format!(
                "{}{}<d:response><d:href>/dav/calendars/alex/work/gone.ics</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>",
                ok(
                    "/dav/calendars/alex/work/a.ics",
                    &format!("<cal:calendar-data>{EVENT_A}</cal:calendar-data>")
                ),
                ok(
                    "/dav/calendars/alex/work/b.ics",
                    &format!("<cal:calendar-data><![CDATA[{EVENT_B}]]></cal:calendar-data>")
                ),
            )),
        )
    })
}
fn choice() -> CalendarChoice {
    CalendarChoice {
        id: "work".into(),
        name: "Work".into(),
        primary: false,
        color: "#ff2968".into(),
    }
}

#[tokio::test]
async fn report_asks_for_the_month_and_reads_every_calendar_object() {
    let (port, seen) = serve(calendar_server()).await;
    let feed = Feed {
        choice: choice(),
        url: SecretText(format!("http://127.0.0.1:{port}/dav/calendars/alex/work/")),
        kind: SourceKind::CalDav,
        login: Some(login()),
    };
    let events = parsed(&feed, "2026-10").await.unwrap();
    let titles: Vec<_> = events.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, ["Standup", "Holiday", "Holiday"]);
    assert!(events.iter().all(|e| e.calendar_color == "#ff2968"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "REPORT");
    assert_eq!(seen[0].depth.as_deref(), Some("1"));
    let body = caldav::report_body("2026-10").unwrap();
    assert_eq!(seen[0].body, body);
    assert!(body.contains("<c:comp-filter name=\"VEVENT\">"));
    let (from, to) = google::window("2026-10").unwrap();
    let utc = |v: &str| {
        chrono::DateTime::parse_from_rfc3339(v)
            .unwrap()
            .with_timezone(&chrono::Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
    };
    assert!(body.contains(&format!(
        "<c:time-range start=\"{}\" end=\"{}\"/>",
        utc(&from),
        utc(&to)
    )));
}

#[tokio::test]
async fn a_month_without_events_is_empty_not_an_error() {
    let (port, _) = serve(Arc::new(|_: &Seen, _| answer(207, multistatus("")))).await;
    let feed = Feed {
        choice: choice(),
        url: SecretText(format!("http://127.0.0.1:{port}/cal/")),
        kind: SourceKind::CalDav,
        login: Some(login()),
    };
    assert!(parsed(&feed, "2026-10").await.unwrap().is_empty());
}

#[test]
fn merged_objects_keep_components_inside_one_calendar() {
    let merged = String::from_utf8(caldav::merge(&[EVENT_A.into(), EVENT_B.into()])).unwrap();
    assert_eq!(merged.matches("BEGIN:VCALENDAR").count(), 1);
    assert_eq!(merged.matches("END:VCALENDAR").count(), 1);
    assert_eq!(merged.matches("BEGIN:VEVENT").count(), 2);
    assert!(!merged.contains("PRODID"));
}

#[tokio::test]
async fn outlook_link_keeps_wall_time_and_applies_exceptions() {
    let (port, _) = serve(Arc::new(|request, _| {
        if request.method == "GET" && request.path == "/calendar.ics" {
            answer(200, include_str!("fixtures/outlook-series.ics"))
        } else {
            answer(404, "")
        }
    }))
    .await;
    let feed = Feed {
        kind: SourceKind::Link,
        url: SecretText(format!("http://127.0.0.1:{port}/calendar.ics")),
        choice: choice(),
        login: None,
    };
    let events = parsed(&feed, "2026-03").await.unwrap();
    let times: Vec<_> = events
        .iter()
        .map(|e| (e.start.as_str(), e.end.as_str()))
        .collect();
    assert_eq!(
        times,
        [
            ("2026-03-04T09:00:00-05:00", "2026-03-04T09:30:00-05:00"),
            ("2026-03-11T09:00:00-04:00", "2026-03-11T09:30:00-04:00"),
            ("2026-03-26T11:30:00-04:00", "2026-03-26T12:00:00-04:00"),
            ("2026-03-27T09:00:00-04:00", "2026-03-27T09:30:00-04:00"),
        ]
    );
    assert_eq!(events[2].title, "Rescheduled office hours");
}

#[tokio::test]
async fn password_feeds_send_basic_auth_and_report_a_wrong_password() {
    let handler: Handler = Arc::new(|seen: &Seen, _| {
        if seen.authorization.as_deref() == Some(basic().as_str()) {
            answer(200, EVENT_A)
        } else {
            answer(401, "")
        }
    });
    let (port, seen) = serve(handler).await;
    let mut feed = Feed {
        choice: choice(),
        url: SecretText(format!("http://127.0.0.1:{port}/private.ics")),
        kind: SourceKind::Link,
        login: Some(login()),
    };
    assert_eq!(parsed(&feed, "2026-10").await.unwrap().len(), 1);
    feed.login = Some(Login {
        username: SecretText("alex".into()),
        password: SecretText("wrong".into()),
    });
    assert_eq!(
        parsed(&feed, "2026-10").await.unwrap_err(),
        Problem::Credentials
    );
    assert_eq!(
        Problem::Credentials.message(),
        "The server did not accept the username or password."
    );
    feed.login = None;
    assert_eq!(
        parsed(&feed, "2026-10").await.unwrap_err(),
        Problem::Credentials
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].authorization.as_deref(), Some(basic().as_str()));
    assert_eq!(seen[2].authorization, None);
}

#[tokio::test]
async fn a_redirect_to_another_host_drops_the_login() {
    let handler: Handler = Arc::new(|seen: &Seen, port| {
        if seen.path == "/private.ics" {
            // Same server, new host name: the login must stay behind.
            moved(format!("http://localhost:{port}/moved.ics"))
        } else {
            answer(200, EVENT_A)
        }
    });
    let (port, seen) = serve(handler).await;
    let feed = Feed {
        choice: choice(),
        url: SecretText(format!("http://127.0.0.1:{port}/private.ics")),
        kind: SourceKind::Link,
        login: Some(login()),
    };
    assert_eq!(parsed(&feed, "2026-10").await.unwrap().len(), 1);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].authorization.as_deref(), Some(basic().as_str()));
    assert!(seen[1].host.starts_with("localhost:"));
    assert_eq!(seen[1].authorization, None);
}

#[tokio::test]
async fn a_redirect_on_the_same_host_keeps_the_login() {
    let handler: Handler = Arc::new(|seen: &Seen, port| {
        if seen.path == "/private.ics" {
            moved(format!("http://127.0.0.1:{port}/moved.ics"))
        } else if seen.authorization.as_deref() == Some(basic().as_str()) {
            answer(200, EVENT_A)
        } else {
            answer(401, "")
        }
    });
    let (port, _) = serve(handler).await;
    let feed = Feed {
        choice: choice(),
        url: SecretText(format!("http://127.0.0.1:{port}/private.ics")),
        kind: SourceKind::Link,
        login: Some(login()),
    };
    assert_eq!(parsed(&feed, "2026-10").await.unwrap().len(), 1);
}

#[tokio::test]
async fn redirects_stop_after_five() {
    let handler: Handler = Arc::new(|seen: &Seen, port| {
        let n: u32 = seen.path.trim_start_matches("/r").parse().unwrap_or(0);
        moved(format!("http://127.0.0.1:{port}/r{}", n + 1))
    });
    let (port, seen) = serve(handler).await;
    let feed = Feed {
        choice: choice(),
        url: SecretText(format!("http://127.0.0.1:{port}/r0")),
        kind: SourceKind::Link,
        login: None,
    };
    assert_eq!(
        parsed(&feed, "2026-10").await.unwrap_err(),
        Problem::InvalidFeed
    );
    assert_eq!(seen.lock().unwrap().len(), 6);
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "halcyon-calendar-file-{name}-{}",
        store::nonce().unwrap()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn calendar_files_are_read_with_the_feed_limits() {
    let dir = scratch("read");
    let file = dir.join("team.ics");
    std::fs::write(&file, EVENT_A).unwrap();
    let events = parsed_file(&file, "2026-10", &choice()).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].calendar_name, "Work");
    // Changed on disk: the next read sees it.
    std::fs::write(&file, EVENT_B).unwrap();
    assert_eq!(
        parsed_file(&file, "2026-10", &choice())
            .await
            .unwrap()
            .len(),
        2
    );
    let large = dir.join("large.ics");
    std::fs::write(&large, vec![b'x'; super::http::MAX_BYTES + 1]).unwrap();
    assert_eq!(
        parsed_file(&large, "2026-10", &choice()).await.unwrap_err(),
        Problem::FeedLimit
    );
    let broken = dir.join("broken.ics");
    std::fs::write(&broken, "not a calendar").unwrap();
    assert_eq!(
        parsed_file(&broken, "2026-10", &choice())
            .await
            .unwrap_err(),
        Problem::CalendarFile
    );
    for path in [dir.join("missing.ics"), dir.clone(), "relative.ics".into()] {
        assert_eq!(
            parsed_file(&path, "2026-10", &choice()).await.unwrap_err(),
            Problem::CalendarFile
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn secrets_stay_out_of_debug_and_saved_connections() {
    let connection = Connection {
        backend: Backend::Subscription,
        account: "Calendar subscriptions".into(),
        calendar_id: "work".into(),
        calendar_name: "Work".into(),
        revision: "r".into(),
        credential: "opaque-keyring-id".into(),
        selections: vec![choice()],
        subscriptions: vec![choice()],
        sources: vec![Source {
            id: "work".into(),
            kind: SourceKind::CalDav,
            path: None,
        }],
    };
    let saved = serde_json::to_string(&connection).unwrap();
    assert!(saved.contains("\"cal_dav\""));
    let found = CalDavCalendar {
        url: SecretText("https://caldav.example.com/alex/work/".into()),
        name: "Work".into(),
        color: String::new(),
    };
    for text in [
        saved,
        format!("{connection:?}"),
        format!("{found:?}"),
        format!("{:?}", login()),
    ] {
        for secret in ["caldav.example.com", "app-password-1234", "\"alex\""] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
    }
    // Connections saved before sources existed read as calendar links.
    let old: Connection = serde_json::from_str(
        r#"{"backend":"subscription","account":"Calendar subscriptions","subscriptions":[{"id":"x","name":"X","primary":false}]}"#,
    )
    .unwrap();
    assert_eq!(old.source_kind("x"), SourceKind::Link);
    assert!(old.calendar_files().is_empty());
    assert!(!serde_json::to_string(&old).unwrap().contains("sources"));
    // A keyring entry from before keeps working as a plain link.
    let feed: Feed = serde_json::from_str(
        r#"{"choice":{"id":"x","name":"X","primary":false},"url":"https://example.com/a.ics"}"#,
    )
    .unwrap();
    assert_eq!(feed.kind, SourceKind::Link);
    assert!(feed.login.is_none());
}

/// Adds, reads, watches and removes a calendar file in an isolated home with
/// no keyring at all: files need none.
#[test]
fn calendar_files_join_the_list_without_a_keyring() {
    const CHILD: &str = "HALCYON_CALENDAR_FILE_TEST";
    const NAME: &str = "calendar::caldav_tests::calendar_files_join_the_list_without_a_keyring";
    if std::env::var_os(CHILD).is_none() {
        let root = scratch("isolated");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        crate::test_env::configure(&mut command, &root);
        let result = command
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let root = crate::test_env::assert_isolated();
    assert!(!store::preview());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let file = root.join("family.ics");
            std::fs::write(&file, EVENT_A).unwrap();
            let connection = add_file(store::load_connection(), String::new(), file.clone())
                .await
                .unwrap();
            assert_eq!(connection.backend, Backend::Subscription);
            assert!(
                connection.credential.is_empty(),
                "files need no keyring item"
            );
            assert_eq!(connection.subscriptions[0].name, "family");
            assert_eq!(connection.calendar_files(), vec![file.clone()]);
            assert_eq!(store::load_connection(), connection);
            assert_eq!(
                add_file(connection.clone(), "Again".into(), file.clone())
                    .await
                    .unwrap_err(),
                Problem::Duplicate
            );
            let month = chrono::Local::now().format("%Y-%m").to_string();
            let data = subscription::events(&connection, &month, false)
                .await
                .unwrap();
            assert!(data.events.len() <= 1);
            // Check connection reads the file too.
            assert_eq!(
                calendars(connection.clone(), true).await.unwrap(),
                connection.subscriptions
            );
            let second = root.join("club.ics");
            std::fs::write(&second, EVENT_B).unwrap();
            let both = add_file(connection, "Club".into(), second).await.unwrap();
            assert_eq!(both.subscriptions.len(), 2);
            assert_eq!(both.selected_calendars().len(), 2);
            let id = both.subscriptions[0].id.clone();
            let one = remove_subscription(both, id).await.unwrap();
            assert_eq!(one.subscriptions.len(), 1);
            assert_eq!(one.sources.len(), 1);
            assert_eq!(one.subscriptions[0].name, "Club");
            std::fs::remove_file(&file).unwrap();
            let id = one.subscriptions[0].id.clone();
            let gone = remove_subscription(one, id).await.unwrap();
            assert!(!gone.connected() && !gone.cleanup_pending());
            // A file that can't be read is not added.
            let missing = add_file(gone, String::new(), root.join("missing.ics"))
                .await
                .unwrap_err();
            assert_eq!(missing, Problem::CalendarFile);
        });
}

/// A CalDAV account whose keyring is unavailable saves nothing: no
/// connection file, and no password anywhere on disk.
#[test]
fn caldav_without_a_keyring_saves_nothing() {
    const CHILD: &str = "HALCYON_CALDAV_NO_KEYRING_TEST";
    const NAME: &str = "calendar::caldav_tests::caldav_without_a_keyring_saves_nothing";
    if std::env::var_os(CHILD).is_none() {
        let root = scratch("nokeyring");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        crate::test_env::configure(&mut command, &root);
        let result = command
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let root = crate::test_env::assert_isolated();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (port, _) = serve(calendar_server()).await;
            let found = vec![CalDavCalendar {
                url: SecretText(format!("http://127.0.0.1:{port}/dav/calendars/alex/work/")),
                name: "Work".into(),
                color: "#ff2968".into(),
            }];
            let problem = add_caldav(store::load_connection(), login(), found)
                .await
                .unwrap_err();
            assert_eq!(problem, Problem::Keyring);
            assert!(!format!("{problem:?}").contains("app-password"));
            assert!(!store::account_path().unwrap().exists());
            fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
                for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, out);
                    } else {
                        out.push(path);
                    }
                }
            }
            let mut files = vec![];
            walk(&root, &mut files);
            for file in files {
                let bytes = std::fs::read(&file).unwrap_or_default();
                let text = String::from_utf8_lossy(&bytes);
                assert!(
                    !text.contains("app-password-1234") && !text.contains("/dav/calendars"),
                    "{}",
                    file.display()
                );
            }
        });
}

#[test]
fn added_sources_count_toward_the_eight_calendars() {
    let connection = Connection {
        backend: Backend::Subscription,
        account: "Calendar subscriptions".into(),
        subscriptions: (0..8)
            .map(|i| CalendarChoice {
                id: i.to_string(),
                name: i.to_string(),
                primary: false,
                color: String::new(),
            })
            .collect(),
        ..Default::default()
    };
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(subscription::add(
            connection,
            vec![Added::Path {
                kind: SourceKind::File,
                path: "/tmp/x.ics".into(),
                name: String::new(),
                color: String::new(),
            }],
        ));
    assert_eq!(result.unwrap_err(), Problem::CalendarLimit);
}

/// A folder laid out the way vdirsyncer and khal keep calendars: events in
/// the chosen folder itself, and one subfolder per calendar with its
/// `displayname` and `color` files.
fn vdir() -> std::path::PathBuf {
    let root = scratch("folder");
    std::fs::write(root.join("loose.ics"), EVENT_A).unwrap();
    let work = root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("displayname"), "Work shifts\n").unwrap();
    std::fs::write(work.join("color"), "#FF2968FF\n").unwrap();
    std::fs::write(work.join("a.ics"), EVENT_A).unwrap();
    std::fs::write(work.join("b.ICS"), EVENT_B).unwrap();
    std::fs::write(work.join("notes.txt"), "not a calendar").unwrap();
    // Deeper than one level is not read, nor are hidden folders and files.
    std::fs::create_dir_all(work.join("archive")).unwrap();
    std::fs::write(work.join("archive/old.ics"), EVENT_B).unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::write(root.join(".hidden/x.ics"), EVENT_A).unwrap();
    std::fs::write(work.join(".partial.ics"), "BEGIN:VCALENDAR").unwrap();
    std::fs::create_dir_all(root.join("empty")).unwrap();
    root
}

#[tokio::test]
async fn calendar_folders_are_found_and_read_like_vdirsyncer_keeps_them() {
    let root = vdir();
    let found = folder::discover(&root).unwrap();
    let root_name = root.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        found,
        vec![
            FolderCalendar {
                path: root.clone(),
                name: root_name,
                color: String::new(),
            },
            FolderCalendar {
                path: root.join("work"),
                name: "Work shifts".into(),
                color: "#ff2968".into(),
            },
        ]
    );
    // One event per file, merged into one calendar: one standup and two
    // weekly holidays. The archive below it is not read.
    let events = read_source(
        &SourceKind::Folder,
        Origin::Path(&root.join("work")),
        &choice(),
        "2026-10",
    )
    .await
    .unwrap();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.calendar_name == "Work"));
    let loose = read_source(
        &SourceKind::Folder,
        Origin::Path(&root),
        &choice(),
        "2026-10",
    )
    .await
    .unwrap();
    assert_eq!(loose.len(), 1);
    // A broken file makes the folder a calendar file problem, as for a file.
    std::fs::write(root.join("work/c.ics"), "not a calendar").unwrap();
    assert_eq!(
        read_source(
            &SourceKind::Folder,
            Origin::Path(&root.join("work")),
            &choice(),
            "2026-10",
        )
        .await
        .unwrap_err(),
        Problem::CalendarFile
    );
    assert_eq!(
        folder::discover(&root.join("empty")).unwrap_err(),
        Problem::EmptyFolder
    );
    for path in [
        root.join("missing"),
        "relative".into(),
        root.join("loose.ics"),
    ] {
        assert_eq!(folder::discover(&path).unwrap_err(), Problem::CalendarFile);
        assert_eq!(folder::read(&path).unwrap_err(), Problem::CalendarFile);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn calendar_folders_have_one_size_cap_for_all_their_files() {
    let root = scratch("folder-cap");
    // Each file is under the limit; together they are over it.
    let part = super::http::MAX_BYTES / 4 + 1;
    for index in 0..4 {
        let mut body = EVENT_A.as_bytes().to_vec();
        body.resize(part, b' ');
        std::fs::write(root.join(format!("{index}.ics")), body).unwrap();
    }
    assert_eq!(folder::read(&root).unwrap_err(), Problem::FeedLimit);
    std::fs::remove_file(root.join("3.ics")).unwrap();
    assert!(folder::read(&root).is_ok(), "three quarters fit");
    std::fs::write(root.join("big.ics"), vec![b'x'; super::http::MAX_BYTES + 1]).unwrap();
    assert_eq!(folder::read(&root).unwrap_err(), Problem::FeedLimit);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn holidays_are_a_public_link_in_a_muted_color() {
    let germany = holidays::COUNTRIES
        .iter()
        .position(|(label, _)| *label == "Germany")
        .unwrap();
    let feed = holiday_feed(germany).unwrap();
    assert_eq!(feed.kind, SourceKind::Holidays);
    assert!(feed.kind.uses_keyring(), "kept like any other link");
    assert!(feed.login.is_none());
    assert_eq!(feed.choice.name, "Germany holidays");
    assert_eq!(feed.choice.color, holidays::COLOR);
    assert_eq!(
        feed.url.0,
        "https://www.thunderbird.net/media/caldata/autogen/GermanHolidays.ics"
    );
    assert_eq!(
        holiday_feed(holidays::COUNTRIES.len()).err(),
        Some(Problem::FeedUrl)
    );
    // Saved feeds from before this kind existed still read as links.
    let old: Feed = serde_json::from_str(
        r#"{"choice":{"id":"a","name":"A","primary":false,"color":""},"url":"https://example.com/a.ics"}"#,
    )
    .unwrap();
    assert_eq!(old.kind, SourceKind::Link);
    let saved = serde_json::to_string(&feed).unwrap();
    assert!(saved.contains(r#""kind":"holidays""#), "{saved}");
}

#[tokio::test]
async fn holiday_feeds_are_read_through_the_link_path() {
    let (port, seen) = serve(Arc::new(|seen: &Seen, _| {
        if seen.method == "GET" && seen.path == "/GermanHolidays.ics" {
            answer(200, EVENT_B)
        } else {
            answer(404, "")
        }
    }))
    .await;
    let feed = Feed {
        url: SecretText(format!("http://127.0.0.1:{port}/GermanHolidays.ics")),
        ..holiday_feed(0).unwrap()
    };
    let events = parsed(&feed, "2026-10").await.unwrap();
    assert_eq!(events.len(), 2);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].authorization.is_none(), "no login is sent");
}

#[test]
fn every_source_kind_says_where_it_is_kept() {
    // Kinds kept in the keyring are read through a feed, the rest through
    // a path; the reader refuses a mismatch instead of guessing.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let feed = holiday_feed(0).unwrap();
    for kind in [
        SourceKind::Link,
        SourceKind::CalDav,
        SourceKind::Holidays,
        SourceKind::File,
        SourceKind::Folder,
    ] {
        let wrong = if kind.uses_keyring() {
            rt.block_on(read_source(
                &kind,
                Origin::Path(std::path::Path::new("/tmp")),
                &choice(),
                "2026-10",
            ))
        } else {
            rt.block_on(read_source(
                &kind,
                Origin::Feed(&feed),
                &choice(),
                "2026-10",
            ))
        };
        assert_eq!(wrong.unwrap_err(), Problem::Changed, "{kind:?}");
    }
}

/// Adds, reads and removes calendar folders in an isolated home with no
/// keyring: folders need none, and their names and colors carry over.
#[test]
fn calendar_folders_join_the_list_without_a_keyring() {
    const CHILD: &str = "HALCYON_CALENDAR_FOLDER_TEST";
    const NAME: &str = "calendar::caldav_tests::calendar_folders_join_the_list_without_a_keyring";
    if std::env::var_os(CHILD).is_none() {
        let root = scratch("folder-isolated");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        crate::test_env::configure(&mut command, &root);
        let result = command
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let home = crate::test_env::assert_isolated();
    assert!(!store::preview());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let root = home.join("calendars");
            std::fs::create_dir_all(root.join("work")).unwrap();
            std::fs::create_dir_all(root.join("home")).unwrap();
            std::fs::write(root.join("work/displayname"), "Work").unwrap();
            std::fs::write(root.join("work/color"), "#3a86ff").unwrap();
            std::fs::write(root.join("work/a.ics"), EVENT_A).unwrap();
            std::fs::write(root.join("home/b.ics"), EVENT_B).unwrap();
            let found = discover_folder(&root).unwrap();
            assert_eq!(found.len(), 2);
            let connection = add_folders(store::load_connection(), found.clone())
                .await
                .unwrap();
            assert_eq!(connection.backend, Backend::Subscription);
            assert!(connection.credential.is_empty(), "no keyring item");
            let names: Vec<_> = connection
                .subscriptions
                .iter()
                .map(|c| (c.name.as_str(), c.color.as_str()))
                .collect();
            assert_eq!(names, vec![("home", ""), ("Work", "#3a86ff")]);
            assert_eq!(
                connection.calendar_folders(),
                vec![root.join("home"), root.join("work")]
            );
            assert!(connection.calendar_files().is_empty());
            assert_eq!(store::load_connection(), connection);
            assert_eq!(
                add_folders(connection.clone(), found[..1].to_vec())
                    .await
                    .unwrap_err(),
                Problem::Duplicate
            );
            assert_eq!(
                calendars(connection.clone(), true).await.unwrap(),
                connection.subscriptions
            );
            let data = subscription::events(&connection, "2026-10", false)
                .await
                .unwrap();
            assert_eq!(data.events.len(), 3);
            let id = connection.subscriptions[0].id.clone();
            let one = remove_subscription(connection, id).await.unwrap();
            assert_eq!(one.calendar_folders(), vec![root.join("work")]);
            assert!(one.credential.is_empty());
            // Nothing picked adds nothing and changes nothing.
            assert_eq!(
                add_folders(one.clone(), vec![]).await.unwrap_err(),
                Problem::EmptyFolder
            );
            assert_eq!(store::load_connection(), one);
        });
}
