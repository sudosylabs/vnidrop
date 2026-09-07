use super::App;
use gtk::{gdk_pixbuf::Pixbuf, gio, glib};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    rc::Rc,
};

fn thumbnail(source: &Path) -> Option<Vec<u8>> {
    let metadata = std::fs::metadata(source).ok()?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return None;
    }
    let (format, width, height) = Pixbuf::file_info(source)?;
    if !matches!(format.name().as_deref(), Some("png" | "jpeg" | "webp"))
        || width <= 0
        || height <= 0
        || i64::from(width) * i64::from(height) > 32_000_000
    {
        return None;
    }
    let image = Pixbuf::from_file_at_scale(source, 256, 256, true).ok()?;
    let bytes = image.save_to_bufferv("png", &[]).ok()?;
    (bytes.len() <= 512 * 1024).then_some(bytes)
}
fn restore(profile: &Path, active: &BTreeSet<u64>) -> BTreeMap<u64, Vec<u8>> {
    let directory = profile.join("ui/previews");
    let mut entries: Vec<_> = std::fs::read_dir(&directory)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let id = entry
                .file_name()
                .to_str()?
                .strip_suffix(".preview")?
                .parse::<u64>()
                .ok()?;
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return None;
            }
            Some((metadata.modified().ok(), id, entry.path(), metadata.len()))
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let mut used = 0;
    let mut previews = BTreeMap::new();
    for (_, id, path, size) in entries {
        if !active.contains(&id) || size == 0 || size > 512 * 1024 || used + size > 20 * 1024 * 1024
        {
            let _ = std::fs::remove_file(path);
            continue;
        }
        let mut bytes = Vec::new();
        if std::fs::File::open(&path)
            .and_then(|f| f.take(512 * 1024 + 1).read_to_end(&mut bytes))
            .is_ok()
            && bytes.len() <= 512 * 1024
        {
            if let Some((_, width, height)) = Pixbuf::file_info(&path) {
                if width > 0 && height > 0 && width <= 1024 && height <= 1024 {
                    used += size;
                    previews.insert(id, bytes);
                }
            }
        }
    }
    previews
}
impl App {
    pub(super) fn save_preview(self: &Rc<Self>, id: u64, source: std::path::PathBuf) {
        let profile = self.profile.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(move || {
                let Some(bytes) = thumbnail(&source) else {
                    return;
                };
                let dir = profile.join("ui/previews");
                if std::fs::create_dir_all(&dir).is_err() {
                    return;
                }
                let temp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
                if std::fs::write(&temp, bytes).is_ok() {
                    let _ = std::fs::rename(&temp, dir.join(format!("{id}.preview")));
                }
                let _ = std::fs::remove_file(temp);
            })
            .await;
            if let Some(app) = weak.upgrade() {
                app.preview_ids.borrow_mut().clear();
                app.refresh();
            }
        });
    }
    pub(super) fn load_previews(self: &Rc<Self>) {
        if self.preview_loading.get() {
            return;
        }
        let ids: BTreeSet<_> = self
            .snapshot
            .borrow()
            .as_ref()
            .map(|s| {
                s.transfers
                    .iter()
                    .filter(|t| t.direction == "send")
                    .map(|t| t.transfer_id)
                    .collect()
            })
            .unwrap_or_default();
        if *self.preview_ids.borrow() == ids {
            return;
        }
        self.preview_ids.replace(ids.clone());
        self.preview_loading.set(true);
        let (profile, weak) = (self.profile.clone(), Rc::downgrade(self));
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || restore(&profile, &ids))
                .await
                .unwrap_or_default();
            if let Some(app) = weak.upgrade() {
                app.preview_loading.set(false);
                app.previews.replace(
                    result
                        .into_iter()
                        .filter_map(|(id, data)| {
                            gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(data))
                                .ok()
                                .map(|image| (id, image))
                        })
                        .collect(),
                );
                app.detail_fingerprint.borrow_mut().clear();
                app.render();
                app.load_previews();
            }
        });
    }
}

#[cfg(test)]
pub(super) fn verify_preview_cache() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("original.png");
    let image = Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 64, 64).unwrap();
    image.fill(0x6699ccff);
    image.savev(&source, "png", &[]).unwrap();
    let bytes = thumbnail(&source).unwrap();
    let dir = root.path().join("ui/previews");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("1.preview"), &bytes).unwrap();
    std::fs::write(dir.join("2.preview"), &bytes).unwrap();
    std::fs::remove_file(source).unwrap();
    let restored = restore(root.path(), &BTreeSet::from([1]));
    assert_eq!(restored.get(&1), Some(&bytes));
    assert!(!dir.join("2.preview").exists());
    std::fs::write(dir.join("3.preview"), vec![0; 512 * 1024 + 1]).unwrap();
    restore(root.path(), &BTreeSet::from([1, 3]));
    assert!(!dir.join("3.preview").exists());
}
