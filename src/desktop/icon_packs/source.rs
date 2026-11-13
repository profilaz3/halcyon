//! Where the pack catalog comes from: the live OCS API, or a saved catalog
//! directory that never touches the network (tests and screenshots).
//!
//! A fixture directory holds `catalog.json` (an OCS answer), `thumbs/` with
//! each preview under the last part of its URL, and `files/` with each
//! archive under its download name.
use super::install::Progress;
use super::ocs::{self, CatalogPage, Pack, Sort};
use std::path::{Path, PathBuf};

/// Names a fixture catalog directory.
pub const FIXTURE_ENV: &str = "HALCYON_ICON_PACKS_FIXTURE";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Live,
    Fixture(PathBuf),
    /// Every request fails as if offline. Tests without a fixture use it,
    /// so no test reaches the network.
    Offline,
}

const OFFLINE: &str = "Can’t reach the catalog. Check your connection.";

/// The last path segment of `url`, when it is a plain file name.
fn file_name(url: &str) -> Option<String> {
    let path = reqwest::Url::parse(url).ok()?;
    let name = path.path_segments()?.next_back()?.to_string();
    (!name.is_empty() && !name.starts_with('.') && !name.contains('\\')).then_some(name)
}

impl Source {
    pub fn from_env() -> Self {
        match std::env::var_os(FIXTURE_ENV) {
            Some(dir) if !dir.is_empty() => Self::Fixture(PathBuf::from(dir)),
            _ if cfg!(test) => Self::Offline,
            _ => Self::Live,
        }
    }

    fn catalog(dir: &Path) -> Result<Vec<Pack>, String> {
        let bytes = super::read(&dir.join("catalog.json"), 8 * 1024 * 1024)
            .ok_or_else(|| OFFLINE.to_string())?;
        Ok(ocs::parse(&bytes, 0)?.packs)
    }

    /// One page of packs, sorted and filtered by `search`.
    pub async fn page(&self, sort: Sort, search: String, page: u32) -> Result<CatalogPage, String> {
        match self {
            Self::Live => ocs::fetch(sort, search, page).await,
            Self::Offline => Err(OFFLINE.into()),
            Self::Fixture(dir) => {
                let query = search.trim().to_lowercase();
                let mut packs: Vec<Pack> = Self::catalog(dir)?
                    .into_iter()
                    .filter(|pack| {
                        query.is_empty()
                            || [&pack.name, &pack.author, &pack.summary]
                                .iter()
                                .any(|value| value.to_lowercase().contains(&query))
                    })
                    .collect();
                match sort {
                    Sort::Popular => packs.sort_by(|a, b| b.downloads.cmp(&a.downloads)),
                    Sort::Newest => packs.sort_by(|a, b| b.changed.cmp(&a.changed)),
                    Sort::TopRated => packs.sort_by(|a, b| b.score.cmp(&a.score)),
                }
                let total = packs.len() as u32;
                let start = (page * ocs::PAGE_SIZE) as usize;
                Ok(CatalogPage {
                    total,
                    page,
                    packs: packs
                        .into_iter()
                        .skip(start)
                        .take(ocs::PAGE_SIZE as usize)
                        .collect(),
                })
            }
        }
    }

    /// The curated packs, in their order.
    pub async fn curated(&self) -> Result<Vec<Pack>, String> {
        match self {
            Self::Live => ocs::fetch_curated().await,
            Self::Offline => Err(OFFLINE.into()),
            Self::Fixture(dir) => {
                let packs = Self::catalog(dir)?;
                Ok(ocs::CURATED
                    .iter()
                    .filter_map(|(title, search, author)| {
                        let page = CatalogPage {
                            total: 0,
                            page: 0,
                            packs: packs
                                .iter()
                                .filter(|pack| pack.name.to_lowercase().contains(search))
                                .cloned()
                                .collect(),
                        };
                        ocs::pick_curated(&page, title, author)
                    })
                    .collect())
            }
        }
    }

    /// The pack with fresh download links.
    pub async fn pack(&self, id: u64) -> Result<Pack, String> {
        match self {
            Self::Live => ocs::fetch_pack(id).await,
            Self::Offline => Err(OFFLINE.into()),
            Self::Fixture(dir) => Self::catalog(dir)?
                .into_iter()
                .find(|pack| pack.id == id)
                .ok_or_else(|| "This pack is no longer offered".to_string()),
        }
    }

    /// The cached thumbnail for `url`.
    pub async fn thumbnail(&self, cache: PathBuf, url: String) -> Result<PathBuf, String> {
        match self {
            Self::Live => ocs::thumbnail(cache, url).await,
            Self::Offline => Err(OFFLINE.into()),
            Self::Fixture(dir) => {
                let path = ocs::thumbnail_path(&cache, &url);
                if path.is_file() {
                    return Ok(path);
                }
                let name = file_name(&url).ok_or("No preview")?;
                let bytes = super::read(&dir.join("thumbs").join(name), 8 * 1024 * 1024)
                    .ok_or("No preview")?;
                let png = ocs::make_thumbnail(&bytes)?;
                super::write_atomic(&path, &png).map_err(|error| error.to_string())?;
                ocs::trim_cache(&cache, ocs::CACHE_LIMIT);
                Ok(path)
            }
        }
    }

    /// Downloads the archive at `url` to `dest`, at most `limit` bytes.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        limit: u64,
        mut progress: impl FnMut(Progress),
    ) -> Result<(), String> {
        match self {
            Self::Live => super::install::download(url, dest, limit, progress).await,
            Self::Offline => Err(OFFLINE.into()),
            Self::Fixture(dir) => {
                let name = file_name(url).ok_or("The download link isn’t usable")?;
                let source = dir.join("files").join(name);
                let total = std::fs::metadata(&source)
                    .map_err(|_| OFFLINE.to_string())?
                    .len();
                if total > limit {
                    return Err(format!(
                        "The pack is larger than {} MB",
                        limit / 1024 / 1024
                    ));
                }
                // Reported in steps, as a real download would.
                for step in 0..=4u64 {
                    progress(Progress {
                        done: total * step / 4,
                        total: Some(total),
                    });
                    tokio::task::yield_now().await;
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::copy(&source, dest)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
        }
    }
}
