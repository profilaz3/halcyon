use super::*;
use crate::data::{CalendarData, CalendarEvent};
use chrono::{Datelike, Days, Local, NaiveDate, TimeZone};
use reqwest::{Client, Response};
use serde_json::Value;

pub(super) const TOKEN: &str = "https://oauth2.googleapis.com/token";
pub(super) const SCOPE: &str = "openid email https://www.googleapis.com/auth/calendar.events.readonly https://www.googleapis.com/auth/calendar.calendarlist.readonly";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarChoice {
    pub id: String,
    pub name: String,
    pub primary: bool,
    #[serde(default)]
    pub color: String,
}

pub(super) fn http() -> Result<Client, Problem> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Problem::Network)
}
pub(super) async fn json(response: Result<Response, reqwest::Error>) -> Result<Value, Problem> {
    let mut response = response.map_err(|_| Problem::Network)?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > 8 * 1024 * 1024)
    {
        return Err(Problem::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Problem::Network)? {
        if bytes.len().saturating_add(chunk.len()) > 8 * 1024 * 1024 {
            return Err(Problem::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value = serde_json::from_slice(&bytes).map_err(|_| Problem::InvalidResponse)?;
    if !status.is_success() {
        // Never return Google's raw response or a URL containing a token.
        return Err(
            if status.as_u16() == 401 || body["error"] == "invalid_grant" {
                Problem::SignIn
            } else if status.as_u16() == 403
                && body["error"]["errors"].as_array().is_some_and(|errors| {
                    errors
                        .iter()
                        .any(|error| error["reason"] == "insufficientPermissions")
                })
            {
                Problem::Denied
            } else if body["error"] == "invalid_client" || body["error"] == "unauthorized_client" {
                Problem::InvalidClient
            } else {
                Problem::Service
            },
        );
    }
    Ok(body)
}
pub(super) async fn access_token(connection: &Connection, unlock: bool) -> Result<String, Problem> {
    let credential: Credential = keyring::read(&connection.credential, unlock).await?;
    let token = json(
        http()?
            .post(TOKEN)
            .form(&[
                ("client_id", credential.client.client_id.as_str()),
                ("client_secret", credential.client.client_secret.as_str()),
                ("refresh_token", credential.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await,
    )
    .await?;
    token["access_token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(Problem::SignIn)
}
pub(super) async fn calendars(access: &str) -> Result<Vec<CalendarChoice>, Problem> {
    let client = http()?;
    let mut choices = Vec::new();
    let mut token = String::new();
    for _ in 0..20 {
        let body = json(
            client
                .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
                .bearer_auth(access)
                .query(&[
                    ("maxResults", "250"),
                    ("minAccessRole", "reader"),
                    (
                        "fields",
                        "nextPageToken,items(id,summary,summaryOverride,primary,accessRole,backgroundColor)",
                    ),
                    ("pageToken", &token),
                ])
                .send()
                .await,
        )
        .await?;
        for value in body["items"].as_array().ok_or(Problem::InvalidResponse)? {
            if let Some(id) = value["id"].as_str() {
                choices.push(CalendarChoice {
                    id: id.into(),
                    name: value["summaryOverride"]
                        .as_str()
                        .or_else(|| value["summary"].as_str())
                        .unwrap_or("Calendar")
                        .into(),
                    primary: value["primary"].as_bool().unwrap_or(false),
                    color: value["backgroundColor"].as_str().unwrap_or_default().into(),
                });
            }
        }
        match body["nextPageToken"].as_str() {
            Some(next) if !next.is_empty() && next != token => token = next.into(),
            None | Some("") => {
                choices.sort_by(|a, b| b.primary.cmp(&a.primary).then_with(|| a.name.cmp(&b.name)));
                return Ok(choices);
            }
            _ => return Err(Problem::InvalidResponse),
        }
    }
    Err(Problem::InvalidResponse)
}
pub(super) fn window(month: &str) -> Result<(String, String), Problem> {
    if !store::valid_month(month) {
        return Err(Problem::InvalidResponse);
    }
    let first = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
        .map_err(|_| Problem::InvalidResponse)?;
    let start = first - Days::new(first.weekday().num_days_from_monday().into());
    let end = start + Days::new(43);
    let local = |date: NaiveDate| {
        Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|time| time.to_rfc3339())
            .ok_or(Problem::InvalidResponse)
    };
    Ok((local(start)?, local(end)?))
}
pub(super) async fn events(connection: &Connection, month: &str) -> Result<CalendarData, Problem> {
    let access = access_token(connection, false).await?;
    let choices = connection.selected_calendars();
    if choices.len() > 32 {
        return Err(Problem::InvalidResponse);
    }
    let mut result = CalendarData::default();
    for choice in choices {
        let selected = Connection {
            calendar_id: choice.id.clone(),
            ..connection.clone()
        };
        let mut data = calendar_events(&selected, month, &access).await?;
        for event in &mut data.events {
            event.calendar_name = choice.name.clone();
            event.calendar_color = choice.color.clone();
        }
        result.events.extend(data.events);
        if result.events.len() > 20000
            || result.events.iter().map(event_text_bytes).sum::<usize>() > EVENT_TEXT_BUDGET
        {
            return Err(Problem::InvalidResponse);
        }
    }
    result.events.sort_by(CalendarEvent::chronological_order);
    result.account = connection.account.clone();
    result.month = month.into();
    result.updated_at = chrono::Utc::now().to_rfc3339();
    Ok(result)
}
async fn calendar_events(
    connection: &Connection,
    month: &str,
    access: &str,
) -> Result<CalendarData, Problem> {
    let client = http()?;
    let (start, end) = window(month)?;
    let mut url = reqwest::Url::parse("https://www.googleapis.com/calendar/v3/calendars/").unwrap();
    url.path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .push(&connection.calendar_id)
        .push("events");
    let mut events = Vec::new();
    let mut token = String::new();
    for _ in 0..50 {
        let body = json(client.get(url.clone()).bearer_auth(&access).query(&[
            ("singleEvents", "true"), ("orderBy", "startTime"), ("timeMin", &start), ("timeMax", &end),
            ("maxResults", "250"), ("pageToken", &token),
            ("fields", "nextPageToken,items(summary,start,end,status,eventType,attendees(self,responseStatus),htmlLink,hangoutLink,conferenceData(entryPoints(entryPointType,uri)),description,location)"),
        ]).send().await).await?;
        events.extend(
            body["items"]
                .as_array()
                .ok_or(Problem::InvalidResponse)?
                .iter()
                .filter_map(normalize_event),
        );
        if events.iter().map(event_text_bytes).sum::<usize>() > EVENT_TEXT_BUDGET {
            return Err(Problem::InvalidResponse);
        }
        match body["nextPageToken"].as_str() {
            Some(next) if !next.is_empty() && next != token => token = next.into(),
            None | Some("") => {
                return Ok(CalendarData {
                    account: connection.account.clone(),
                    month: month.into(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                    events,
                    connection_revision: connection.revision.clone(),
                    calendar_id: connection.calendar_id.clone(),
                });
            }
            _ => return Err(Problem::InvalidResponse),
        }
    }
    Err(Problem::InvalidResponse)
}
pub(super) fn safe_link(value: &str) -> Option<String> {
    reqwest::Url::parse(value)
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.has_host()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .map(|url| url.into())
}
pub(super) fn meeting_link(event: &Value) -> Option<String> {
    if let Some(entries) = event["conferenceData"]["entryPoints"].as_array() {
        for entry in entries {
            if entry["entryPointType"] == "video" {
                if let Some(url) = entry["uri"].as_str().and_then(safe_link) {
                    return Some(url);
                }
            }
        }
    }
    if let Some(url) = event["hangoutLink"].as_str().and_then(safe_link) {
        return Some(url);
    }
    [event["description"].as_str(), event["location"].as_str()]
        .into_iter()
        .flatten()
        .find_map(crate::data::call_link)
}
fn description_text(input: &str) -> String {
    static TAGS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static BREAKS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static ENTITIES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let bounded: String = input.chars().take(8000).collect();
    let breaks =
        BREAKS.get_or_init(|| regex::Regex::new(r"(?i)<br\s*/?>|</(?:p|div|li)\s*>").unwrap());
    let lines = breaks.replace_all(&bounded, "\n");
    let tags = TAGS.get_or_init(|| regex::Regex::new(r"(?s)<[^>]*>").unwrap());
    let plain = tags.replace_all(&lines, "");
    let entities = ENTITIES.get_or_init(|| {
        regex::Regex::new(r"&(?:amp|lt|gt|quot|apos|nbsp|#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6});")
            .unwrap()
    });
    entities
        .replace_all(&plain, |c: &regex::Captures<'_>| match &c[0] {
            "&amp;" => "&".into(),
            "&lt;" => "<".into(),
            "&gt;" => ">".into(),
            "&quot;" => "\"".into(),
            "&apos;" => "'".into(),
            "&nbsp;" => " ".into(),
            value => {
                let digits = &value[2..value.len() - 1];
                let number = if digits.starts_with(['x', 'X']) {
                    u32::from_str_radix(&digits[1..], 16).ok()
                } else {
                    digits.parse::<u32>().ok()
                };
                number
                    .and_then(char::from_u32)
                    .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            }
        })
        .trim()
        .to_string()
}

pub(super) fn normalize_event(event: &Value) -> Option<CalendarEvent> {
    if event["status"] == "cancelled"
        || matches!(
            event["eventType"].as_str(),
            Some("outOfOffice" | "workingLocation")
        )
        || event["attendees"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["self"] == true && item["responseStatus"] == "declined")
        })
    {
        return None;
    }
    let start = event["start"]["dateTime"]
        .as_str()
        .or_else(|| event["start"]["date"].as_str())?;
    let end = event["end"]["dateTime"]
        .as_str()
        .or_else(|| event["end"]["date"].as_str())?;
    Some(CalendarEvent {
        title: event["summary"]
            .as_str()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Untitled event")
            .into(),
        start: start.into(),
        end: end.into(),
        all_day: event["start"]["date"].is_string(),
        url: event["htmlLink"]
            .as_str()
            .and_then(safe_link)
            .unwrap_or_default(),
        meeting_url: meeting_link(event),
        description: description_text(event["description"].as_str().unwrap_or_default()),
        location: event["location"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(1000)
            .collect(),
        ..Default::default()
    })
}
