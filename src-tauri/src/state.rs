//! Shared state: the images and videos that are open, and the accessors commands use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kelvinlens_core::{Params, ThermalImage, video::Video};

use crate::cmd_error::CmdError;

#[derive(Default)]
pub(crate) struct Store {
    pub(crate) next_id: u32,
    pub(crate) images: HashMap<u32, Arc<ThermalImage>>,
    pub(crate) videos: HashMap<u32, Arc<Video>>,
    /// Bytes sent by the UI ahead of `write_staged` (exported images).
    pub(crate) staged: Option<Vec<u8>>,
    /// JPEG frames of a video export in progress.
    pub(crate) avi_frames: Vec<Vec<u8>>,
    /// Recently decoded video frames (video id, frame, parameters) for stepping and the
    /// high-sensitivity view, which needs the frames before the current one.
    pub(crate) frames: FrameCache,
}

impl Store {
    pub(crate) fn insert_image(&mut self, image: Arc<ThermalImage>) -> u32 {
        self.next_id += 1;
        self.images.insert(self.next_id, image);
        self.next_id
    }

    pub(crate) fn image_or(&self, id: u32, msg: &'static str) -> CmdResult<Arc<ThermalImage>> {
        self.images.get(&id).cloned().ok_or_else(|| CmdError::not_found(msg))
    }

    pub(crate) fn image(&self, id: u32) -> CmdResult<Arc<ThermalImage>> {
        self.image_or(id, "This image is no longer open")
    }

    pub(crate) fn video(&self, id: u32) -> CmdResult<Arc<Video>> {
        self.videos.get(&id).cloned().ok_or_else(|| CmdError::unsupported("This image is not a video"))
    }
}

#[derive(Default)]
pub(crate) struct FrameCache {
    pub(crate) entries: std::collections::VecDeque<(u32, usize, Option<Params>, Arc<ThermalImage>)>,
    pub(crate) bytes: usize,
}

impl FrameCache {
    /// About 256 MB of temperatures.
    const LIMIT: usize = 256 * 1024 * 1024;

    pub(crate) fn get(&self, id: u32, index: usize, params: &Option<Params>) -> Option<Arc<ThermalImage>> {
        self.entries.iter().find(|(i, f, p, _)| *i == id && *f == index && p == params).map(|e| e.3.clone())
    }

    pub(crate) fn put(&mut self, id: u32, index: usize, params: Option<Params>, image: Arc<ThermalImage>) {
        self.bytes += image.kelvin().len() * 4;
        self.entries.push_back((id, index, params, image));
        while self.bytes > Self::LIMIT && self.entries.len() > 1 {
            if let Some((_, _, _, old)) = self.entries.pop_front() {
                self.bytes -= old.kelvin().len() * 4;
            }
        }
    }

    pub(crate) fn forget(&mut self, id: u32) {
        self.entries.retain(|e| e.0 != id);
        self.bytes = self.entries.iter().map(|e| e.3.kelvin().len() * 4).sum();
    }
}

/// Frame `index` of video `id` with `params`, from the cache or decoded (and cached).
pub(crate) fn cached_frame(
    state: &AppState,
    id: u32,
    video: &Video,
    index: usize,
    params: &Option<Params>,
) -> CmdResult<Arc<ThermalImage>> {
    if let Some(f) = state.lock()?.frames.get(id, index, params) {
        return Ok(f);
    }
    let frame = Arc::new(video.frame(index, params.as_ref())?);
    state.lock()?.frames.put(id, index, *params, frame.clone());
    Ok(frame)
}

pub(crate) type AppState = Mutex<Store>;
pub(crate) type CmdResult<T> = Result<T, CmdError>;

/// Shared handle to an open image; the store lock is released before this returns.
pub(crate) fn image(state: &AppState, id: u32) -> CmdResult<Arc<ThermalImage>> {
    state.lock()?.image(id)
}

/// Changes an open image under the store lock. Copy-on-write: an in-flight render that holds the
/// image keeps its consistent old copy, and the next one sees the change.
pub(crate) fn with_image_mut<T>(
    state: &AppState,
    id: u32,
    f: impl FnOnce(&mut ThermalImage) -> CmdResult<T>,
) -> CmdResult<T> {
    let mut store = state.lock()?;
    let entry = store.images.get_mut(&id).ok_or_else(|| CmdError::not_found("This image is no longer open"))?;
    f(Arc::make_mut(entry))
}

/// The video `id` and the parameters its current frame is shown with (`None`: the file's own).
pub(crate) fn video_and_params(state: &AppState, id: u32) -> CmdResult<(Arc<Video>, Option<Params>)> {
    let store = state.lock()?;
    let video = store.video(id)?;
    let img = store.image_or(id, "This video is no longer open")?;
    Ok((video, (img.params() != img.file_params()).then(|| img.params())))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use kelvinlens_core::Format;
    use kelvinlens_core::thermogram::Camera;

    use super::*;
    use crate::cmd_error::ErrorKind;

    fn test_image() -> ThermalImage {
        ThermalImage::from_kelvin(Path::new("a.tiff"), Format::Panorama, vec![300.0; 4], 2, 2, Camera::default())
            .unwrap()
    }

    #[test]
    fn store_image_missing_has_kept_messages() {
        let store = Store::default();
        let e = store.image(1).err().unwrap();
        assert_eq!((e.kind, e.message.as_str()), (ErrorKind::NotFound, "This image is no longer open"));
        let e = store.image_or(1, "x").err().unwrap();
        assert_eq!((e.kind, e.message.as_str()), (ErrorKind::NotFound, "x"));
    }

    #[test]
    fn video_on_plain_image_is_unsupported() {
        let mut store = Store::default();
        let id = store.insert_image(Arc::new(test_image()));
        let e = store.video(id).err().unwrap();
        assert_eq!((e.kind, e.message.as_str()), (ErrorKind::Unsupported, "This image is not a video"));
    }

    #[test]
    fn make_mut_leaves_shared_holder_untouched() {
        // Variant of the plan's test: the write goes through `with_image_mut`, which copies the
        // image when a render still holds it.
        let state = AppState::default();
        let id = state.lock().unwrap().insert_image(Arc::new(test_image()));
        let held = image(&state, id).unwrap();
        with_image_mut(&state, id, |_| Ok(())).unwrap();
        assert!(!Arc::ptr_eq(&held, state.lock().unwrap().images.get(&id).unwrap()));
        // With no other holder the image is changed in place.
        drop(held);
        let now = image(&state, id).unwrap();
        drop(now);
        let before = Arc::as_ptr(state.lock().unwrap().images.get(&id).unwrap());
        with_image_mut(&state, id, |_| Ok(())).unwrap();
        assert_eq!(before, Arc::as_ptr(state.lock().unwrap().images.get(&id).unwrap()));
    }

    #[test]
    fn store_is_free_while_an_image_is_in_use() {
        let state = Arc::new(AppState::default());
        let id = state.lock().unwrap().insert_image(Arc::new(test_image()));
        let held = image(&state, id).unwrap(); // a long render holds this
        let worker = std::thread::spawn(move || {
            let _held = held;
            std::thread::sleep(Duration::from_millis(500));
        });
        let t = Instant::now();
        let other = image(&state, id).unwrap(); // another command meanwhile
        assert!(t.elapsed() < Duration::from_millis(100));
        drop(other);
        worker.join().unwrap();
    }
}
