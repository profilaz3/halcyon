//! Bounded, declarative setup transport. Imported assets never name filesystem paths.
use super::{Profile, ProfileError, SourceLook};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
pub(crate) mod assets;
#[cfg(test)]
mod tests;

pub const FORMAT: &str = "halcyon-setup-package";
pub const MAX_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_IMAGE: usize = 16 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const ASSET_DIR: &str = "setup-assets";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credits {
    pub author: String,
    pub license: String,
    pub source: String,
}
impl Credits {
    pub fn validate(&self) -> Result<(), String> {
        for (s, max) in [
            (&self.author, 80),
            (&self.license, 120),
            (&self.source, 512),
        ] {
            if s.len() > max || s.chars().any(char::is_control) {
                return Err("Credits are too long or contain control characters".into());
            }
        }
        if !self.source.is_empty() {
            let url = reqwest::Url::parse(&self.source).map_err(|_| "Use an HTTPS source link")?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("Use an HTTPS source link without login details".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub revision: String,
}
impl Revision {
    fn validate(&self) -> Result<(), String> {
        if self.id.len() != 34
            || !self.id.starts_with("s-")
            || !hex(&self.id[2..], 32)
            || !hex(&self.revision, 64)
        {
            return Err("Invalid package identity".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub identity: Revision,
    pub credits: Credits,
    pub parent: Option<Revision>,
    pub picture: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ancestors: Vec<Attribution>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribution {
    pub identity: Revision,
    pub credits: Credits,
}
impl Origin {
    pub fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        self.credits.validate()?;
        if self.picture.as_ref().is_some_and(|hash| !hex(hash, 64)) {
            return Err("Invalid picture identity".into());
        }
        if let Some(parent) = &self.parent {
            parent.validate()?;
        }
        if self.ancestors.len() > 16 {
            return Err("This remix has too many source revisions".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for source in &self.ancestors {
            source.identity.validate()?;
            source.credits.validate()?;
            if source.identity == self.identity
                || !seen.insert((&source.identity.id, &source.identity.revision))
            {
                return Err("Repeated or circular source revision".into());
            }
        }
        Ok(())
    }
    pub fn lineage(&self) -> Vec<Attribution> {
        let mut all = self.ancestors.clone();
        all.push(Attribution {
            identity: self.identity.clone(),
            credits: self.credits.clone(),
        });
        all
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Picture {
    pub sha256: String,
    pub png: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub format: String,
    pub version: u32,
    pub origin: Origin,
    #[serde(deserialize_with = "read_profile")]
    pub profile: Profile,
    pub wallpaper: Option<Picture>,
}
fn read_profile<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Profile, D::Error> {
    // The image envelope is larger than a manifest. Reject its raw profile slice
    // before building arbitrary JSON value/array trees inside widget settings.
    let raw = <&'de serde_json::value::RawValue>::deserialize(deserializer)?;
    if raw.get().len() > super::MAX_PROFILE_BYTES {
        return Err(serde::de::Error::custom("Embedded profile is too large"));
    }
    super::import(raw.get().as_bytes()).map_err(serde::de::Error::custom)
}
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn new_id() -> Result<String, String> {
    let mut random = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut random))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "s-{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}
pub fn recognizes(bytes: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Header {
        format: String,
    }
    bytes.len() <= MAX_BYTES
        && serde_json::from_slice::<Header>(bytes).is_ok_and(|h| h.format == FORMAT)
}
impl Package {
    fn fingerprint(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Signed<'a> {
            format: &'a str,
            version: u32,
            origin: Origin,
            profile: &'a Profile,
            wallpaper: &'a Option<Picture>,
        }
        struct Hash(Sha256);
        impl std::io::Write for Hash {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.update(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut origin = self.origin.clone();
        origin.identity.revision.clear();
        let mut hash = Hash(Sha256::new());
        serde_json::to_writer(
            &mut hash,
            &Signed {
                format: &self.format,
                version: self.version,
                origin,
                profile: &self.profile,
                wallpaper: &self.wallpaper,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(hash
            .0
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("This look package is too large".into());
        }
        let package: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid look package: {e}"))?;
        package.validate()?;
        Ok(package)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT
            || ![1, 2].contains(&self.version)
            || (self.version == 1 && !self.origin.ancestors.is_empty())
        {
            return Err("This look package needs a supported Halcyon version".into());
        }
        self.origin.validate()?;
        if self.origin.picture.as_ref() != self.wallpaper.as_ref().map(|p| &p.sha256) {
            return Err("Picture reference does not match the package".into());
        }
        if self.profile.package.is_some() {
            return Err("Nested package metadata is not supported".into());
        }
        // The legacy portable validator owns the allowed settings and bindings.
        super::import(&super::export_local(&self.profile).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if let Some(picture) = &self.wallpaper {
            if self
                .profile
                .wallpaper
                .as_ref()
                .and_then(|w| w.background.as_ref())
                .is_none()
            {
                return Err("A packaged picture needs a shared wallpaper setting".into());
            }
            if picture.png.len() > MAX_IMAGE.div_ceil(3) * 4 || !hex(&picture.sha256, 64) {
                return Err("Invalid or oversized package picture".into());
            }
            let bytes = STANDARD
                .decode(&picture.png)
                .map_err(|_| "Invalid picture encoding")?;
            if bytes.len() > MAX_IMAGE || digest(&bytes) != picture.sha256 {
                return Err("The package picture has changed".into());
            }
            assets::check_png(&bytes)?;
        }
        if self.fingerprint()? != self.origin.identity.revision {
            return Err("The look package revision does not match its contents".into());
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("This look package is too large".into());
        }
        Ok(bytes)
    }
    /// Explicit export only. The selected shared picture is normalized, never copied with metadata.
    pub fn build(
        profile: &Profile,
        include_picture: bool,
        credits: Credits,
    ) -> Result<(Self, Vec<&'static str>), String> {
        profile.validate().map_err(|e| e.to_string())?;
        credits.validate()?;
        let (mut portable, mut omitted) = profile.portable();
        portable.package = None;
        let wallpaper = if include_picture {
            let path = match profile
                .wallpaper
                .as_ref()
                .and_then(|w| w.background.as_ref())
                .and_then(|b| b.source.as_ref())
            {
                Some(SourceLook::Image(path)) => path,
                _ => return Err(
                    "Choose a saved setup with a single personal wallpaper to include its picture"
                        .into(),
                ),
            };
            if credits.author.trim().is_empty() || credits.license.trim().is_empty() {
                return Err("Add the picture author and license before including it".into());
            }
            let bytes = assets::normalize(path)?;
            // Plain profile export may remove the entire personal background.
            // Restore only its portable fit here; display-specific members stay out.
            let mut background = profile
                .wallpaper
                .as_ref()
                .and_then(|w| w.background.clone())
                .ok_or("No shared wallpaper setting")?;
            // A small deterministic fallback remains in plain profile exports.
            background.source = Some(SourceLook::Color(super::Rgb::Hex("#16252b".into())));
            background.slideshow = None;
            portable
                .wallpaper
                .get_or_insert_with(Default::default)
                .background = Some(background);
            omitted.retain(|s| *s != super::OWN_WALLPAPER);
            Some(Picture {
                sha256: digest(&bytes),
                png: STANDARD.encode(bytes),
            })
        } else {
            None
        };
        portable.version = portable.written_version();
        let mut package = Self {
            format: FORMAT.into(),
            version: if profile.package.is_some() { 2 } else { 1 },
            origin: Origin {
                identity: Revision {
                    id: new_id()?,
                    revision: String::new(),
                },
                credits,
                parent: profile.package.as_ref().map(|p| p.identity.clone()),
                picture: wallpaper.as_ref().map(|p| p.sha256.clone()),
                ancestors: profile
                    .package
                    .as_ref()
                    .map(Origin::lineage)
                    .unwrap_or_default(),
            },
            profile: portable,
            wallpaper,
        };
        package.origin.identity.revision = package.fingerprint()?;
        package.validate()?;
        Ok((package, omitted))
    }
    /// Deterministic identity for a source shipped with this application.
    pub fn curated(profile: &Profile, slug: &str, credits: Credits) -> Result<Self, String> {
        let mut portable = profile.portable().0;
        portable.package = None;
        portable.version = portable.written_version();
        let mut package = Self {
            format: FORMAT.into(),
            version: 1,
            profile: portable,
            wallpaper: None,
            origin: Origin {
                identity: Revision {
                    id: format!(
                        "s-{}",
                        &digest(format!("halcyon-curated/{slug}").as_bytes())[..32]
                    ),
                    revision: String::new(),
                },
                credits,
                parent: None,
                picture: None,
                ancestors: Vec::new(),
            },
        };
        package.origin.identity.revision = package.fingerprint()?;
        package.validate()?;
        Ok(package)
    }
    /// Publish all assets before any library reference. Desktop changes require a later Apply.
    pub fn import(
        &self,
        store: &crate::desktop::DesktopStore,
    ) -> Result<super::ProfileId, ProfileError> {
        let work = || -> Result<Profile, String> {
            self.validate()?;
            let mut local = self.profile.clone();
            if let Some(picture) = &self.wallpaper {
                let bytes = STANDARD.decode(&picture.png).map_err(|e| e.to_string())?;
                let path = assets::stage(store, &picture.sha256, &bytes)?;
                local
                    .wallpaper
                    .as_mut()
                    .unwrap()
                    .background
                    .as_mut()
                    .unwrap()
                    .source = Some(SourceLook::Image(path));
            }
            local.package = Some(self.origin.clone());
            local.version = super::VERSION;
            local.validate().map_err(|e| e.to_string())?;
            Ok(local)
        };
        super::ProfileLibrary::new(store).create(&work().map_err(ProfileError::Invalid)?)
    }
}

pub(crate) fn verify_picture(path: &std::path::Path, hash: &str) -> Result<(), String> {
    if !hex(hash, 64) || digest(&assets::read(path, MAX_IMAGE)?) != hash {
        return Err(
            "The imported wallpaper has changed. Import its package again before applying.".into(),
        );
    }
    Ok(())
}
