//! Effects that parsing raises: replies to the program and notices for the
//! embedder.
//!
//! libghostty reports effects through C callbacks while
//! [`Terminal::write`](crate::Terminal::write) runs. The callbacks registered
//! here turn them into [`TerminalEvent`]s, in stream order, for the handler
//! set with [`Terminal::set_event_handler`](crate::Terminal::set_event_handler).
//! OSC 52 clipboard requests have no callback, so the terminal ignores both
//! reads and writes.

use std::{ffi::c_void, slice};

use crate::{ffi, terminal::TerminalSize};

/// An effect of program output, in the order the output raised it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    /// Bytes the terminal answers the program with, such as a status or
    /// color report; they belong on the PTY's input.
    Reply(Vec<u8>),
    /// The program set the title (OSC 0 or 2).
    Title(String),
    /// The program rang the bell.
    Bell,
    /// The program reported its working directory (OSC 7, 9, or 1337) as
    /// it sent it: a `file://` URI or a path; empty when cleared.
    WorkingDirectory(String),
}

pub(crate) type EventHandler = Box<dyn FnMut(TerminalEvent) + Send>;

/// What the callbacks reach through the terminal's userdata pointer.
pub(crate) struct Callbacks {
    /// The geometry that size reports describe.
    pub(crate) size: TerminalSize,
    pub(crate) handler: Option<EventHandler>,
}

/// Registers every callback on `raw`, with `callbacks` as userdata.
///
/// # Safety
///
/// `callbacks` must stay valid, and not be accessed through a reference
/// during any terminal call, until `raw` is freed.
pub(crate) unsafe fn register(raw: ffi::GhosttyTerminal, callbacks: *mut Callbacks) {
    let options: [(ffi::GhosttyTerminalOption, *const c_void); 6] = [
        (ffi::GHOSTTY_TERMINAL_OPT_USERDATA, callbacks.cast()),
        (
            ffi::GHOSTTY_TERMINAL_OPT_WRITE_PTY,
            write_pty as *const c_void,
        ),
        (ffi::GHOSTTY_TERMINAL_OPT_BELL, bell as *const c_void),
        (
            ffi::GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
            title_changed as *const c_void,
        ),
        (
            ffi::GHOSTTY_TERMINAL_OPT_PWD_CHANGED,
            pwd_changed as *const c_void,
        ),
        (ffi::GHOSTTY_TERMINAL_OPT_SIZE, size as *const c_void),
    ];
    for (option, value) in options {
        // SAFETY: each callback option takes the function itself, with the
        // signature its typedef declares; userdata is an opaque pointer.
        crate::terminal::check(
            unsafe { ffi::ghostty_terminal_set(raw, option, value) },
            "registering a terminal callback",
        );
    }
}

/// # Safety
///
/// `userdata` is the pointer [`register`] installed, and no reference to
/// it is live.
unsafe fn emit(userdata: *mut c_void, event: TerminalEvent) {
    // SAFETY: guaranteed by the caller.
    let callbacks = unsafe { &mut *userdata.cast::<Callbacks>() };
    if let Some(handler) = callbacks.handler.as_mut() {
        handler(event);
    }
}

/// Reads a borrowed string the terminal holds, such as its title.
///
/// # Safety
///
/// `raw` is live and `data` is a kind whose output is a `GhosttyString`.
unsafe fn read_string(raw: ffi::GhosttyTerminal, data: ffi::GhosttyTerminalData) -> String {
    let mut string = ffi::GhosttyString {
        ptr: std::ptr::null(),
        len: 0,
    };
    // SAFETY: guaranteed by the caller.
    let result = unsafe {
        ffi::ghostty_terminal_get(raw, data, (&mut string as *mut ffi::GhosttyString).cast())
    };
    if result != ffi::GHOSTTY_SUCCESS || string.len == 0 {
        return String::new();
    }
    // SAFETY: the library returned `len` readable bytes.
    String::from_utf8_lossy(unsafe { slice::from_raw_parts(string.ptr, string.len) }).into_owned()
}

unsafe extern "C" fn write_pty(
    _raw: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    // SAFETY: the library passes `len` readable bytes for the call.
    let bytes = unsafe { slice::from_raw_parts(data, len) }.to_vec();
    // SAFETY: the library passes back the registered userdata.
    unsafe { emit(userdata, TerminalEvent::Reply(bytes)) };
}

unsafe extern "C" fn bell(_raw: ffi::GhosttyTerminal, userdata: *mut c_void) {
    // SAFETY: the library passes back the registered userdata.
    unsafe { emit(userdata, TerminalEvent::Bell) };
}

unsafe extern "C" fn title_changed(raw: ffi::GhosttyTerminal, userdata: *mut c_void) {
    // SAFETY: the callback receives the live terminal, and the title is a string.
    let title = unsafe { read_string(raw, ffi::GHOSTTY_TERMINAL_DATA_TITLE) };
    // SAFETY: the library passes back the registered userdata.
    unsafe { emit(userdata, TerminalEvent::Title(title)) };
}

unsafe extern "C" fn pwd_changed(raw: ffi::GhosttyTerminal, userdata: *mut c_void) {
    // SAFETY: the callback receives the live terminal, and the pwd is a string.
    let directory = unsafe { read_string(raw, ffi::GHOSTTY_TERMINAL_DATA_PWD) };
    // SAFETY: the library passes back the registered userdata.
    unsafe { emit(userdata, TerminalEvent::WorkingDirectory(directory)) };
}

unsafe extern "C" fn size(
    _raw: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    out: *mut ffi::GhosttySizeReportSize,
) -> bool {
    // SAFETY: the library passes back the registered userdata, and no
    // reference to it is live during a terminal call.
    let size = unsafe { &*userdata.cast::<Callbacks>() }.size;
    // SAFETY: the library passes a writable report.
    unsafe {
        *out = ffi::GhosttySizeReportSize {
            rows: size.rows,
            columns: size.columns,
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        };
    }
    true
}
