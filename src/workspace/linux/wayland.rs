use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};

use super::{exclusive_edge, positive_extent};
use crate::{
    error::AppError,
    workspace::model::{MonitorId, MonitorInfo},
};

/// `wl_output` batches its state events and terminates the batch with `done`. A compositor may
/// answer the bind in a later frame, so roundtrips repeat until every output is complete.
const MAX_ROUNDTRIPS: usize = 8;
/// `zxdg_output_manager_v1` is at version 3, from which on `zxdg_output_v1::done` is deprecated
/// in favour of `wl_output::done`.
const XDG_OUTPUT_VERSION: u32 = 3;

/// Enumerates the outputs advertised by the compositor.
///
/// `primary_output` is the connector name of the primary output as reported by the X server
/// (`XWayland`), because `wl_output` itself carries no primary flag.
pub(super) fn enumerate_monitors(
    primary_output: Option<&str>,
) -> Result<Vec<MonitorInfo>, AppError> {
    let connection = Connection::connect_to_env().map_err(|error| {
        AppError::Workspace(format!("failed to connect to the Wayland display: {error}"))
    })?;
    let display = connection.display();
    let mut event_queue = connection.new_event_queue::<Registry>();
    let queue_handle = event_queue.handle();
    // The registry outlives the enumeration: destroying it invalidates the bound outputs.
    let _registry = display.get_registry(&queue_handle, ());

    let mut state = Registry::default();
    roundtrip(&mut event_queue, &mut state)?;

    // The manager global can be advertised after the outputs, so its per-output objects are
    // created once the initial registry batch has been processed.
    state.bind_xdg_outputs(&queue_handle);
    roundtrip(&mut event_queue, &mut state)?;

    for _ in 0..MAX_ROUNDTRIPS {
        if state.outputs.iter().all(Output::is_complete) {
            break;
        }
        roundtrip(&mut event_queue, &mut state)?;
    }

    state
        .outputs
        .iter()
        .map(|output| monitor_info(output, primary_output))
        .collect()
}

fn roundtrip(event_queue: &mut EventQueue<Registry>, state: &mut Registry) -> Result<(), AppError> {
    event_queue
        .roundtrip(state)
        .map(|_| ())
        .map_err(|error| AppError::Workspace(format!("Wayland roundtrip failed: {error}")))
}

/// Registry state collecting every `wl_output` global the compositor advertises.
#[derive(Default)]
struct Registry {
    outputs: Vec<Output>,
    manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
}

impl Registry {
    fn bind_xdg_outputs(&mut self, queue_handle: &QueueHandle<Self>) {
        let Some(manager) = self.manager.clone() else {
            return;
        };

        for output in &mut self.outputs {
            if output.xdg.is_none() {
                output.xdg = Some(manager.get_xdg_output(&output.proxy, queue_handle, ()));
            }
        }
    }
}

struct Output {
    /// Registry global name, only used to build a fallback id.
    global_name: u32,
    proxy: wl_output::WlOutput,
    xdg: Option<zxdg_output_v1::ZxdgOutputV1>,
    state: OutputState,
}

impl Output {
    fn is_complete(&self) -> bool {
        self.state.complete
            && (self.xdg.is_none() || self.state.logical_size.is_some() || self.state.xdg_done)
    }
}

