//! The Pling (OpenDesktop OCS v1) icon theme catalog: category 132, "Full
//! Icon Themes". No key is needed. Responses are bounded and parsed
//! leniently: numbers may arrive as strings, and any item without a free
//! archive download is left out.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const ENDPOINT: &str = "https://api.pling.com/ocs/v1/content/data";
pub const CATEGORY: u32 = 132;
pub const PAGE_SIZE: u32 = 20;
/// A catalog page is about 10 KB per item; descriptions are long.
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const MAX_PREVIEW: usize = 8 * 1024 * 1024;
/// Thumbnails are resized to this width before they are cached.
pub const THUMB_WIDTH: u32 = 560;
/// The thumbnail cache is trimmed, oldest first, to this size.
pub const CACHE_LIMIT: u64 = 48 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    #[default]
    Popular,
    Newest,
    TopRated,
}

impl Sort {
    pub const ALL: [Self; 3] = [Self::Popular, Self::Newest, Self::TopRated];
    pub fn title(self) -> &'static str {
        match self {
            Self::Popular => "Popular",
            Self::Newest => "Newest",
            Self::TopRated => "Top rated",
        }
    }
    /// OCS `sortmode`.
    pub fn mode(self) -> &'static str {
        match self {
            Self::Popular => "down",
            Self::Newest => "new",
            Self::TopRated => "high",
        }
    }
}

/// One downloadable archive of a pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackFile {
    pub name: String,
    pub link: String,
    /// Size in bytes, as the catalog states it (it lists kilobytes).
    pub size: u64,
}

impl PackFile {
    /// The file name without its archive extension or a leading `01-`
    /// ordering prefix: "Tela-black" for `Tela-black.tar.xz`.
    pub fn variant(&self) -> String {
        let mut name = self.name.as_str();
        for suffix in [
            ".tar.gz", ".tar.xz", ".tar.bz2", ".tgz", ".txz", ".zip", ".tar",
        ] {
            if name.len() > suffix.len() && name.to_ascii_lowercase().ends_with(suffix) {
                name = &name[..name.len() - suffix.len()];
                break;
            }
        }
        let name = match name.split_once('-') {
            Some((prefix, rest))
                if !rest.is_empty()
                    && prefix.len() <= 2
                    && prefix.chars().all(|c| c.is_ascii_digit()) =>
            {
                rest
            }
            _ => name,
        };
        name.to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub id: u64,
    pub name: String,
    pub author: String,
    pub summary: String,
    pub downloads: u64,
    /// 0 to 100.
    pub score: u32,
    /// `YYYY-MM-DD` of the last update.
    pub changed: String,
    pub page: String,
    pub preview: Option<String>,
    pub license: Option<&'static str>,
    pub files: Vec<PackFile>,
}

impl Pack {
    /// "2.8M downloads".
    pub fn downloads_title(&self) -> String {
        let count = self.downloads as f64;
        let short = if count >= 1_000_000. {
            format!("{:.1}M", count / 1_000_000.)
        } else if count >= 1_000. {
            format!("{:.0}K", count / 1_000.)
        } else {
            self.downloads.to_string()
        };
        format!("{} downloads", short.replace(".0M", "M"))
    }
    /// "4.6 ★" on a five point scale.
    pub fn rating_title(&self) -> String {
        format!("{:.1} ★", f64::from(self.score.min(100)) / 20.)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogPage {
    pub total: u32,
    pub page: u32,
    pub packs: Vec<Pack>,
}

impl CatalogPage {
    pub fn more(&self, per_page: u32) -> bool {
        (self.page + 1) * per_page < self.total
    }
}

/// The well-known packs shown first: (title, search, author).
pub const CURATED: [(&str, &str, &str); 6] = [
    ("Papirus", "papirus", "x-varlesh-x"),
    ("Tela", "tela", "vinceliuice"),
    ("Colloid", "colloid", "vinceliuice"),
    ("Fluent", "fluent", "vinceliuice"),
    ("WhiteSur", "whitesur", "vinceliuice"),
    ("Qogir", "qogir", "vinceliuice"),
];

/// The query for one catalog page.
pub fn query(sort: Sort, search: &str, page: u32, size: u32) -> Vec<(&'static str, String)> {
    let mut query = vec![
        ("categories", CATEGORY.to_string()),
        ("format", "json".into()),
        ("pagesize", size.to_string()),
        ("page", page.to_string()),
        ("sortmode", sort.mode().into()),
    ];
    let search = search.trim();
    if !search.is_empty() {
        query.push(("search", search.chars().take(80).collect()));
    }
    query
}

fn number(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(number)) => number.as_u64().unwrap_or_default(),
        Some(Value::String(text)) => text.trim().parse().unwrap_or_default(),
        _ => 0,
    }
}
fn string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

/// Plain text from catalog HTML: tags dropped, common entities decoded,
/// whitespace collapsed and control characters removed.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&hearts;", "♥")
        .replace("&amp;", "&");
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

/// At most `max` characters, cut at a word with an ellipsis.
pub fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':', '-', ' ']))
}

