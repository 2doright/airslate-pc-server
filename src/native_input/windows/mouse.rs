use std::{
    cell::Cell,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use windows::{
    Win32::{
        System::LibraryLoader::{GetProcAddress, LoadLibraryA},
        UI::{
            Input::KeyboardAndMouse::{
                MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
                MOUSEEVENTF_VIRTUALDESK,
            },
            WindowsAndMessaging::{
                CreateWindowExW, HWND_MESSAGE, MSG, PM_NOREMOVE, PeekMessageW, WINDOW_EX_STYLE,
                WS_OVERLAPPED,
            },
        },
    },
    core::{PCSTR, w},
};

use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjectionCommandKind, PenInjector},
};

use super::shortcut::absolute_mouse_coords;

type MouseEventFn = unsafe extern "system" fn(u32, i32, i32, u32, usize);

thread_local! {
    static INJECTION_WINDOW: Cell<isize> = const { Cell::new(0) };
}

/// Mouse fallback for applications that only consume mouse input.
///
/// Some Unity player builds expose a Pen device but drive their drawing surface
/// exclusively from Mouse.current (or from the legacy Input mouse API). A synthetic
/// Windows Ink pen never reaches that code path, so this injector emits the absolute
/// mouse move and left-button stream those players already handle.
pub(super) struct WindowsMouseInjector {
    pressed: AtomicBool,
}

impl WindowsMouseInjector {
    pub(super) fn new() -> Self {
        Self {
            pressed: AtomicBool::new(false),
        }
    }
}

impl PenInjector for WindowsMouseInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        ensure_injection_window()?;

        let pressed = self.pressed.load(Ordering::Acquire);
        let (events, next_pressed) = build_events(&command, pressed);
        self.pressed.store(next_pressed, Ordering::Release);

        for event in events {
            dispatch(event);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseEvent {
    Move(i32, i32),
    LeftDown,
    LeftUp,
}

fn build_events(command: &PenInjectionCommand, pressed: bool) -> (Vec<MouseEvent>, bool) {
    let (x, y) = absolute_mouse_coords(command.x, command.y);
    let mut events = vec![MouseEvent::Move(x, y)];

    let next_pressed = match command.kind {
        PenInjectionCommandKind::Down => {
            events.push(MouseEvent::LeftDown);
            true
        }
        PenInjectionCommandKind::Up => {
            events.push(MouseEvent::LeftUp);
            false
        }
        PenInjectionCommandKind::Cancel => {
            if pressed {
                events.push(MouseEvent::LeftUp);
            }
            false
        }
        PenInjectionCommandKind::Update => pressed,
    };

    (events, next_pressed)
}

fn ensure_injection_window() -> Result<(), AppError> {
    INJECTION_WINDOW.with(|slot| {
        if slot.get() != 0 {
            return Ok(());
        }

        // SAFETY: STATIC is a registered system class and HWND_MESSAGE creates a
        // message-only window owned by this thread.
        unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("AirSlate pointer injection"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )?;

            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, Some(hwnd), 0, 0, PM_NOREMOVE);
            slot.set(hwnd.0 as isize);
        }

        Ok(())
    })
}

fn dispatch(event: MouseEvent) {
    // SAFETY: every flag and coordinate is a valid mouse_event parameter.
    unsafe {
        let send = mouse_event_fn();
        match event {
            MouseEvent::Move(x, y) => {
                send(
                    (MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK).0,
                    x,
                    y,
                    0,
                    0,
                );
            }
            MouseEvent::LeftDown => send(MOUSEEVENTF_LEFTDOWN.0, 0, 0, 0, 0),
            MouseEvent::LeftUp => send(MOUSEEVENTF_LEFTUP.0, 0, 0, 0, 0),
        }
    }
}

fn mouse_event_fn() -> MouseEventFn {
    // Resolve the export explicitly: the generated import binding silently drops
    // events on some Windows builds, while the exported function still injects.
    static FUNCTION: OnceLock<MouseEventFn> = OnceLock::new();
    *FUNCTION.get_or_init(|| {
        // SAFETY: user32.dll is loaded for the process lifetime and mouse_event is a
        // documented export with the resolved signature.
        unsafe {
            let library = LoadLibraryA(PCSTR(b"user32.dll\0".as_ptr()))
                .expect("user32.dll should always load");
            let address = GetProcAddress(library, PCSTR(b"mouse_event\0".as_ptr()))
                .expect("user32.dll should export mouse_event");
            std::mem::transmute(address)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(kind: PenInjectionCommandKind) -> PenInjectionCommand {
        PenInjectionCommand {
            x: 1280,
            y: 800,
            tablet_x: 16_000,
            tablet_y: 10_000,
            kind,
            in_range: true,
            is_contact: matches!(
                kind,
                PenInjectionCommandKind::Down | PenInjectionCommandKind::Update
            ),
            pressure: 512,
            tilt_x: 0,
            tilt_y: 0,
        }
    }

    #[test]
    fn down_moves_then_presses_left_button() {
        let (events, pressed) = build_events(&command(PenInjectionCommandKind::Down), false);

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], MouseEvent::Move(_, _)));
        assert_eq!(events[1], MouseEvent::LeftDown);
        assert!(pressed);
    }

    #[test]
    fn up_moves_then_releases_left_button() {
        let (events, pressed) = build_events(&command(PenInjectionCommandKind::Up), true);

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], MouseEvent::Move(_, _)));
        assert_eq!(events[1], MouseEvent::LeftUp);
        assert!(!pressed);
    }

    #[test]
    fn update_never_changes_the_button_state() {
        let (pressed_events, pressed) =
            build_events(&command(PenInjectionCommandKind::Update), true);
        let (hover_events, hover_pressed) =
            build_events(&command(PenInjectionCommandKind::Update), false);

        assert_eq!(pressed_events.len(), 1);
        assert_eq!(hover_events.len(), 1);
        assert!(pressed);
        assert!(!hover_pressed);
    }

    #[test]
    fn cancel_releases_only_when_the_button_is_down() {
        let (pressed_events, pressed) =
            build_events(&command(PenInjectionCommandKind::Cancel), true);
        let (hover_events, hover_pressed) =
            build_events(&command(PenInjectionCommandKind::Cancel), false);

        assert_eq!(pressed_events.len(), 2);
        assert_eq!(hover_events.len(), 1);
        assert!(!pressed);
        assert!(!hover_pressed);
    }
}
