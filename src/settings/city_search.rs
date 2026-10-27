use super::City;
use cosmic::iced::futures::future::{AbortHandle, AbortRegistration, Abortable};
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

const ENDPOINT: &str = "https://geocoding-api.open-meteo.com/v1/search";
const MAX_BYTES: usize = 64 * 1024;
const DEADLINE: Duration = Duration::from_secs(20);
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

#[derive(Debug, Default)]
pub(super) struct CitySearch {
    pub query: String,
    pub generation: u64,
    pub deadline: Option<Instant>,
    pub searching: bool,
    pub searched: bool,
    pub error: Option<String>,
    cancellation: Option<AbortHandle>,
}

impl CitySearch {
    pub fn edit(&mut self, query: String, now: Instant) {
        self.cancel();
        self.query = query;
        self.generation = self.generation.wrapping_add(1);
        self.searching = false;
        self.searched = false;
        self.error = None;
        self.deadline =
            (self.query.trim().chars().count() >= 2).then_some(now + Duration::from_millis(350));
    }

    pub fn begin(&mut self, generation: u64) -> Option<(String, AbortRegistration)> {
        if generation != self.generation || self.searching || self.query.trim().chars().count() < 2
        {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.deadline = None;
        self.searching = true;
        self.searched = false;
        self.error = None;
        self.cancel();
        let (abort, registration) = AbortHandle::new_pair();
        self.cancellation = Some(abort);
        Some((self.query.trim().to_string(), registration))
    }

    pub fn finish(&mut self, generation: u64, error: Option<String>) -> bool {
        if generation != self.generation || !self.searching {
            return false;
        }
        self.cancellation = None;
        self.searching = false;
        self.searched = error.is_none();
        self.error = error;
        true
    }

    fn cancel(&mut self) {
        if let Some(abort) = self.cancellation.take() {
            abort.abort();
        }
    }
    pub fn select(&mut self, name: String) {
        self.edit(name, Instant::now());
        self.deadline = None;
    }
}

impl Drop for CitySearch {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(super) async fn search(
    query: String,
    registration: AbortRegistration,
) -> Result<Vec<City>, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    let client = CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(DEADLINE)
                .build()
                .map_err(|_| "City search is unavailable.".into())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    search_from(client, ENDPOINT, query, registration).await
}

async fn search_from(
    client: &reqwest::Client,
    endpoint: &str,
    query: String,
    registration: AbortRegistration,
) -> Result<Vec<City>, String> {
    let operation = async {
        let _slot = SLOTS
            .acquire()
            .await
            .map_err(|_| "City search is unavailable.".to_string())?;
        fetch(client, endpoint, query).await
    };
    Abortable::new(tokio::time::timeout(DEADLINE, operation), registration)
        .await
        .map_err(|_| "Search cancelled.".to_string())?
        .map_err(|_| "City search timed out. Try again.".to_string())?
}

async fn fetch(
    client: &reqwest::Client,
    endpoint: &str,
    query: String,
) -> Result<Vec<City>, String> {
    if query.chars().count() > 256 {
        return Err("City name is too long.".into());
    }
    let mut response = client
        .get(endpoint)
        .query(&[
            ("name", query.as_str()),
            ("count", "10"),
            ("language", "en"),
            ("format", "json"),
        ])
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| {
            "City search is unavailable. Check your connection and try again.".to_string()
        })?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err("City search response is too large.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read the city search results".to_string())?
    {
        if chunk.len() > MAX_BYTES - bytes.len() {
            return Err("City search response is too large.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    cities(&bytes)
}

fn cities(bytes: &[u8]) -> Result<Vec<City>, String> {
    #[derive(serde::Deserialize)]
    struct Response {
        #[serde(default)]
        results: Vec<City>,
    }
    let response: Response = serde_json::from_slice(bytes)
        .map_err(|_| "Could not read the city search results".to_string())?;
    Ok(response
        .results
        .into_iter()
        .filter(|city| {
            !city.name.trim().is_empty()
                && city.name.len() <= 256
                && city.admin1.len() <= 256
                && city.country.len() <= 256
                && city.timezone.as_ref().is_none_or(|zone| zone.len() <= 128)
                && city.latitude.is_finite()
                && (-90.0..=90.0).contains(&city.latitude)
                && city.longitude.is_finite()
                && (-180.0..=180.0).contains(&city.longitude)
        })
        .take(10)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn request(socket: &mut tokio::net::TcpStream) {
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !bytes.ends_with(b"\r\n\r\n") {
                assert!(bytes.len() < 8192);
                bytes.push(socket.read_u8().await.unwrap());
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn editing_closes_the_obsolete_http_request_and_preserves_new_generation() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (started, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            started.send(()).unwrap();
            let mut byte = [0];
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(3), socket.read(&mut byte))
                    .await
                    .unwrap()
                    .unwrap(),
                0
            );
        });
        let mut search = CitySearch::default();
        search.edit("Zagreb".into(), Instant::now());
        let (query, registration) = search.begin(search.generation).unwrap();
        let old = search.generation;
        let job = tokio::spawn(async move {
            search_from(&reqwest::Client::new(), &endpoint, query, registration).await
        });
        ready.await.unwrap();
        search.edit("Paris".into(), Instant::now());
        let _next = search.begin(search.generation).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(3), job)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_err());
        assert!(!search.finish(old, result.err()));
        assert!(search.searching);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn selection_drop_and_waiting_for_a_slot_cancel_without_network() {
        let permits = SLOTS.acquire_many(2).await.unwrap();
        for select in [true, false] {
            let mut search = CitySearch::default();
            search.edit("Zagreb".into(), Instant::now());
            let (query, registration) = search.begin(search.generation).unwrap();
            let task = tokio::spawn(async move {
                search_from(
                    &reqwest::Client::new(),
                    "http://127.0.0.1:1",
                    query,
                    registration,
                )
                .await
            });
            if select {
                search.select("Paris".into());
            } else {
                drop(search);
            }
            let error = tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert_eq!(error, "Search cancelled.");
        }
        drop(permits);
        assert_eq!(SLOTS.available_permits(), 2);
    }

    #[tokio::test]
    async fn response_limit_applies_with_and_without_content_length() {
        for advertised in [true, false] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                request(&mut socket).await;
                let length = if advertised {
                    format!("Content-Length: {}\r\n", MAX_BYTES + 1)
                } else {
                    String::new()
                };
                socket
                    .write_all(
                        format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{length}\r\n").as_bytes(),
                    )
                    .await
                    .unwrap();
                let _ = socket.write_all(&vec![b' '; MAX_BYTES + 1]).await;
            });
            let (abort, registration) = AbortHandle::new_pair();
            let result = search_from(
                &reqwest::Client::new(),
                &endpoint,
                "Paris".into(),
                registration,
            )
            .await;
            assert_eq!(result.unwrap_err(), "City search response is too large.");
            drop(abort);
            server.await.unwrap();
        }
    }

    #[test]
    fn returned_cities_have_bounded_text_count_and_valid_coordinates() {
        let valid = serde_json::json!({"name":"Zagreb", "latitude":45.8, "longitude":16.0});
        let mut rows = vec![
            serde_json::json!({"name":"", "latitude":45.8, "longitude":16.0}),
            serde_json::json!({"name":"a".repeat(257),"latitude":0,"longitude":0}),
            serde_json::json!({"name":"Bad coordinate","latitude":91,"longitude":0}),
        ];
        rows.extend(std::iter::repeat_n(valid, 20));
        let result =
            cities(&serde_json::to_vec(&serde_json::json!({"results":rows})).unwrap()).unwrap();
        assert_eq!(result.len(), 10);
        assert!(result.iter().all(|city| city.name == "Zagreb"));
        assert!(cities(b"not json").is_err());
    }

    #[test]
    fn typing_waits_and_only_the_latest_query_can_start() {
        let now = Instant::now();
        let mut search = CitySearch::default();
        search.edit("Za".into(), now);
        let first = search.generation;
        assert!(search.deadline.is_some_and(|deadline| deadline > now));
        search.edit("  Zagreb  ".into(), now + Duration::from_millis(100));
        assert!(search.begin(first).is_none());
        assert_eq!(
            search
                .begin(search.generation)
                .map(|request| request.0)
                .as_deref(),
            Some("Zagreb")
        );
        assert!(search.searching);
        assert!(search.deadline.is_none());
        assert!(search.begin(search.generation).is_none());
    }

    #[test]
    fn short_queries_cancel_pending_search_and_ignore_old_results() {
        let mut search = CitySearch::default();
        search.edit("Paris".into(), Instant::now());
        let old = search.generation;
        assert_eq!(
            search.begin(old).map(|request| request.0).as_deref(),
            Some("Paris")
        );
        let old = search.generation;
        search.edit("P".into(), Instant::now());
        assert!(search.deadline.is_none());
        assert!(!search.searching);
        assert!(!search.finish(old, None));
        assert!(!search.searched);
        assert!(search.begin(search.generation).is_none());
    }

    #[test]
    fn failed_search_can_be_retried_without_accepting_the_old_response() {
        let mut search = CitySearch::default();
        search.edit("Paris".into(), Instant::now());
        assert_eq!(
            search
                .begin(search.generation)
                .map(|request| request.0)
                .as_deref(),
            Some("Paris")
        );
        let old = search.generation;
        assert!(search.finish(old, Some("Offline".into())));
        assert_eq!(search.error.as_deref(), Some("Offline"));
        assert!(!search.searched);
        assert_eq!(
            search
                .begin(search.generation)
                .map(|request| request.0)
                .as_deref(),
            Some("Paris")
        );
        assert_ne!(search.generation, old);
        assert!(!search.finish(old, None));
        assert!(search.finish(search.generation, None));
        assert!(search.searched);
        assert!(search.error.is_none());
    }

    #[test]
    fn selecting_a_city_invalidates_pending_results() {
        let mut search = CitySearch::default();
        search.edit("Spring".into(), Instant::now());
        assert_eq!(
            search
                .begin(search.generation)
                .map(|request| request.0)
                .as_deref(),
            Some("Spring")
        );
        let request = search.generation;
        search.select("Springfield".into());
        assert_eq!(search.query, "Springfield");
        assert!(!search.searching && !search.searched);
        assert!(search.deadline.is_none());
        assert!(!search.finish(request, None));
    }
}
