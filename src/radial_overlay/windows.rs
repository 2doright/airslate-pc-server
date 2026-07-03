use std::{
    ffi::c_void,
    mem::size_of,
    ptr::null_mut,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::Duration,
};

use windows::core::Interface;
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FILL_MODE_WINDING,
                },
                D2D1_ARC_SEGMENT, D2D1_DEBUG_LEVEL_NONE, D2D1_ELLIPSE, D2D1_FACTORY_OPTIONS,
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
                D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE, D2D1_SWEEP_DIRECTION_CLOCKWISE,
                D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_BOLD, DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::{
                AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
                CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
                HBITMAP, HDC, HGDIOBJ, RGBQUAD, SelectObject,
            },
        },
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
            LibraryLoader::GetModuleHandleW,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_TOPMOST,
            IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, SW_HIDE,
            SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SetWindowPos, ShowWindow, ULW_ALPHA,
            UpdateLayeredWindow, WM_QUIT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
    core::{PCWSTR, w},
};
use windows_numerics::Vector2;

use tracing::info;

use crate::{
    error::AppError,
    radial_overlay::OverlayCommand,
    shortcut::{RadialInnerSlot, RadialMenuOverlayState, RadialSelection, ScreenPoint},
};

const WINDOW_CLASS: PCWSTR = w!("AirSlateRadialOverlay");
const WINDOW_TITLE: PCWSTR = w!("AirSlateRadialOverlayHost");
const OVERLAY_SIZE: i32 = 440;
const HOLD_INDICATOR_SIZE: i32 = 48;
const HOLD_INDICATOR_RADIUS: f32 = 5.0;
const HOLD_INDICATOR_OFFSET_X: i32 = 14;
const HOLD_INDICATOR_OFFSET_Y: i32 = -14;
const CENTER_RADIUS: f32 = 26.0;
const INNER_RING_INNER_RADIUS: f32 = 34.0;
const INNER_RING_OUTER_RADIUS: f32 = 132.0;
const OUTER_RING_INNER_RADIUS: f32 = 132.0;
const OUTER_RING_OUTER_RADIUS: f32 = 212.0;
const INNER_LABEL_RADIUS: f32 = 88.0;
const OUTER_LABEL_RADIUS: f32 = 172.0;

fn outer_ring_inner_radius(inner_enabled: bool) -> f32 {
    if inner_enabled {
        OUTER_RING_INNER_RADIUS
    } else {
        INNER_RING_INNER_RADIUS
    }
}

pub(crate) fn run_overlay_thread(receiver: Receiver<OverlayCommand>) -> Result<(), AppError> {
    let mut overlay = OverlayThread::new(receiver)?;
    overlay.run()
}

struct OverlayThread {
    receiver: Receiver<OverlayCommand>,
    hwnd: HWND,
    render_target: ID2D1DCRenderTarget,
    label_format: IDWriteTextFormat,
    memory_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    rect: RECT,
    visible: bool,
    radial_state: Option<RadialMenuOverlayState>,
    hold_indicator_point: Option<ScreenPoint>,
    com_initialized: bool,
}

impl OverlayThread {
    fn new(receiver: Receiver<OverlayCommand>) -> Result<Self, AppError> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
        let com_initialized = true;

        let instance = unsafe { GetModuleHandleW(None)? };
        register_window_class(instance.into())?;
        let hwnd = create_host_window(instance.into())?;

