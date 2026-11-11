//! The scene model: Depth Anything V2 Small (Apache-2.0), downloaded only
//! when the person asks for it in Settings, from one pinned revision, and
//! checked by size and SHA-256 before anything runs it. It is never bundled.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

/// One downloadable file and what it must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub url: String,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
}

pub const REVISION: &str = "4472b7362082ad9968fee890ca0f1e5aca36b93d";
pub const SIZE: u64 = 99_060_839;
pub const SHA256: &str = "afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c";
/// The file under `~/.local/share/halcyon/models`, named by its revision.
pub const FILE: &str = "depth-anything-v2-small-4472b73.onnx";

/// The pinned model.
pub fn pinned() -> Source {
    Source {
        url: format!(
            "https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/{REVISION}/onnx/model.onnx"
        ),
        size: SIZE,
        sha256: SHA256.into(),
    }
}

pub fn dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|dir| dir.join("models"))
}
pub fn path() -> Option<PathBuf> {
    dir().map(|dir| dir.join(FILE))
}

/// A cheap check for Settings: the file is there at the right size. The
/// scene worker checks the hash every time it loads it.
pub fn installed(path: &Path, source: &Source) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == source.size)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads the model and checks it, reading at most one byte more than it
/// should be.
pub fn read_verified(path: &Path, source: &Source) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| Error::Disk)?;
    let meta = file.metadata().map_err(|_| Error::Disk)?;
    if !meta.is_file() || meta.len() != source.size {
        return Err(Error::Size);
    }
    let mut bytes = Vec::with_capacity(source.size as usize);
    file.take(source.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Disk)?;
    if bytes.len() as u64 != source.size {
        return Err(Error::Size);
    }
    if hex(&Sha256::digest(&bytes)) != source.sha256 {
        return Err(Error::Hash);
    }
    Ok(bytes)
}

/// Why a download did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// No connection, or it stalled.
    Network,
    /// The server answered with something other than the file.
    Status(u16),
    /// Not the expected number of bytes.
    Size,
    /// The right size, but not the expected file.
    Hash,
    /// It could not be saved.
    Disk,
    Cancelled,
}
impl Error {
    pub fn sentence(self) -> &'static str {
        match self {
            Self::Network => {
                "Couldn’t download the scene model. Check that you’re online, then choose Try again."
            }
            Self::Status(_) => "The download server didn’t send the model",
            Self::Size | Self::Hash => "The download wasn’t the expected file, so it was removed",
            Self::Disk => "Couldn’t save the model on this computer",
            Self::Cancelled => "Download cancelled",
        }
    }
}

/// Downloads `source` to `target`: into a `.part` file beside it, checked
/// as it arrives and again at the end, then renamed. Anything else is
/// removed. `progress` hears the bytes received so far.
pub async fn download(
    source: &Source,
    target: &Path,
    mut cancel: watch::Receiver<bool>,
    mut progress: impl FnMut(u64) + Send,
) -> Result<(), Error> {
    use tokio::io::AsyncWriteExt;
    let part = target.with_extension("part");
    let run = async {
        let secure = source.url.starts_with("https://");
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("too many redirects")
                } else if secure && attempt.url().scheme() != "https" {
                    attempt.error("insecure redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| Error::Network)?;
        let mut response = client
            .get(&source.url)
            .send()
            .await
            .map_err(|_| Error::Network)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(Error::Status(response.status().as_u16()));
        }
        if response.content_length().is_some_and(|n| n != source.size) {
            return Err(Error::Size);
        }
        if let Some(dir) = target.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|_| Error::Disk)?;
        }
        let mut file = tokio::fs::File::create(&part)
            .await
            .map_err(|_| Error::Disk)?;
        let mut hash = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Network)? {
            received += chunk.len() as u64;
            if received > source.size {
                return Err(Error::Size);
            }
            hash.update(&chunk);
            file.write_all(&chunk).await.map_err(|_| Error::Disk)?;
            progress(received);
        }
        if received != source.size {
            return Err(Error::Size);
        }
        if hex(&hash.finalize()) != source.sha256 {
            return Err(Error::Hash);
        }
        file.sync_all().await.map_err(|_| Error::Disk)?;
        drop(file);
        tokio::fs::rename(&part, target)
            .await
            .map_err(|_| Error::Disk)
    };
    let result = if *cancel.borrow() {
        Err(Error::Cancelled)
    } else {
        tokio::select! {
            biased;
            _ = cancel.changed() => Err(Error::Cancelled),
            result = run => result,
        }
    };
    if result.is_err() {
        let _ = tokio::fs::remove_file(&part).await;
    }
    result
}

/// Removes the model and every analysis made with it.
pub fn remove(path: &Path, scenes: Option<&Path>) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("part"));
    if let Some(root) = scenes {
        super::cache::clear(root);
    }
}

#[cfg(test)]
mod tests;
