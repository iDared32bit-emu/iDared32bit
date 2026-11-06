/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Games' names and icons, kept between runs so the app picker doesn't have
//! to open every game each time it's shown.

use crate::image::Image;
use plist::{Dictionary, Value};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const CACHE_FILE: &str = "AppPicker.plist";

pub struct Cache {
    path: PathBuf,
    /// What else the names and icons depend on. If it changes, nothing in
    /// the cache is used.
    settings: String,
    /// What was saved last time, by game file name.
    old: Dictionary,
    /// The games found this time, which is what's saved.
    new: Dictionary,
    changed: bool,
}

impl Cache {
    /// `settings` must describe everything the names and icons depend on
    /// other than the games themselves, e.g. the preferred languages.
    pub fn load(dir: &Path, settings: &str) -> Cache {
        let path = dir.join(CACHE_FILE);
        let settings = format!("{} {}", crate::DISPLAY_VERSION, settings);
        let old = Value::from_file(&path)
            .ok()
            .and_then(Value::into_dictionary)
            .filter(|cache| {
                cache.get("Settings").and_then(Value::as_string) == Some(settings.as_str())
            })
            .and_then(|mut cache| cache.remove("Games"))
            .and_then(Value::into_dictionary)
            .unwrap_or_default();
        Cache {
            path,
            settings,
            old,
            new: Dictionary::new(),
            changed: false,
        }
    }

    /// The name and icon saved for a game, if it hasn't changed since.
    pub fn get(&mut self, app_path: &Path) -> Option<(String, Option<Image>)> {
        let key = key(app_path)?;
        let stamp = stamp(app_path)?;
        let entry = self.old.get(&key)?.as_dictionary()?;
        if entry.get("Stamp")?.as_string()? != stamp {
            return None;
        }
        let name = entry.get("Name")?.as_string()?.to_string();
        let icon = match entry.get("Icon") {
            Some(icon) => Some(Image::from_png_premultiplied(icon.as_data()?).ok()?),
            None => None,
        };
        self.new.insert(key, Value::Dictionary(entry.clone()));
        Some((name, icon))
    }

    /// Saves a game's name and icon (none if it couldn't be loaded).
    pub fn insert(&mut self, app_path: &Path, name: &str, icon: Option<&Image>) {
        let (Some(key), Some(stamp)) = (key(app_path), stamp(app_path)) else {
            return;
        };
        let mut entry = Dictionary::new();
        entry.insert("Stamp".to_string(), Value::String(stamp));
        entry.insert("Name".to_string(), Value::String(name.to_string()));
        if let Some(icon) = icon {
            entry.insert("Icon".to_string(), Value::Data(icon.to_png()));
        }
        self.new.insert(key, Value::Dictionary(entry));
        self.changed = true;
    }

    /// Writes the cache, if anything was added or a game has gone.
    pub fn save(self) {
        if !self.changed && self.new.len() == self.old.len() {
            return;
        }
        let mut cache = Dictionary::new();
        cache.insert("Settings".to_string(), Value::String(self.settings));
        cache.insert("Games".to_string(), Value::Dictionary(self.new));
        // Written beside it first, so a half-written cache is never read.
        let temp = self.path.with_extension("tmp");
        let result = self
            .path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                Value::Dictionary(cache)
                    .to_file_binary(&temp)
                    .map_err(|e| e.to_string())
            })
            .and_then(|()| std::fs::rename(&temp, &self.path).map_err(|e| e.to_string()));
        if let Err(e) = result {
            log!("Warning: couldn't save {}: {}", self.path.display(), e);
        }
    }
}

fn key(app_path: &Path) -> Option<String> {
    app_path.file_name()?.to_str().map(str::to_string)
}

/// Changes when the game does: its size and modification time. A folder's
/// time doesn't change when files in it do, so for an .app folder, its
/// Info.plist's are used.
fn stamp(app_path: &Path) -> Option<String> {
    let file = if app_path.is_dir() {
        app_path.join("Info.plist")
    } else {
        app_path.to_path_buf()
    };
    let metadata = std::fs::metadata(file).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!("{} {}", metadata.len(), modified.as_nanos()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("idared_cache_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saved_and_loaded_exactly() {
        let dir = test_dir("saved");
        let game = dir.join("Game.ipa");
        std::fs::write(&game, b"game").unwrap();
        // Premultiplied, including a half-transparent pixel.
        let pixels = vec![255, 0, 0, 255, 64, 32, 0, 128, 0, 0, 0, 0, 1, 2, 3, 4];
        let icon = Image::from_pixel_vec(pixels.clone(), (2, 2));

        let mut cache = Cache::load(&dir, "en");
        assert!(cache.get(&game).is_none());
        cache.insert(&game, "Game", Some(&icon));
        cache.insert(&dir.join("Other.ipa"), "Other", None); // no such file
        cache.save();

        let mut cache = Cache::load(&dir, "en");
        let (name, icon) = cache.get(&game).unwrap();
        assert_eq!(name, "Game");
        let icon = icon.unwrap();
        assert_eq!(icon.dimensions(), (2, 2));
        assert_eq!(icon.pixels(), &pixels[..]);

        // Different settings: nothing is used.
        assert!(Cache::load(&dir, "fr").get(&game).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn changed_and_removed_games() {
        let dir = test_dir("changed");
        let game = dir.join("Game.ipa");
        let gone = dir.join("Gone.ipa");
        std::fs::write(&game, b"game").unwrap();
        std::fs::write(&gone, b"gone").unwrap();
        let mut cache = Cache::load(&dir, "");
        cache.insert(&game, "Game", None);
        cache.insert(&gone, "Gone", None);
        cache.save();

        // A changed game isn't used; one that's gone is dropped when saved.
        std::fs::write(&game, b"game, updated").unwrap();
        std::fs::remove_file(&gone).unwrap();
        let mut cache = Cache::load(&dir, "");
        assert!(cache.get(&game).is_none());
        cache.insert(&game, "Game 2", None);
        cache.save();
        std::fs::write(&gone, b"gone").unwrap();
        let mut cache = Cache::load(&dir, "");
        assert_eq!(cache.get(&game).unwrap().0, "Game 2");
        assert!(cache.get(&gone).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
