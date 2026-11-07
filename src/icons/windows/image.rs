//! Win32 image loading: render a shell icon or thumbnail to RGBA pixels, and
//! the worker-thread queue that does it off the UI thread.

use super::*;
use std::collections::VecDeque;
use std::mem::size_of;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    GetDIBits, GetObjectW, HBITMAP,
};
use windows::Win32::System::Com::Urlmon::E_PENDING;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_ICONONLY, SIIGBF_THUMBNAILONLY,
};
use windows::core::HSTRING;

pub(super) fn to_rgba(pixels: &mut [u8]) {
    let no_alpha = pixels.chunks_exact(4).all(|pixel| pixel[3] == 0);
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if no_alpha {
            pixel[3] = u8::MAX;
        }
    }
}

unsafe fn read_bitmap(bitmap: HBITMAP) -> Option<Pixels> {
    unsafe {
        let mut info = BITMAP::default();
        let size = size_of::<BITMAP>() as i32;
        if GetObjectW(bitmap.into(), size, Some((&raw mut info).cast())) == 0 {
            return None;
        }
        let (width, height) = (info.bmWidth.unsigned_abs(), info.bmHeight.unsigned_abs());
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = Pixels::new(width, height);
        let target = pixels.make_mut_bytes();
        let context = CreateCompatibleDC(None);
        let copied =
            GetDIBits(context, bitmap, 0, height, Some(target.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
        let _ = DeleteDC(context);
        if copied == 0 {
            return None;
        }
        to_rgba(target);
        Some(pixels)
    }
}

fn render_bitmap(factory: &IShellItemImageFactory, size: SIZE, flags: SIIGBF) -> Option<HBITMAP> {
    for _ in 0..PENDING_RETRIES {
        match unsafe { factory.GetImage(size, flags) } {
            Err(error) if error.code() == E_PENDING => thread::sleep(PENDING_RETRY_DELAY),
            result => return result.ok(),
        }
    }
    None
}

pub(super) fn shell_image(path: &str, size: u32, flags: SIIGBF) -> Option<Pixels> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let wanted = SIZE { cx: size as i32, cy: size as i32 };
        let bitmap = render_bitmap(&factory, wanted, flags)?;
        let pixels = read_bitmap(bitmap);
        let _ = DeleteObject(bitmap.into());
        pixels
    }
}

pub(super) struct Request {
    pub(super) key: IconKey,
    pub(super) path: String,
    pub(super) pixels: u32,
}

impl Request {
    fn load(&self) -> Option<Pixels> {
        let path = shell_path(&self.key.source, &self.path);
        let thumbnail = self.key.thumbnail.then(|| shell_image(path, self.pixels, SIIGBF_THUMBNAILONLY));
        thumbnail.flatten().or_else(|| shell_image(path, self.pixels, SIIGBF_ICONONLY))
    }
}

#[derive(Default)]
pub(super) struct Queue {
    pub(super) requests: Mutex<VecDeque<Request>>,
    pub(super) wake: Condvar,
}

pub(super) fn load_requests(queue: Arc<Queue>, results: Sender<(IconKey, Option<Pixels>)>) {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    loop {
        let request = {
            let mut requests = queue.requests.lock().unwrap();
            loop {
                match requests.pop_back() {
                    Some(request) => break request,
                    None => requests = queue.wake.wait(requests).unwrap(),
                }
            }
        };
        let pixels = request.load();
        if results.send((request.key, pixels)).is_err() {
            return;
        }
    }
}