#[derive(Default)]
struct OutputState {
    /// Connector name, e.g. `HDMI-1` (`wl_output` v4 or `xdg_output` v2).
    name: Option<String>,
    /// Human readable name, e.g. `AOC 27"` (`wl_output` v4 or `xdg_output` v2).
    description: Option<String>,
    make: String,
    model: String,
    /// Position in the global compositor space (`wl_output` v1..v3).
    position: (i32, i32),
    /// Current mode in device pixels.
    mode: Option<(i32, i32)>,
    scale: i32,
    rotated: bool,
    /// Position in the global compositor space (`xdg_output`).
    logical_position: Option<(i32, i32)>,
    /// Size in the global compositor space (`xdg_output`).
    logical_size: Option<(i32, i32)>,
    xdg_done: bool,
    complete: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Registry {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        if interface == wl_output::WlOutput::interface().name {
            // Version 4 is the only source of a connector name; the bind never exceeds the
            // version the compositor advertised.
            let proxy =
                registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), queue_handle, ());
            state.outputs.push(Output {
                global_name: name,
                proxy,
                xdg: None,
                state: OutputState::default(),
            });
        } else if interface == zxdg_output_manager_v1::ZxdgOutputManagerV1::interface().name
            && state.manager.is_none()
        {
            state.manager = Some(
                registry.bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, _, _>(
                    name,
                    version.min(XDG_OUTPUT_VERSION),
                    queue_handle,
                    (),
                ),
            );
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Registry {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| &output.proxy == proxy)
        else {
            return;
        };

        match event {
            wl_output::Event::Geometry {
                x,
                y,
                make,
                model,
                transform,
                ..
            } => {
                output.state.position = (x, y);
                output.state.make = make;
                output.state.model = model;
                output.state.rotated = swaps_axes(&transform);
            }
            wl_output::Event::Mode {
                width,
                height,
                flags,
                ..
            } => {
                if is_current(&flags) {
                    output.state.mode = Some((width, height));
                }
            }
            wl_output::Event::Scale { factor } => output.state.scale = factor,
            wl_output::Event::Name { name } => output.state.name = Some(name),
            wl_output::Event::Description { description } => {
                output.state.description = Some(description);
            }
            wl_output::Event::Done => output.state.complete = true,
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, ()> for Registry {
    fn event(
        state: &mut Self,
        proxy: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| output.xdg.as_ref() == Some(proxy))
        else {
            return;
        };

        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                output.state.logical_position = Some((x, y));
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                output.state.logical_size = Some((width, height));
            }
            zxdg_output_v1::Event::Name { name } => {
                output.state.name.get_or_insert(name);
            }
            zxdg_output_v1::Event::Description { description } => {
                output.state.description.get_or_insert(description);
            }
            zxdg_output_v1::Event::Done => output.state.xdg_done = true,
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for Registry {
    fn event(
        _: &mut Self,
        _: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        _: zxdg_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn is_current(flags: &WEnum<wl_output::Mode>) -> bool {
    matches!(flags, WEnum::Value(flags) if flags.contains(wl_output::Mode::Current))
}

fn swaps_axes(transform: &WEnum<wl_output::Transform>) -> bool {
    matches!(
        transform,
        WEnum::Value(
            wl_output::Transform::_90
                | wl_output::Transform::_270
                | wl_output::Transform::Flipped90
                | wl_output::Transform::Flipped270
        )
    )
}

fn monitor_info(output: &Output, primary_output: Option<&str>) -> Result<MonitorInfo, AppError> {
    let state = &output.state;
    let (left, top) = state.logical_position.unwrap_or(state.position);
    let (width, height) = logical_size(state)?;
    let pixel_width = positive_extent(width, "width")?;
    let pixel_height = positive_extent(height, "height")?;

    let id = state
        .name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("output-{}", output.global_name));
    let device_name = state
        .description
        .clone()
        .filter(|description| !description.is_empty())
        .or_else(|| {
            let label = format!("{} {}", state.make, state.model);
            (!label.trim().is_empty()).then_some(label)
        })
        .unwrap_or_else(|| id.clone());

    Ok(MonitorInfo {
        id: MonitorId::new(id.clone()),
        device_name,
        is_primary: primary_output == Some(id.as_str()),
        pixel_width,
        pixel_height,
        virtual_left: left,
        virtual_top: top,
        virtual_right: exclusive_edge(left, pixel_width)?,
        virtual_bottom: exclusive_edge(top, pixel_height)?,
    })
}

/// Resolves the output size in the global compositor space.
fn logical_size(state: &OutputState) -> Result<(i32, i32), AppError> {
    if let Some(size) = state.logical_size {
        return Ok(size);
    }

    // Without `xdg_output` the only size is the mode in device pixels, and the scale is an
    // integer, so the layout size is the mode divided by that scale.
    let (mode_width, mode_height) = state.mode.ok_or_else(|| {
        AppError::Workspace("Wayland output reported no current mode".to_string())
    })?;
    let scale = state.scale.max(1);

    Ok(if state.rotated {
        (mode_height / scale, mode_width / scale)
    } else {
        (mode_width / scale, mode_height / scale)
    })
}
