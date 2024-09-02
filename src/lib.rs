use gst::glib;

mod custom_compositor;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    custom_compositor::register(plugin)?;
    Ok(())
}

gst::plugin_define!(
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
