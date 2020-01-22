#[macro_use]
extern crate glib;
#[macro_use]
extern crate gstreamer as gst;

extern crate gstreamer_video as gst_video;

#[cfg(not(feature = "v1_18"))]
extern crate glib_sys;
#[cfg(not(feature = "v1_18"))]
extern crate gobject_sys;
#[cfg(feature = "v1_18")]
extern crate gstreamer_base as gst_base;
#[cfg(not(feature = "v1_18"))]
extern crate gstreamer_sys as gst_sys;
#[cfg(not(feature = "v1_18"))]
#[allow(dead_code)]
mod base;
#[cfg(not(feature = "v1_18"))]
mod gst_base {
    pub use super::base::*;
}

extern crate once_cell;

mod custom_compositor;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    custom_compositor::register(plugin)?;
    Ok(())
}

gst_plugin_define!(
    customcompositor,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
    "MIT/X11",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY"),
    env!("BUILD_REL_DATE")
);