        let d2d_factory: ID2D1Factory = unsafe {
            D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS {
                    debugLevel: D2D1_DEBUG_LEVEL_NONE,
                }),
            )?
        };
        let dwrite_factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        let render_target = unsafe {
            d2d_factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: windows::Win32::Graphics::Direct2D::Common::D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            })?
        };
        let label_format = unsafe {
            let format = dwrite_factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                22.0,
                w!("zh-cn"),
            )?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format
        };

        let memory_dc = unsafe { CreateCompatibleDC(None) };
        if memory_dc.0.is_null() {
            return Err(AppError::Io(std::io::Error::last_os_error()));
        }

        let mut bits = null_mut::<c_void>();
        let bitmap = unsafe {
            CreateDIBSection(
                None,
                &bitmap_info(OVERLAY_SIZE, OVERLAY_SIZE),
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )?
        };
        let old_bitmap = unsafe { SelectObject(memory_dc, HGDIOBJ(bitmap.0)) };
        let rect = RECT {
            left: 0,
            top: 0,
            right: OVERLAY_SIZE,
            bottom: OVERLAY_SIZE,
        };

        Ok(Self {
            receiver,
            hwnd,
            render_target,
            label_format,
            memory_dc,
            bitmap,
            old_bitmap,
            rect,
            visible: false,
            radial_state: None,
            hold_indicator_point: None,
            com_initialized,
        })
    }

    fn run(&mut self) -> Result<(), AppError> {
        loop {
            if self.pump_window_messages()? {
                return Ok(());
            }

            match self.receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(command) => {
                    if self.handle_command(command)? {
                        return Ok(());
                    }
                    while let Ok(command) = self.receiver.try_recv() {
                        if self.handle_command(command)? {
                            return Ok(());
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    fn pump_window_messages(&mut self) -> Result<bool, AppError> {
        let mut message = MSG::default();
        loop {
            let has_message =
                unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool();
            if !has_message {
                return Ok(false);
            }
            if message.message == WM_QUIT {
                return Ok(true);
            }
            unsafe {
                DispatchMessageW(&message);
            }
        }
    }

    fn handle_command(&mut self, command: OverlayCommand) -> Result<bool, AppError> {
        match command {
            OverlayCommand::Show(state) | OverlayCommand::Update(state) => {
                info!(x = state.anchor.center.x, y = state.anchor.center.y, selection = ?state.selection, "rendering radial menu overlay");
                self.radial_state = Some(state);
                self.render()?;
                Ok(false)
            }
            OverlayCommand::Hide => {
                info!("hiding radial menu overlay");
                self.radial_state = None;
                self.render()?;
                Ok(false)
            }
            OverlayCommand::SyncHoldIndicator(point) => {
                self.hold_indicator_point = point;
                self.render()?;
                Ok(false)
            }
            OverlayCommand::Shutdown => Ok(true),
        }
    }

    fn render(&mut self) -> Result<(), AppError> {
        let bounds = overlay_bounds(self.radial_state.as_ref(), self.hold_indicator_point);
        let Some((x, y, width, height)) = bounds else {
            self.hide();
            return Ok(());
        };
        self.rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };

        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            )?;
            self.render_target.BindDC(self.memory_dc, &self.rect)?;
            self.render_target.BeginDraw();
            self.render_target.Clear(Some(&color(0.0, 0.0, 0.0, 0.0)));

            if let Some(state) = self.radial_state.as_ref() {
                let ring_fill_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.13, 0.15, 0.19, 0.965), None)?;
                let outline_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.78, 0.84, 0.98, 0.16), None)?;
                let divider_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.78, 0.84, 0.98, 0.08), None)?;
                let selected_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.86, 0.62, 0.28, 0.34), None)?;
                let selected_edge_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.95, 0.73, 0.36, 0.58), None)?;
                let center_selected_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.86, 0.62, 0.28, 0.34), None)?;
                let center_selected_edge_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.95, 0.73, 0.36, 0.58), None)?;
                let toggled_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.86, 0.62, 0.28, 0.34), None)?;
                let toggled_edge_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.95, 0.73, 0.36, 0.58), None)?;
                let text_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.975, 0.982, 1.0, 0.985), None)?;
                let text_dim_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.86, 0.89, 0.95, 0.96), None)?;
                let center_gap_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 0.995), None)?;
                let center_brush = self
                    .render_target
                    .CreateSolidColorBrush(&color(0.05, 0.07, 0.10, 0.985), None)?;

                let center = v2(
                    (state.anchor.center.x - x) as f32,
                    (state.anchor.center.y - y) as f32,
                );

                let inner_enabled = state.config.inner_enabled;
                if inner_enabled {
                    draw_ring_band(
                        &self.render_target,
                        center,
                        INNER_RING_INNER_RADIUS,
                        INNER_RING_OUTER_RADIUS,
                        &ring_fill_brush,
                    )?;
                    draw_inner_glow_ring(&self.render_target, center, &selected_edge_brush)?;
                }
                draw_ring_band(
                    &self.render_target,
                    center,
                    outer_ring_inner_radius(inner_enabled),
                    OUTER_RING_OUTER_RADIUS,
                    &ring_fill_brush,
                )?;
                draw_ring_outlines(&self.render_target, center, &outline_brush, inner_enabled)?;
                self.render_target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: center,
                        radiusX: INNER_RING_INNER_RADIUS,
                        radiusY: INNER_RING_INNER_RADIUS,
                    },
                    &center_gap_brush,
                );
                self.render_target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: center,
                        radiusX: CENTER_RADIUS,
                        radiusY: CENTER_RADIUS,
                    },
                    &center_brush,
                );

                if inner_enabled {
                    for slot in &state.active_inner_slots {
                        draw_inner_sector(&self.render_target, center, *slot, &toggled_brush)?;
                        draw_inner_sector_edge(
                            &self.render_target,
                            center,
                            *slot,
                            &toggled_edge_brush,
                        )?;
                    }
                }

                match state.selection {
                    RadialSelection::Inner(slot) if inner_enabled => {
                        draw_inner_sector(&self.render_target, center, slot, &selected_brush)?;
                        draw_inner_sector_edge(
                            &self.render_target,
                            center,
                            slot,
                            &selected_edge_brush,
                        )?;
                    }
                    RadialSelection::Outer(index) => {
                        draw_outer_sector(
                            &self.render_target,
                            center,
                            index,
                            &selected_brush,
                            inner_enabled,
                        )?;
                        draw_outer_sector_edge(
                            &self.render_target,
                            center,
                            index,
                            &selected_edge_brush,
                            inner_enabled,
                        )?;
                    }
                    RadialSelection::Inner(_) => {}
                    RadialSelection::Center => {
                        self.render_target.FillEllipse(
                            &D2D1_ELLIPSE {
                                point: center,
                                radiusX: CENTER_RADIUS,
                                radiusY: CENTER_RADIUS,
                            },
                            &center_selected_brush,
                        );
                        self.render_target.DrawEllipse(
                            &D2D1_ELLIPSE {
                                point: center,
                                radiusX: CENTER_RADIUS,
                                radiusY: CENTER_RADIUS,
                            },
                            &center_selected_edge_brush,
                            1.8,
                            None,
                        );
                    }
                }

                draw_ring_dividers(&self.render_target, center, &divider_brush, inner_enabled)?;
                draw_labels(
                    &self.render_target,
                    &self.label_format,
                    &text_brush,
                    &text_dim_brush,
                    center,
                    state,
                )?;
            }

            if let Some(point) = self.hold_indicator_point {
                draw_hold_indicator(&self.render_target, point, x, y)?;
            }

            self.render_target.EndDraw(None, None)?;
            present_layered(self.hwnd, self.memory_dc, x, y, width, height)?;
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            self.visible = true;
        }

        Ok(())
    }

    fn hide(&mut self) {
        if !self.visible {
            return;
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
    }
}