/// Whether a download name looks like an archive Halcyon can open. The
/// format is checked again from the bytes when installing.
pub fn archive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [".tar.gz", ".tgz", ".tar.xz", ".txz", ".zip", ".tar"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
        // Some uploads are renamed `Theme.tar-20261020003513.gz`.
        || (name.contains(".tar") && name.ends_with(".gz"))
}

fn license(tags: &str) -> Option<&'static str> {
    const KNOWN: [(&str, &str); 9] = [
        ("gplv3", "GPL-3.0"),
        ("gplv2", "GPL-2.0"),
        ("lgplv3", "LGPL-3.0"),
        ("lgplv2", "LGPL-2.1"),
        ("cc-by-sa", "CC BY-SA"),
        ("cc-by", "CC BY"),
        ("mit", "MIT"),
        ("apache", "Apache-2.0"),
        ("bsd", "BSD"),
    ];
    let tags: Vec<&str> = tags.split(',').map(str::trim).collect();
    KNOWN
        .iter()
        .find(|(tag, _)| tags.contains(tag))
        .map(|(_, title)| *title)
}

fn https(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    (url.scheme() == "https" && url.has_host() && url.username().is_empty()).then(|| url.into())
}

fn parse_item(item: &Value) -> Option<Pack> {
    if item
        .get("xdg_type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "icons")
    {
        return None;
    }
    let id = number(item.get("id"));
    let name = plain(&string(item.get("name")));
    if id == 0 || name.is_empty() {
        return None;
    }
    let mut files = Vec::new();
    for index in 1..=32 {
        let Some(link) = item.get(format!("downloadlink{index}")) else {
            break;
        };
        let file_name = string(item.get(format!("downloadname{index}")));
        let price = string(item.get(format!("downloadprice{index}")));
        let free = price.is_empty() || price.parse::<f64>().is_ok_and(|price| price == 0.);
        let Some(link) = link.as_str().and_then(https) else {
            continue;
        };
        if free && archive_name(&file_name) && !file_name.contains(['/', '\\']) {
            files.push(PackFile {
                name: file_name.chars().filter(|c| !c.is_control()).collect(),
                link,
                size: number(item.get(format!("downloadsize{index}"))) * 1024,
            });
        }
    }
    if files.is_empty() {
        return None;
    }
    let preview = ["smallpreviewpic1", "previewpic1"]
        .into_iter()
        .find_map(|key| item.get(key).and_then(Value::as_str).and_then(https));
    Some(Pack {
        id,
        name: shorten(&name, 60),
        author: plain(&string(item.get("personid"))),
        summary: shorten(&plain(&string(item.get("summary"))), 140),
        downloads: number(item.get("downloads")),
        score: number(item.get("score")).min(100) as u32,
        changed: string(item.get("changed")).chars().take(10).collect(),
        page: item
            .get("detailpage")
            .and_then(Value::as_str)
            .and_then(https)
            .unwrap_or_default(),
        preview,
        license: license(&string(item.get("tags"))),
        files,
    })
}

/// Parses one OCS JSON response.
pub fn parse(bytes: &[u8], page: u32) -> Result<CatalogPage, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| "The catalog sent an unreadable answer")?;
    if value.get("status").and_then(Value::as_str) != Some("ok") {
        let message = plain(&string(value.get("message")));
        return Err(if message.is_empty() {
            "The catalog refused the request".into()
        } else {
            format!("The catalog refused the request: {}", shorten(&message, 80))
        });
    }
    let packs = value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_item).collect())
        .unwrap_or_default();
    Ok(CatalogPage {
        total: number(value.get("totalitems")).min(u64::from(u32::MAX)) as u32,
        page,
        packs,
    })
}

/// The curated pack named `author` from a search for it.
pub fn pick_curated(page: &CatalogPage, title: &str, author: &str) -> Option<Pack> {
    let title = title.to_lowercase();
    page.packs
        .iter()
        .filter(|pack| pack.author.eq_ignore_ascii_case(author))
        .find(|pack| pack.name.to_lowercase().starts_with(&title))
        .or_else(|| {
            page.packs
                .iter()
                .find(|pack| pack.author.eq_ignore_ascii_case(author))
        })
        .cloned()
}

