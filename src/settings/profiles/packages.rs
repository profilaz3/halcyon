use super::*;
use crate::desktop::profiles::packages::{Credits, Package};
#[cfg(test)]
mod tests;
#[derive(Clone, Debug)]
pub enum Edit {
    Open,
    Close,
    Picture(bool),
    Author(String),
    License(String),
    Source(String),
    Export,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Draft {
    pub profile: Profile,
    pub picture: bool,
    pub credits: Credits,
    picture_stamp: Option<[i128; 7]>,
}
fn picture_stamp(profile: &Profile) -> Option<[i128; 7]> {
    let Some(profiles::SourceLook::Image(path)) = profile
        .wallpaper
        .as_ref()
        .and_then(|w| w.background.as_ref())
        .and_then(|b| b.source.as_ref())
    else {
        return None;
    };
    let meta = fs::symlink_metadata(path).ok()?;
    meta.is_file().then(|| {
        [
            meta.dev() as i128,
            meta.ino() as i128,
            meta.len() as i128,
            meta.mtime() as i128,
            meta.mtime_nsec() as i128,
            meta.ctime() as i128,
            meta.ctime_nsec() as i128,
        ]
    })
}
impl Draft {
    fn picture_current(&self) -> bool {
        !self.picture
            || self.picture_stamp.is_some() && self.picture_stamp == picture_stamp(&self.profile)
    }
    pub fn has_picture(&self) -> bool {
        matches!(
            self.profile
                .wallpaper
                .as_ref()
                .and_then(|w| w.background.as_ref())
                .and_then(|b| b.source.as_ref()),
            Some(profiles::SourceLook::Image(_))
        )
    }
}
impl ProfilesEditor {
    pub(super) fn package_edit(&mut self, edit: Edit) -> Outcome {
        if self.busy() {
            return Outcome::None;
        }
        if matches!(edit, Edit::Open) {
            if let Some(profile) = self.profile().cloned() {
                let credits = profile
                    .package
                    .as_ref()
                    .map(|p| p.credits.clone())
                    .unwrap_or_default();
                self.package = Some(Draft {
                    picture_stamp: picture_stamp(&profile),
                    profile,
                    picture: false,
                    credits,
                });
                self.notice = None;
            }
            return Outcome::None;
        }
        if matches!(edit, Edit::Close) {
            self.package = None;
            self.notice = None;
            return Outcome::None;
        }
        let Some(draft) = &mut self.package else {
            return Outcome::None;
        };
        match edit {
            Edit::Picture(on) => draft.picture = on && draft.has_picture(),
            Edit::Author(s) => draft.credits.author = s.chars().take(80).collect(),
            Edit::License(s) => draft.credits.license = s.chars().take(120).collect(),
            Edit::Source(s) => draft.credits.source = s.chars().take(512).collect(),
            Edit::Export => {
                let draft = draft.clone();
                if self.profile() != Some(&draft.profile) || !draft.picture_current() {
                    self.notice = Some(Notice::Error(
                        "The setup or wallpaper changed. Reopen the package review.".into(),
                    ));
                    return Outcome::None;
                }
                let Some(store) = self.store().filter(|s| s.writable()) else {
                    return Outcome::None;
                };
                let confine = store.fixture_root().map(Path::to_path_buf);
                match Package::build(&draft.profile, draft.picture, draft.credits.clone())
                    .and_then(|(package, omitted)| package.encode().map(|bytes| (bytes, omitted)))
                    .and_then(|ready| {
                        if draft.picture_current() {
                            Ok(ready)
                        } else {
                            Err(
                                "The wallpaper changed during export. Reopen the package review."
                                    .into(),
                            )
                        }
                    }) {
                    Ok((bytes, omitted)) => {
                        let file_name = file_name(&draft.profile.name)
                            .trim_end_matches(".json")
                            .to_owned()
                            + ".halcyon";
                        let id = self.request(Pending::Export(draft.profile.name, omitted));
                        self.notice = None;
                        return Outcome::Dialog(Request::Export {
                            id,
                            file_name,
                            bytes,
                            confine,
                        });
                    }
                    Err(error) => self.notice = Some(Notice::Error(error)),
                }
            }
            _ => {}
        }
        Outcome::None
    }
}