impl Drop for OverlayThread {
    fn drop(&mut self) {
        self.hide();
        unsafe {
            let _ = SelectObject(self.memory_dc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.memory_dc);
            let _ = DestroyWindow(self.hwnd);
            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

fn overlay_bounds(
    radial_state: Option<&RadialMenuOverlayState>,
    hold_indicator_point: Option<ScreenPoint>,
) -> Option<(i32, i32, i32, i32)> {
    if let Some(state) = radial_state {
        return Some((
            state.anchor.center.x - (OVERLAY_SIZE / 2),
            state.anchor.center.y - (OVERLAY_SIZE / 2),
            OVERLAY_SIZE,
            OVERLAY_SIZE,
        ));
    }

    hold_indicator_point.map(|point| {
        (
            point.x + HOLD_INDICATOR_OFFSET_X - (HOLD_INDICATOR_SIZE / 2),
            point.y + HOLD_INDICATOR_OFFSET_Y - (HOLD_INDICATOR_SIZE / 2),
            HOLD_INDICATOR_SIZE,
            HOLD_INDICATOR_SIZE,
        )
    })
}

fn draw_hold_indicator(
    target: &ID2D1DCRenderTarget,
    point: ScreenPoint,
    origin_x: i32,
    origin_y: i32,
) -> Result<(), AppError> {
    let glow_brush = unsafe { target.CreateSolidColorBrush(&color(1.0, 0.24, 0.24, 0.28), None)? };
    let fill_brush = unsafe { target.CreateSolidColorBrush(&color(1.0, 0.22, 0.22, 0.94), None)? };
    let edge_brush = unsafe { target.CreateSolidColorBrush(&color(1.0, 0.74, 0.74, 0.95), None)? };
    let center = v2(
        (point.x + HOLD_INDICATOR_OFFSET_X - origin_x) as f32,
        (point.y + HOLD_INDICATOR_OFFSET_Y - origin_y) as f32,
    );

    unsafe {
        target.FillEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: HOLD_INDICATOR_RADIUS + 3.0,
                radiusY: HOLD_INDICATOR_RADIUS + 3.0,
            },
            &glow_brush,
        );
        target.FillEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: HOLD_INDICATOR_RADIUS,
                radiusY: HOLD_INDICATOR_RADIUS,
            },
            &fill_brush,
        );
        target.DrawEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: HOLD_INDICATOR_RADIUS,
                radiusY: HOLD_INDICATOR_RADIUS,
            },
            &edge_brush,
            1.0,
            None,
        );
    }

    Ok(())
}

