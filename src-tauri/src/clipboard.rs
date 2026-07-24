//! Native clipboard bridge used only after an explicit global shortcut.
//!
//! This module never observes clipboard changes continuously. It snapshots the
//! clipboard only for the short copy/paste operation and restores it afterwards.

use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_UNICODETEXT, OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CONTROL, VK_INSERT,
    VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
use windows::core::{Error, HRESULT, Result};

const RETRY_COUNT: usize = 40;
const RETRY_DELAY: Duration = Duration::from_millis(25);
const COPY_SETTLE_DELAY: Duration = Duration::from_millis(50);
const COPY_SENTINEL: &str = "__CODEX_INPUT_ENHANCER_COPY_SENTINEL__";
const KEY_HOLD_DELAY: Duration = Duration::from_millis(24);

#[derive(Clone, Debug)]
pub struct CapturedSelection {
    pub target: isize,
    pub text: String,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn send_key(vk: u16, key_up: bool) {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
}

fn send_ctrl_combo(vk: u16) {
    send_key(VK_CONTROL.0, false);
    thread::sleep(KEY_HOLD_DELAY);
    send_key(vk, false);
    thread::sleep(KEY_HOLD_DELAY);
    send_key(vk, true);
    thread::sleep(KEY_HOLD_DELAY);
    send_key(VK_CONTROL.0, true);
}

fn release_trigger_modifiers() {
    for key in [VK_CONTROL.0, VK_MENU.0, VK_SHIFT.0, VK_LWIN.0, VK_RWIN.0] {
        send_key(key, true);
    }
    thread::sleep(COPY_SETTLE_DELAY);
}

fn open_clipboard_with_retry() -> Result<()> {
    for _ in 0..RETRY_COUNT {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return Ok(());
        }
        thread::sleep(RETRY_DELAY);
    }
    unsafe { OpenClipboard(None) }
}

fn wait_for_clipboard_change(previous_sequence: u32) -> Result<()> {
    for _ in 0..RETRY_COUNT {
        if unsafe { GetClipboardSequenceNumber() } != previous_sequence {
            thread::sleep(COPY_SETTLE_DELAY);
            return Ok(());
        }
        thread::sleep(RETRY_DELAY);
    }
    Err(Error::new(
        HRESULT(0x80004005u32 as i32),
        "Ctrl+C did not update the clipboard.",
    ))
}

fn text_from_clipboard() -> Result<String> {
    unsafe {
        open_clipboard_with_retry()?;
        let result = (|| {
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32)?;
            let global = HGLOBAL(handle.0);
            let pointer = GlobalLock(global) as *const u16;
            if pointer.is_null() {
                return Err(Error::from_win32());
            }
            let mut length = 0usize;
            while *pointer.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            let _ = GlobalUnlock(global);
            Ok(text)
        })();
        CloseClipboard()?;
        result
    }
}

fn put_text_on_clipboard(text: &str) -> Result<()> {
    let text = wide(text);
    unsafe {
        open_clipboard_with_retry()?;
        let result = (|| {
            EmptyClipboard()?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * std::mem::size_of::<u16>())?;
            let destination = GlobalLock(memory) as *mut u16;
            if destination.is_null() {
                return Err(Error::from_win32());
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), destination, text.len());
            let _ = GlobalUnlock(memory);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0)))?;
            Ok(())
        })();
        CloseClipboard()?;
        result
    }
}

fn save_clipboard() -> Result<IDataObject> {
    for _ in 0..RETRY_COUNT {
        if let Ok(data) = unsafe { OleGetClipboard() } {
            return Ok(data);
        }
        thread::sleep(RETRY_DELAY);
    }
    unsafe { OleGetClipboard() }
}

fn restore_clipboard(saved: &IDataObject) -> Result<()> {
    for _ in 0..RETRY_COUNT {
        if unsafe { OleSetClipboard(saved) }.is_ok() {
            return Ok(());
        }
        thread::sleep(RETRY_DELAY);
    }
    unsafe { OleSetClipboard(saved) }
}

fn copy_selection_to_clipboard(target: HWND, previous_sequence: u32) -> bool {
    unsafe {
        let _ = SetForegroundWindow(target);
    }
    thread::sleep(COPY_SETTLE_DELAY);
    release_trigger_modifiers();
    send_ctrl_combo(b'C' as u16);
    if wait_for_clipboard_change(previous_sequence).is_ok() {
        return true;
    }
    unsafe {
        let _ = SetForegroundWindow(target);
    }
    thread::sleep(COPY_SETTLE_DELAY);
    send_ctrl_combo(VK_INSERT.0);
    wait_for_clipboard_change(previous_sequence).is_ok()
}

fn capture_selection() -> Result<Option<CapturedSelection>> {
    let target = unsafe { GetForegroundWindow() };
    if target.0.is_null() {
        return Err(Error::new(
            HRESULT(0x80004005u32 as i32),
            "No foreground window.",
        ));
    }
    let saved_clipboard = save_clipboard()?;
    put_text_on_clipboard(COPY_SENTINEL)?;
    let sentinel_sequence = unsafe { GetClipboardSequenceNumber() };
    if !copy_selection_to_clipboard(target, sentinel_sequence) {
        restore_clipboard(&saved_clipboard)?;
        return Ok(None);
    }
    let selected = text_from_clipboard();
    restore_clipboard(&saved_clipboard)?;
    selected.map(|text| {
        (!text.trim().is_empty()).then_some(CapturedSelection {
            target: target.0 as isize,
            text,
        })
    })
}

/// Runs the temporary OLE/clipboard interaction on a worker thread.
pub fn capture_selection_on_worker() -> Result<Option<CapturedSelection>> {
    unsafe { OleInitialize(None)? };
    let captured = capture_selection();
    unsafe { OleUninitialize() };
    captured
}

/// Reads plain text already on the clipboard without changing it.
pub fn read_clipboard_text_on_worker() -> Result<Option<String>> {
    let text = text_from_clipboard()?;
    Ok((!text.trim().is_empty()).then_some(text))
}

/// Replaces the original selection through Ctrl+V, preserving the user's clipboard.
pub fn replace_selection(target: isize, replacement: &str) -> Result<()> {
    let target = HWND(target as *mut _);
    unsafe {
        let _ = SetForegroundWindow(target);
    }
    thread::sleep(Duration::from_millis(80));
    if unsafe { GetForegroundWindow() } != target {
        return Err(Error::new(
            HRESULT(0x80004005u32 as i32),
            "The original Codex window did not regain focus; replacement was cancelled.",
        ));
    }

    let saved_clipboard = save_clipboard()?;
    if let Err(error) = put_text_on_clipboard(replacement) {
        let _ = restore_clipboard(&saved_clipboard);
        return Err(error);
    }
    send_ctrl_combo(VK_V.0);
    thread::sleep(Duration::from_millis(250));
    restore_clipboard(&saved_clipboard)
}
