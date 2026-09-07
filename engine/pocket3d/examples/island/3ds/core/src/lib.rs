#![no_std]
extern crate alloc;
use alloc::boxed::Box;
use core::{
    alloc::{GlobalAlloc, Layout},
    ffi::c_void,
};
use pocket_island::{Input, Island};
unsafe extern "C" {
    fn memalign(align: usize, size: usize) -> *mut c_void;
    fn free(p: *mut c_void);
    fn abort() -> !;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        unsafe { memalign(l.align().max(8), l.size().max(1)).cast() }
    }
    unsafe fn dealloc(&self, p: *mut u8, _: Layout) {
        unsafe { free(p.cast()) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { abort() }
}
#[repr(C)]
pub struct Snapshot {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub cam_x: f32,
    pub cam_z: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub anchor_z: f32,
    pub action: u32,
    pub expression: u32,
    pub tick: u32,
    pub messages: u32,
}
#[unsafe(no_mangle)]
pub extern "C" fn island_new() -> *mut Island {
    Box::into_raw(Box::new(Island::new()))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_free(s: *mut Island) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_step(s: *mut Island, x: f32, z: f32, flags: u32) {
    unsafe {
        (*s).advance(Input {
            x,
            z,
            run: flags & 1 != 0,
            wave: flags & 2 != 0,
            sit: flags & 4 != 0,
            cheer: flags & 8 != 0,
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_expression(s: *mut Island, e: u32) {
    unsafe { (*s).set_expression(e as usize) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_vertices(
    s: *const Island,
    terrain: bool,
    count: *mut u32,
) -> *const c_void {
    let v = if terrain {
        &unsafe { &*s }.terrain
    } else {
        &unsafe { &*s }.character
    };
    unsafe {
        *count = v.len() as u32;
    }
    v.as_ptr().cast()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_snapshot(s: *const Island, out: *mut Snapshot) {
    let s = unsafe { &*s };
    let a = s.bubble_anchor();
    unsafe {
        *out = Snapshot {
            x: s.position.x,
            y: s.position.y,
            z: s.position.z,
            cam_x: s.render_camera.x,
            cam_z: s.render_camera.z,
            anchor_x: a.x,
            anchor_y: a.y,
            anchor_z: a.z,
            action: s.action as u32,
            expression: s.expression as u32,
            tick: s.tick as u32,
            messages: s.chat.history.len() as u32,
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_send(s: *mut Island, text: *const u8, len: u32) -> i32 {
    if text.is_null() || len > 192 {
        return -1;
    }
    let Ok(text) = core::str::from_utf8(unsafe { core::slice::from_raw_parts(text, len as usize) })
    else {
        return -1;
    };
    match unsafe { &mut *s }.send(text) {
        Ok(_) => 0,
        Err(pocket_island::MessageError::RateLimited) => -2,
        Err(_) => -1,
    }
}
unsafe fn copy_text(text: &str, out: *mut u8, capacity: u32) -> u32 {
    if capacity == 0 || out.is_null() {
        return 0;
    }
    let mut n = text.len().min(capacity as usize - 1);
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(text.as_ptr(), out, n);
        *out.add(n) = 0;
    }
    n as u32
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_bubble(s: *const Island, out: *mut u8, capacity: u32) -> u32 {
    let s = unsafe { &*s };
    unsafe {
        copy_text(
            s.chat
                .bubble(s.chat.local_peer, s.tick)
                .map(|m| m.body.as_str())
                .unwrap_or(""),
            out,
            capacity,
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_message(
    s: *const Island,
    index: u32,
    out: *mut u8,
    capacity: u32,
) -> u32 {
    let s = unsafe { &*s };
    unsafe {
        copy_text(
            s.chat
                .history
                .iter()
                .rev()
                .nth(index as usize)
                .map(|m| m.body.as_str())
                .unwrap_or(""),
            out,
            capacity,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_present(s: *mut Island, alpha: f32) {
    unsafe { (*s).present(alpha) }
}
