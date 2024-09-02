use gst::glib;
use gst::prelude::*;

mod imp;

glib::wrapper! {
    pub struct CustomCompositor(ObjectSubclass<imp::CustomCompositor>) @extends gst_base::Aggregator, gst::Element, gst::Object, @implements gst::ChildProxy;
}

glib::wrapper! {
    pub struct CustomCompositorPad(ObjectSubclass<imp::CustomCompositorPad>) @extends gst_base::AggregatorPad, gst::Pad, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "custom-compositor",
        gst::Rank::NONE,
        CustomCompositor::static_type(),
    )
}