fn register_window_class(instance: HINSTANCE) -> Result<(), AppError> {
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW)? };
    let class = WNDCLASSW {
        hCursor: cursor,
        hInstance: instance,
        lpszClassName: WINDOW_CLASS,
        lpfnWndProc: Some(window_proc),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassW(&class) };
    if atom == 0 {
        return Err(AppError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn create_host_window(instance: HINSTANCE) -> Result<HWND, AppError> {
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
            WINDOW_CLASS,
            WINDOW_TITLE,
            WS_POPUP,
            0,
            0,
            OVERLAY_SIZE,
            OVERLAY_SIZE,
            None,
            None,
            Some(instance),
            None,
        )?
    };
    Ok(hwnd)
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe fn present_layered(
    hwnd: HWND,
    src_dc: HDC,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), AppError> {
    let dst = POINT { x, y };
    let src = POINT { x: 0, y: 0 };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    unsafe {
        UpdateLayeredWindow(
            hwnd,
            None,
            Some(&dst),
            Some(&size),
            Some(src_dc),
            Some(&src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )?
    };
    Ok(())
}

fn draw_ring_band(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    inner_radius: f32,
    outer_radius: f32,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) -> Result<(), AppError> {
    let stroke_width = outer_radius - inner_radius;
    let radius = inner_radius + stroke_width / 2.0;
    unsafe {
        target.DrawEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: radius,
                radiusY: radius,
            },
            brush,
            stroke_width,
            None,
        );
    }
    Ok(())
}

fn draw_ring_dividers(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    inner_enabled: bool,
) -> Result<(), AppError> {
    unsafe {
        if inner_enabled {
            for angle in [45.0_f32, 135.0, 225.0, 315.0] {
                let (inner, outer) = radial_line(
                    center,
                    INNER_RING_INNER_RADIUS,
                    INNER_RING_OUTER_RADIUS,
                    angle,
                );
                target.DrawLine(inner, outer, brush, 1.4, None);
            }
        }
        for index in 0..8 {
            let angle = index as f32 * 45.0 + 22.5;
            let (inner, outer) = radial_line(
                center,
                outer_ring_inner_radius(inner_enabled),
                OUTER_RING_OUTER_RADIUS,
                angle,
            );
            target.DrawLine(inner, outer, brush, 1.2, None);
        }
    }
    Ok(())
}

fn radial_line(center: Vector2, r0: f32, r1: f32, angle_deg: f32) -> (Vector2, Vector2) {
    let radians = (angle_deg - 90.0).to_radians();
    let dx = radians.cos();
    let dy = radians.sin();
    (
        v2(center.X + dx * r0, center.Y + dy * r0),
        v2(center.X + dx * r1, center.Y + dy * r1),
    )
}