pub fn client() -> Result<reqwest::Client, String> {
    client_for(Duration::from_secs(45))
}

/// An HTTPS-only client that gives up after `total`.
pub fn client_for(total: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .timeout(total)
        .user_agent(concat!("Halcyon/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("insecure redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| error.to_string())
}

async fn bounded(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut response = response
        .error_for_status()
        .map_err(|error| describe(&error))?;
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return Err("The answer is too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| describe(&error))? {
        if chunk.len() > limit - bytes.len() {
            return Err("The answer is too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// A short reason for a failed request.
pub fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "The catalog took too long to answer".into()
    } else if error.is_connect() {
        "Can’t reach the catalog. Check your connection.".into()
    } else if let Some(status) = error.status() {
        format!("The catalog answered with an error ({})", status.as_u16())
    } else {
        "The connection to the catalog failed".into()
    }
}

pub async fn fetch(sort: Sort, search: String, page: u32) -> Result<CatalogPage, String> {
    let client = client()?;
    let response = client
        .get(ENDPOINT)
        .query(&query(sort, &search, page, PAGE_SIZE))
        .send()
        .await
        .map_err(|error| describe(&error))?;
    parse(&bounded(response, MAX_RESPONSE).await?, page)
}

/// One pack with fresh download links (they are signed and expire).
pub async fn fetch_pack(id: u64) -> Result<Pack, String> {
    let client = client()?;
    let response = client
        .get(format!("{ENDPOINT}/{id}"))
        .query(&[("format", "json")])
        .send()
        .await
        .map_err(|error| describe(&error))?;
    parse(&bounded(response, MAX_RESPONSE).await?, 0)?
        .packs
        .into_iter()
        .find(|pack| pack.id == id)
        .ok_or_else(|| "This pack is no longer offered".to_string())
}

/// The curated packs, in their order; one that can't be found is left out.
pub async fn fetch_curated() -> Result<Vec<Pack>, String> {
    let client = client()?;
    let mut packs = Vec::new();
    let mut failure = None;
    for (title, search, author) in CURATED {
        let request = client
            .get(ENDPOINT)
            .query(&query(Sort::Popular, search, 0, 10))
            .send()
            .await;
        let page = match request {
            Ok(response) => bounded(response, MAX_RESPONSE)
                .await
                .and_then(|bytes| parse(&bytes, 0)),
            Err(error) => Err(describe(&error)),
        };
        match page {
            Ok(page) => packs.extend(pick_curated(&page, title, author)),
            Err(error) => failure = Some(error),
        }
    }
    match failure {
        Some(error) if packs.is_empty() => Err(error),
        _ => Ok(packs),
    }
}

/// Where the thumbnail for `url` is cached.
pub fn thumbnail_path(cache: &Path, url: &str) -> PathBuf {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(url.as_bytes());
    let name: String = digest[..12].iter().map(|b| format!("{b:02x}")).collect();
    cache.join(format!("{name}.png"))
}

/// Decodes a preview, scales it to `THUMB_WIDTH` and encodes it as PNG.
pub fn make_thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode().map_err(|error| error.to_string())?;
    let picture = if picture.width() > THUMB_WIDTH {
        let height = (u64::from(picture.height()) * u64::from(THUMB_WIDTH)
            / u64::from(picture.width().max(1)))
        .max(1) as u32;
        picture.resize_exact(THUMB_WIDTH, height, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    let mut out = Vec::new();
    picture
        .to_rgba8()
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(out)
}

/// Removes the oldest cached thumbnails until the cache fits `limit`.
pub fn trim_cache(cache: &Path, limit: u64) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_file().then(|| {
                (
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    meta.len(),
                    entry.path(),
                )
            })
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// The cached thumbnail for `url`, downloading it first if needed.
pub async fn thumbnail(cache: PathBuf, url: String) -> Result<PathBuf, String> {
    let path = thumbnail_path(&cache, &url);
    if path.is_file() {
        return Ok(path);
    }
    let client = client()?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| describe(&error))?;
    let bytes = bounded(response, MAX_PREVIEW).await?;
    let path2 = path.clone();
    tokio::task::spawn_blocking(move || {
        let png = make_thumbnail(&bytes)?;
        super::write_atomic(&path2, &png).map_err(|error| error.to_string())?;
        trim_cache(&cache, CACHE_LIMIT);
        Ok::<_, String>(())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(path)
}
