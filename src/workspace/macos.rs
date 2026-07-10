use crate::{
    error::AppError,
    workspace::model::{MonitorId, MonitorInfo},
};

type CGDirectDisplayID = u32;
type CGError = i32;
type CGDisplayCount = u32;
type SizeT = usize;
type CGFloat = f64;

const K_CG_ERROR_SUCCESS: CGError = 0;
const MAX_DISPLAYS: usize = 32;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: CGFloat,
    y: CGFloat,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    width: CGFloat,
    height: CGFloat,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut CGDirectDisplayID,
        display_count: *mut CGDisplayCount,
    ) -> CGError;
    fn CGMainDisplayID() -> CGDirectDisplayID;
    fn CGDisplayPixelsWide(display: CGDirectDisplayID) -> SizeT;
    fn CGDisplayPixelsHigh(display: CGDirectDisplayID) -> SizeT;
    fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
}

pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>, AppError> {
    let mut display_ids = [0_u32; MAX_DISPLAYS];
    let mut display_count = 0_u32;
    let result = unsafe {
        CGGetActiveDisplayList(
            MAX_DISPLAYS as u32,
            display_ids.as_mut_ptr(),
            &mut display_count,
        )
    };

    if result != K_CG_ERROR_SUCCESS {
        return Err(AppError::Workspace(format!(
            "CGGetActiveDisplayList failed with code {result}"
        )));
    }

    let main_display = unsafe { CGMainDisplayID() };
    let monitors = display_ids
        .iter()
        .take(display_count as usize)
        .copied()
        .map(|display_id| monitor_info(display_id, main_display))
        .collect::<Result<Vec<_>, _>>()?;

    if monitors.is_empty() {
        return Err(AppError::Workspace(
            "CoreGraphics returned no active displays".to_string(),
        ));
    }

    Ok(monitors)
}

fn monitor_info(
    display_id: CGDirectDisplayID,
    main_display: CGDirectDisplayID,
) -> Result<MonitorInfo, AppError> {
    let pixel_width = u32::try_from(unsafe { CGDisplayPixelsWide(display_id) })
        .map_err(|_| AppError::Workspace(format!("display {display_id} width is too large")))?;
    let pixel_height = u32::try_from(unsafe { CGDisplayPixelsHigh(display_id) })
        .map_err(|_| AppError::Workspace(format!("display {display_id} height is too large")))?;
    let bounds = unsafe { CGDisplayBounds(display_id) };

    if pixel_width == 0 || pixel_height == 0 {
        return Err(AppError::Workspace(format!(
            "CoreGraphics returned an invalid display mode for display {display_id}"
        )));
    }

    Ok(MonitorInfo {
        id: MonitorId::new(display_id.to_string()),
        device_name: format!("Display {display_id}"),
        is_primary: display_id == main_display,
        pixel_width,
        pixel_height,
        virtual_left: bounds.origin.x.round() as i32,
        virtual_top: bounds.origin.y.round() as i32,
        virtual_right: (bounds.origin.x + bounds.size.width).round() as i32,
        virtual_bottom: (bounds.origin.y + bounds.size.height).round() as i32,
    })
}