fn draw_ring_outlines(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    inner_enabled: bool,
) -> Result<(), AppError> {
    unsafe {
        target.DrawEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: CENTER_RADIUS,
                radiusY: CENTER_RADIUS,
            },
            brush,
            1.1,
            None,
        );
        if inner_enabled {
            target.DrawEllipse(
                &D2D1_ELLIPSE {
                    point: center,
                    radiusX: INNER_RING_OUTER_RADIUS,
                    radiusY: INNER_RING_OUTER_RADIUS,
                },
                brush,
                1.1,
                None,
            );
        }
        target.DrawEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: OUTER_RING_OUTER_RADIUS,
                radiusY: OUTER_RING_OUTER_RADIUS,
            },
            brush,
            1.1,
            None,
        );
    }
    Ok(())
}

fn draw_inner_glow_ring(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) -> Result<(), AppError> {
    unsafe {
        target.DrawEllipse(
            &D2D1_ELLIPSE {
                point: center,
                radiusX: INNER_RING_OUTER_RADIUS - 1.5,
                radiusY: INNER_RING_OUTER_RADIUS - 1.5,
            },
            brush,
            1.4,
            None,
        );
    }
    Ok(())
}

fn draw_inner_sector(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    slot: RadialInnerSlot,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) -> Result<(), AppError> {
    let index = match slot {
        RadialInnerSlot::Top => 0,
        RadialInnerSlot::Right => 1,
        RadialInnerSlot::Bottom => 2,
        RadialInnerSlot::Left => 3,
    };
    draw_sector(
        target,
        center,
        INNER_RING_INNER_RADIUS,
        INNER_RING_OUTER_RADIUS,
        index,
        4,
        brush,
    )
}

fn draw_inner_sector_edge(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    slot: RadialInnerSlot,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) -> Result<(), AppError> {
    let index = match slot {
        RadialInnerSlot::Top => 0,
        RadialInnerSlot::Right => 1,
        RadialInnerSlot::Bottom => 2,
        RadialInnerSlot::Left => 3,
    };
    draw_sector_edge(
        target,
        center,
        INNER_RING_INNER_RADIUS,
        INNER_RING_OUTER_RADIUS,
        index,
        4,
        brush,
    )
}

fn draw_outer_sector(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    index: usize,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    inner_enabled: bool,
) -> Result<(), AppError> {
    draw_sector(
        target,
        center,
        outer_ring_inner_radius(inner_enabled),
        OUTER_RING_OUTER_RADIUS,
        index,
        8,
        brush,
    )
}

fn draw_outer_sector_edge(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    index: usize,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    inner_enabled: bool,
) -> Result<(), AppError> {
    draw_sector_edge(
        target,
        center,
        outer_ring_inner_radius(inner_enabled),
        OUTER_RING_OUTER_RADIUS,
        index,
        8,
        brush,
    )
}

fn draw_sector(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    inner_radius: f32,
    outer_radius: f32,
    index: usize,
    segment_count: usize,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) -> Result<(), AppError> {
    let geometry = create_sector_geometry(
        target,
        center,
        inner_radius,
        outer_radius,
        index,
        segment_count,
    )?;
    unsafe {
        target.FillGeometry(&geometry, brush, None);
    }
    Ok(())
}

fn draw_sector_edge(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    inner_radius: f32,
    outer_radius: f32,
    index: usize,
    segment_count: usize,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) -> Result<(), AppError> {
    let geometry = create_sector_geometry(
        target,
        center,
        inner_radius,
        outer_radius,
        index,
        segment_count,
    )?;
    unsafe {
        target.DrawGeometry(&geometry, brush, 1.6, None);
    }
    Ok(())
}

