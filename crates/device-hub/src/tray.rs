//! システムトレイ常駐。
//!
//! - 常にトレイ アイコンを出す (左クリック / 「Device HUB を開く」で復帰、右クリックでメニュー)
//! - **最小化したらタスクトレイに格納**する: 窓を `SW_HIDE` してタスクバーから消し、
//!   トレイから戻せるようにする
//! - × で閉じたときはそのまま終了 (常駐を続けたい場合は最小化を使う)
//!
//! トレイは専用スレッドで Win32 のメッセージループを回す (`tray-icon` は
//! 同じスレッドでイベントループが動いている前提)。
//! トレイの作成に失敗した場合、最小化で窓を消すと復帰できなくなるので**隠さない**。

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    use gpui_kit::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, IsIconic, PostMessageW, SetForegroundWindow, ShowWindow,
        TranslateMessage, MSG, SW_HIDE, SW_RESTORE, SW_SHOW, WM_CLOSE,
    };

    use windows_sys::Win32::Foundation::HWND;

    /// トレイ アイコンの用意ができたか。できるまでは最小化しても隠さない。
    static TRAY_READY: AtomicBool = AtomicBool::new(false);
    static STARTED: AtomicBool = AtomicBool::new(false);

    /// 窓の HWND (Win32)。
    fn hwnd_of(window: &Window) -> Option<isize> {
        // 固有メソッド `Window::window_handle()` は GPUI のハンドルを返すので、
        // raw-window-handle の実装を明示的に呼ぶ。
        let handle = HasWindowHandle::window_handle(window).ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
            _ => None,
        }
    }

    /// トレイを出す + 最小化をトレイ格納に変える。窓を作った直後に 1 回だけ呼ぶ。
    pub fn install(window: &mut Window, _cx: &mut App) {
        // 最小化 (HMENU の可視状態が Hidden) → タスクバーから消す。
        window
            .observe_window_visibility(|visibility, window, _cx| {
                if visibility.is_visible() || !TRAY_READY.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(hwnd) = hwnd_of(window) {
                    // 最小化のまま `SW_HIDE` すると、タスクバーのボタンが消えて
                    // トレイからだけ戻せる状態になる。
                    unsafe { ShowWindow(hwnd as HWND, SW_HIDE) };
                }
            })
            .detach();

        let Some(hwnd) = hwnd_of(window) else {
            return;
        };
        if STARTED.swap(true, Ordering::Relaxed) {
            return;
        }
        thread::Builder::new()
            .name("devhub-tray".into())
            .spawn(move || run(hwnd))
            .ok();
    }

    /// 窓を前面に出す (トレイの左クリック / メニューから)。
    fn show(hwnd: isize) {
        let hwnd = hwnd as HWND;
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }

    fn run(hwnd: isize) {
        let Some(icon) = tray_image() else { return };
        let Some((menu, open_id, quit_id)) = build_menu() else {
            return;
        };

        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(hwnd);
            }
        }));
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == quit_id {
                // 通常の × と同じ経路 (アプリ側の終了処理を通す)。
                unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) };
            } else if event.id == open_id {
                show(hwnd);
            }
        }));

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Device HUB")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        let Ok(_tray) = tray else {
            TRAY_READY.store(false, Ordering::Relaxed);
            return;
        };
        TRAY_READY.store(true, Ordering::Relaxed);

        // tray-icon (Windows) はこのスレッドのメッセージループでイベントを配る。
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        TRAY_READY.store(false, Ordering::Relaxed);
    }

    /// メニューと、項目の ID。
    fn build_menu() -> Option<(Menu, tray_icon::menu::MenuId, tray_icon::menu::MenuId)> {
        let open = MenuItem::new("Device HUB を開く", true, None);
        let quit = MenuItem::new("終了", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit])
            .ok()?;
        Some((menu, open.id().clone(), quit.id().clone()))
    }

    /// 同梱アイコンを 32px に落としてトレイ用にする。
    fn tray_image() -> Option<tray_icon::Icon> {
        const PNG: &[u8] = include_bytes!("../assets/icon.png");
        let source = image::load_from_memory_with_format(PNG, image::ImageFormat::Png).ok()?;
        let rgba = image::imageops::resize(
            &source.to_rgba8(),
            32,
            32,
            image::imageops::FilterType::Lanczos3,
        );
        tray_icon::Icon::from_rgba(rgba.into_raw(), 32, 32).ok()
    }
}

#[cfg(windows)]
pub use imp::install;

/// Windows 以外では何もしない (トレイは Win32 専用)。
#[cfg(not(windows))]
pub fn install(_window: &mut gpui_kit::Window, _cx: &mut gpui_kit::App) {}