fn create_sector_geometry(
    target: &ID2D1DCRenderTarget,
    center: Vector2,
    inner_radius: f32,
    outer_radius: f32,
    index: usize,
    segment_count: usize,
) -> Result<windows::Win32::Graphics::Direct2D::ID2D1PathGeometry, AppError> {
    let factory: ID2D1Factory = unsafe { target.GetFactory()?.cast()? };
    let geometry = unsafe { factory.CreatePathGeometry()? };
    let sink = unsafe { geometry.Open()? };

    let step = 360.0 / segment_count as f32;
    let start = index as f32 * step - step / 2.0;
    let end = start + step;
    let outer_start = polar_point(center, outer_radius, start);
    let outer_end = polar_point(center, outer_radius, end);
    let inner_end = polar_point(center, inner_radius, end);
    let inner_start = polar_point(center, inner_radius, start);

    unsafe {
        sink.SetFillMode(D2D1_FILL_MODE_WINDING);
        sink.SetSegmentFlags(windows::Win32::Graphics::Direct2D::Common::D2D1_PATH_SEGMENT_NONE);
        sink.BeginFigure(outer_start, D2D1_FIGURE_BEGIN_FILLED);
        sink.AddArc(&arc_segment(outer_end, outer_radius));
        sink.AddLine(inner_end);
        sink.AddArc(&arc_segment(inner_start, inner_radius));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
    }

    Ok(geometry)
}

fn arc_segment(point: Vector2, radius: f32) -> D2D1_ARC_SEGMENT {
    D2D1_ARC_SEGMENT {
        point,
        size: windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F {
            width: radius,
            height: radius,
        },
        rotationAngle: 0.0,
        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
        arcSize: windows::Win32::Graphics::Direct2D::D2D1_ARC_SIZE_SMALL,
    }
}

fn polar_point(center: Vector2, radius: f32, angle_deg: f32) -> Vector2 {
    let radians = (angle_deg - 90.0).to_radians();
    v2(
        center.X + radians.cos() * radius,
        center.Y + radians.sin() * radius,
    )
}

fn draw_labels(
    target: &ID2D1DCRenderTarget,
    format: &IDWriteTextFormat,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    dim_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    center: Vector2,
    state: &RadialMenuOverlayState,
) -> Result<(), AppError> {
    unsafe {
        if state.config.inner_enabled {
            let inner = &state.config.inner;
            draw_text(
                target,
                format,
                brush,
                inner.top.label(),
                text_rect(
                    center.X - 32.0,
                    center.Y - INNER_LABEL_RADIUS - 19.0,
                    64.0,
                    34.0,
                ),
            )?;
            draw_text(
                target,
                format,
                brush,
                inner.bottom.label(),
                text_rect(
                    center.X - 40.0,
                    center.Y + INNER_LABEL_RADIUS - 17.0,
                    80.0,
                    34.0,
                ),
            )?;
            draw_text(
                target,
                format,
                brush,
                inner.left.label(),
                text_rect(
                    center.X - INNER_LABEL_RADIUS - 48.0,
                    center.Y - 17.0,
                    96.0,
                    34.0,
                ),
            )?;
            draw_text(
                target,
                format,
                brush,
                inner.right.label(),
                text_rect(
                    center.X + INNER_LABEL_RADIUS - 34.0,
                    center.Y - 17.0,
                    68.0,
                    34.0,
                ),
            )?;
        }

        for (index, binding) in state.config.outer.iter().enumerate() {
            let angle = (index as f32 * 45.0 - 90.0).to_radians();
            let x = center.X + angle.cos() * OUTER_LABEL_RADIUS;
            let y = center.Y + angle.sin() * OUTER_LABEL_RADIUS;
            let label = binding
                .keys
                .iter()
                .map(key_label)
                .collect::<Vec<_>>()
                .join("+");
            draw_text(
                target,
                format,
                dim_brush,
                &label,
                text_rect(x - 54.0, y - 17.0, 108.0, 34.0),
            )?;
        }
    }
    Ok(())
}

unsafe fn draw_text(
    target: &ID2D1DCRenderTarget,
    format: &IDWriteTextFormat,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    text: &str,
    rect: D2D_RECT_F,
) -> Result<(), AppError> {
    let wide = text.encode_utf16().collect::<Vec<_>>();
    unsafe {
        target.DrawText(
            &wide,
            format,
            &rect,
            brush,
            windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
    Ok(())
}

fn text_rect(x: f32, y: f32, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        bmiColors: [RGBQUAD::default(); 1],
    }
}

fn key_label(key: &crate::shortcut::KeyCode) -> &'static str {
    key.label()
}
