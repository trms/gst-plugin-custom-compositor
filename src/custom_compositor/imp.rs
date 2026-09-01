use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base;
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;
use gst_video::prelude::*;

use std::i32;
use std::sync::Mutex;

use once_cell::sync::Lazy;

// FIXME: Does not enforce that all inputs/outputs have the same pixel aspect ratio

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "custom-compositor",
        gst::DebugColorFlags::empty(),
        Some("Custom Compositor Element"),
    )
});

struct State {
    // Negotiated downstream caps
    info: gst_video::VideoInfo,
    start_time: gst::ClockTime,
    num_frames: u64,
}

#[derive(Default)]
pub struct CustomCompositor {
    // State once we're negotiated with downstream
    state: Mutex<Option<State>>,
}

fn composite_uyvy(
    alpha: f64,
    out_data: &mut [u8],
    out_stride: usize,
    in_data: &[u8],
    in_stride: usize,
    copy_rows: usize,
    copy_cols: usize,
) {
    if alpha == 1.0 {
        for (out_line, in_line) in out_data
            .chunks_exact_mut(out_stride)
            .take(copy_rows)
            .zip(in_data.chunks_exact(in_stride))
        {
            let out_line = &mut out_line[..(copy_cols * 2)];
            let in_line = &in_line[..(copy_cols * 2)];
            out_line.copy_from_slice(in_line);
        }
    } else {
        let alpha = (alpha * 255.0) as u16;

        for (out_line, in_line) in out_data
            .chunks_exact_mut(out_stride)
            .take(copy_rows)
            .zip(in_data.chunks_exact(in_stride))
        {
            let out_line = &mut out_line[..(copy_cols * 2)];
            let in_line = &in_line[..(copy_cols * 2)];

            for (o, i) in out_line.iter_mut().zip(in_line.iter()) {
                let o_v = *o as u16;
                let i_v = *i as u16;

                *o = ((alpha * i_v + (255 - alpha) * o_v) / 255) as u8;
            }
        }
    }
}

// FIXME: Requires input/output to have an even number of pixels, otherwise the last pixel is
// simply skipped
fn composite_ayuv(
    alpha: f64,
    out_data: &mut [u8],
    out_stride: usize,
    in_data: &[u8],
    in_stride: usize,
    copy_rows: usize,
    copy_cols: usize,
) {
    if alpha == 1.0 {
        for (out_line, in_line) in out_data
            .chunks_exact_mut(out_stride)
            .take(copy_rows)
            .zip(in_data.chunks_exact(in_stride))
        {
            let out_line = &mut out_line[..(copy_cols * 2)];
            let in_line = &in_line[..(copy_cols * 4)];

            for (o, i) in out_line.chunks_exact_mut(4).zip(in_line.chunks_exact(8)) {
                // o =   UY   VY
                // i = AYUV AYUV
                let o_u = o[0] as u16;
                let o_y1 = o[1] as u16;
                let o_v = o[2] as u16;
                let o_y2 = o[3] as u16;

                let i_a1 = i[0] as u16;
                let i_y1 = i[1] as u16;
                let i_u1 = i[2] as u16;
                let i_v1 = i[3] as u16;
                let i_a2 = i[4] as u16;
                let i_y2 = i[5] as u16;
                let i_u2 = i[6] as u16;
                let i_v2 = i[7] as u16;
                let i_a_avg = (i_a1 + i_a2) / 2;

                let o_u = i_a_avg * ((i_u1 + i_u2) / 2) + (255 - i_a_avg) * o_u;
                let o_u = (o_u / 255) as u8;
                let o_v = i_a_avg * ((i_v1 + i_v2) / 2) + (255 - i_a_avg) * o_v;
                let o_v = (o_v / 255) as u8;
                let o_y1 = i_a1 * i_y1 + (255 - i_a1) * o_y1;
                let o_y1 = (o_y1 / 255) as u8;
                let o_y2 = i_a2 * i_y2 + (255 - i_a2) * o_y2;
                let o_y2 = (o_y2 / 255) as u8;

                o[0] = o_u;
                o[1] = o_y1;
                o[2] = o_v;
                o[3] = o_y2;
            }
        }
    } else {
        let alpha = (alpha * 255.0) as u16;

        for (out_line, in_line) in out_data
            .chunks_exact_mut(out_stride)
            .take(copy_rows)
            .zip(in_data.chunks_exact(in_stride))
        {
            let out_line = &mut out_line[..(copy_cols * 2)];
            let in_line = &in_line[..(copy_cols * 4)];

            for (o, i) in out_line.chunks_exact_mut(4).zip(in_line.chunks_exact(8)) {
                // o =   UY   VY
                // i = AYUV AYUV
                let o_u = o[0] as u16;
                let o_v = o[2] as u16;
                let o_y1 = o[1] as u16;
                let o_y2 = o[3] as u16;

                let i_a1 = (i[0] as u16 * alpha) / 255;
                let i_y1 = i[1] as u16;
                let i_u1 = i[2] as u16;
                let i_v1 = i[3] as u16;
                let i_a2 = (i[4] as u16 * alpha) / 255;
                let i_y2 = i[5] as u16;
                let i_u2 = i[6] as u16;
                let i_v2 = i[7] as u16;
                let i_a_avg = (i_a1 + i_a2) / 2;

                let o_u = i_a_avg * ((i_u1 + i_u2) / 2) + (255 - i_a_avg) * o_u;
                let o_u = (o_u / 255) as u8;
                let o_v = i_a_avg * ((i_v1 + i_v2) / 2) + (255 - i_a_avg) * o_v;
                let o_v = (o_v / 255) as u8;
                let o_y1 = i_a1 * i_y1 + (255 - i_a1) * o_y1;
                let o_y1 = (o_y1 / 255) as u8;
                let o_y2 = i_a2 * i_y2 + (255 - i_a2) * o_y2;
                let o_y2 = (o_y2 / 255) as u8;

                o[0] = o_u;
                o[1] = o_y1;
                o[2] = o_v;
                o[3] = o_y2;
            }
        }
    }
}

impl CustomCompositor {
    fn fill_queues(
        &self,
        pads: &[gst_base::AggregatorPad],
        timeout: bool,
        time: gst::ClockTime,
        end_time: gst::ClockTime,
    ) -> Result<(), gst::FlowError> {
        let mut all_eos = true;
        let mut need_wait = false;

        gst::trace!(CAT, imp = self, "Filling queues for {}-{}", time, end_time);

        for pad in pads {
            let cpad = pad.downcast_ref::<super::CustomCompositorPad>().unwrap();
            let imp = cpad.imp();
            let settings = imp.settings.lock().unwrap().clone();
            let mut state_guard = imp.state.lock().unwrap();
            if state_guard.is_none() {
                continue;
            }
            let segment = pad.segment();

            if segment.format() != gst::Format::Time {
                gst::warning!(CAT, obj = pad, "Don't have a Time segment");
                continue;
            }

            let segment = segment.downcast_ref::<gst::format::Time>().unwrap().clone();

            let PadState {
                ref info,
                ref mut current_frame,
                ..
            } = state_guard.as_mut().unwrap();

            if !pad.is_eos() {
                all_eos = false;
            }

            let mut have_next_frame = false;
            let mut dropped_old_frame = false;

            loop {
                let next_frame = pad.peek_buffer().and_then(|b| {
                    // Gap buffers
                    if b.size() == 0 {
                        pad.drop_buffer();
                        return None;
                    }
                    let pts = b.pts().unwrap();
                    let pts_end = if let Some(duration) = b.duration() {
                        pts + duration
                    } else {
                        pts
                    };

                    let running_time = segment.to_running_time(pts).unwrap();
                    let running_time_end = segment.to_running_time(pts_end).unwrap();
                    let frame = gst_video::VideoFrame::from_buffer_readable(b, info).unwrap();

                    Some(PadCurrentFrame {
                        frame,
                        converted_frame: None,
                        start_time: running_time,
                        end_time: running_time_end,
                        is_repeat: false,
                    })
                });

                let Some(ref next) = next_frame else {
                    if dropped_old_frame && !pad.is_eos() {
                        need_wait = true;
                    }
                    break;
                };

                gst::trace!(
                    CAT,
                    obj = pad,
                    "Got next buffer {}-{}",
                    next.start_time,
                    next.end_time
                );

                if Some(next.end_time) < current_frame.as_ref().map(|f| f.end_time) {
                    gst::warning!(CAT, obj = pad, "Frame from the past, dropping");
                    pad.drop_buffer();
                    continue;
                }

                if next.end_time >= time && next.start_time < end_time {
                    gst::debug!(
                        CAT,
                        obj = pad,
                        "Taking next buffer {}-{}",
                        next.start_time,
                        next.end_time
                    );
                    *current_frame = next_frame;
                    pad.drop_buffer();
                    have_next_frame = true;
                    break;
                } else if next.start_time >= end_time {
                    if let Some(ref current) = current_frame {
                        gst::debug!(
                            CAT,
                            obj = pad,
                            "Keeping for later {}-{}, using current frame {}-{}",
                            next.start_time,
                            next.end_time,
                            current.start_time,
                            current.end_time,
                        );
                    } else {
                        gst::debug!(
                            CAT,
                            obj = pad,
                            "Keeping for later {}-{}, no current frame",
                            next.start_time,
                            next.end_time
                        );
                    }
                    have_next_frame = true;
                    break;
                } else {
                    gst::debug!(
                        CAT,
                        obj = pad,
                        "Dropping old buffer {}-{}",
                        next.start_time,
                        next.end_time
                    );
                    *current_frame = next_frame;
                    pad.drop_buffer();
                    dropped_old_frame = true;
                }
            }

            if !have_next_frame {
                if let Some(current) = current_frame {
                    // When a pad is marked to repeat black on EOS, we substitute the current frame
                    // with a black one, which we never time out (is_repeat = true).
                    if !current.is_repeat && pad.is_eos() && current.end_time <= time {
                        gst::debug!(
                            CAT,
                            obj = pad,
                            "Timing out old frame {}-{}",
                            current.start_time,
                            current.end_time
                        );

                        if settings.repeat_black_on_eos {
                            let info = gst_video::VideoInfo::builder(
                                gst_video::VideoFormat::Uyvy,
                                current.frame.width(),
                                current.frame.height(),
                            )
                            .build()
                            .unwrap();
                            let b = gst::Buffer::with_size(info.size()).unwrap();
                            let mut frame =
                                gst_video::VideoFrame::from_buffer_writable(b, &info).unwrap();
                            let width = frame.width() as usize;
                            let stride = frame.plane_stride()[0] as usize;
                            for line in frame.plane_data_mut(0).unwrap().chunks_exact_mut(stride) {
                                for pixel in line[0..(2 * width)].chunks_exact_mut(2) {
                                    pixel[0] = 128;
                                    pixel[1] = 0;
                                }
                            }
                            *current_frame = Some(PadCurrentFrame {
                                frame: gst_video::VideoFrame::from_buffer_readable(
                                    frame.into_buffer(),
                                    &info,
                                )
                                .unwrap(),
                                converted_frame: None,
                                start_time: gst::ClockTime::ZERO,
                                end_time: gst::ClockTime::ZERO,
                                is_repeat: true,
                            });

                            gst::debug!(CAT, obj = pad, "Back in black");
                        } else {
                            gst::debug!(CAT, obj = pad, "Pad is fully EOS now");
                            *current_frame = None;
                        }
                    } else {
                        gst::trace!(
                            CAT,
                            obj = pad,
                            "Re-using old frame {}-{}",
                            current.start_time,
                            current.end_time
                        );
                        if !current.is_repeat && current.end_time >= time {
                            all_eos = false;
                        }
                    }
                } else if !pad.is_eos() {
                    gst::debug!(CAT, obj = pad, "Waiting for more data");
                    need_wait = true;
                }
            }
        }

        gst::debug!(
            CAT,
            imp = self,
            "Need waiting {}, timeout {}, all eos {}",
            need_wait,
            timeout,
            all_eos
        );

        if need_wait && !timeout {
            Err(gst_base::AGGREGATOR_FLOW_NEED_DATA)
        } else if all_eos {
            Err(gst::FlowError::Eos)
        } else {
            Ok(())
        }
    }

    fn convert_frames(&self, pads: &[gst_base::AggregatorPad]) -> Result<(), gst::FlowError> {
        for pad in pads {
            let cpad = pad.downcast_ref::<super::CustomCompositorPad>().unwrap();
            let imp = cpad.imp();
            let settings = imp.settings.lock().unwrap().clone();
            let mut state_guard = imp.state.lock().unwrap();

            let PadState {
                ref mut current_frame,
                ..
            } = state_guard.as_mut().unwrap();

            let current_frame = match current_frame {
                Some(frame) => frame,
                None => continue,
            };

            // If no conversion is needed (width/height are matching), just get rid of a potential
            // converted frame from a previous run. Otherwise scale.
            if (settings.width == 0 || settings.width == current_frame.frame.width() as i32)
                || (settings.height == 0 || settings.height == current_frame.frame.height() as i32)
            {
                let _ = current_frame.converted_frame.take();
                continue;
            }

            let target_width = if settings.width == 0 {
                current_frame.frame.width()
            } else {
                settings.width as u32
            };
            let target_height = if settings.height == 0 {
                current_frame.frame.height()
            } else {
                settings.height as u32
            };

            // Check if we already converted before to the same resolution
            if let Some(ref converted_frame) = current_frame.converted_frame {
                if converted_frame.width() == target_width
                    && converted_frame.height() == target_height
                {
                    continue;
                }
            }

            let current_info = current_frame.frame.info();
            let target_info =
                gst_video::VideoInfo::builder(current_info.format(), target_width, target_height)
                    .interlace_mode(current_info.interlace_mode())
                    .flags(current_info.flags())
                    .views(current_info.views())
                    .chroma_site(current_info.chroma_site())
                    .colorimetry(&current_info.colorimetry())
                    .par(current_info.par())
                    .fps(current_info.fps())
                    .multiview_mode(current_info.multiview_mode())
                    .multiview_flags(current_info.multiview_flags())
                    .field_order(current_info.field_order())
                    .build()
                    .unwrap();

            gst::debug!(
                CAT,
                obj = pad,
                "Converting frame from {}x{} to {}x{}",
                current_info.width(),
                current_info.height(),
                target_width,
                target_height
            );

            let converter = gst_video::VideoConverter::new(&current_info, &target_info, None)
                .map_err(|_| {
                    gst::error!(CAT, obj = pad, "Can't create converter");
                    gst::FlowError::NotNegotiated
                })?;

            let converted_buf = gst::Buffer::with_size(target_info.size()).map_err(|_| {
                gst::error!(CAT, obj = pad, "Can't allocate converted buffer");
                gst::FlowError::NotNegotiated
            })?;

            let mut converted_frame =
                gst_video::VideoFrame::from_buffer_writable(converted_buf, &target_info).unwrap();
            converter.frame(&current_frame.frame, &mut converted_frame);
            let converted_buf = converted_frame.into_buffer();

            current_frame.converted_frame = Some(
                gst_video::VideoFrame::from_buffer_readable(converted_buf, &target_info).unwrap(),
            );
        }

        Ok(())
    }

    fn composite(
        &self,
        pads: &[gst_base::AggregatorPad],
        out_frame: &mut gst_video::VideoFrame<gst_video::video_frame::Writable>,
    ) -> Result<(), gst::FlowError> {
        let out_stride = out_frame.plane_stride()[0] as usize;
        let out_width = out_frame.width() as usize;
        let out_height = out_frame.height() as usize;

        for pad in pads {
            let cpad = pad.downcast_ref::<super::CustomCompositorPad>().unwrap();
            let imp = cpad.imp();

            let settings = imp.settings.lock().unwrap().clone();
            if settings.alpha == 0.0 {
                gst::debug!(CAT, obj = pad, "Skipping fully transparent pad");
                continue;
            }

            let mut state_guard = imp.state.lock().unwrap();
            if state_guard.is_none() {
                continue;
            }

            let PadState {
                ref mut current_frame,
                ..
            } = state_guard.as_mut().unwrap();

            let current_frame = match current_frame {
                None => continue,
                Some(frame) => frame,
            };

            let vframe = current_frame
                .converted_frame
                .as_ref()
                .unwrap_or(&current_frame.frame);

            // Calculate output frame start position and input frame start position based on the
            // xpos configuration and the frame sizes
            let start_col =
                if settings.xpos < 0 && settings.xpos.abs() as u64 >= vframe.width() as u64 {
                    // Nothing to copy, completely outside the output frame
                    gst::debug!(CAT, obj = pad, "Skipping fully invisible pad");
                    continue;
                } else if settings.xpos <= 0 {
                    // Need to copy to 0, start at width + xpos
                    // Must start at an even position because of chroma subsampling
                    (0, (-settings.xpos) as usize & !1)
                } else if (settings.xpos as usize) < out_width {
                    // Need to copy to xpos, start at 0
                    // Must start at an even position because of chroma subsampling
                    (settings.xpos as usize & !1, 0)
                } else {
                    // Completely outside the output frame
                    gst::debug!(CAT, obj = pad, "Skipping fully invisible pad");
                    continue;
                };

            let start_row =
                if settings.ypos < 0 && settings.ypos.abs() as u64 >= vframe.height() as u64 {
                    // Nothing to copy, completely outside the output frame
                    gst::debug!(CAT, obj = pad, "Skipping fully invisible pad");
                    continue;
                } else if settings.ypos <= 0 {
                    // Need to copy to 0, start at height + ypos
                    (0, (-settings.ypos) as usize)
                } else if (settings.ypos as usize) < out_height {
                    // Need to copy to ypos, start at 0
                    (settings.ypos as usize, 0)
                } else {
                    // Completely outside the output frame
                    gst::debug!(CAT, obj = pad, "Skipping fully invisible pad");
                    continue;
                };

            // Calculate the number of rows to copy overall
            let copy_cols = std::cmp::min(
                vframe.width() as usize - start_col.1,
                out_width - start_col.0,
            );
            let copy_rows = std::cmp::min(
                vframe.height() as usize - start_row.1,
                out_height - start_row.0,
            );

            gst::debug!(
                CAT,
                obj = pad,
                "Compositing {}x{} from {:?} to {:?}",
                copy_cols,
                copy_rows,
                (start_row.1, start_col.1),
                (start_row.0, start_col.0)
            );

            let in_stride = vframe.plane_stride()[0] as usize;

            let out_data = &mut out_frame.plane_data_mut(0).unwrap()
                [start_row.0 * out_stride + 2 * start_col.0..];
            if vframe.format() == gst_video::VideoFormat::Uyvy {
                let in_data =
                    &vframe.plane_data(0).unwrap()[start_row.1 * in_stride + 2 * start_col.1..];

                composite_uyvy(
                    settings.alpha,
                    out_data,
                    out_stride,
                    in_data,
                    in_stride,
                    copy_rows,
                    copy_cols,
                );
            } else if vframe.format() == gst_video::VideoFormat::Ayuv {
                let in_data =
                    &vframe.plane_data(0).unwrap()[start_row.1 * in_stride + 4 * start_col.1..];
                composite_ayuv(
                    settings.alpha,
                    out_data,
                    out_stride,
                    in_data,
                    in_stride,
                    copy_rows,
                    copy_cols,
                );
            } else {
                unreachable!();
            }
        }

        Ok(())
    }
}

#[glib::object_subclass]
impl ObjectSubclass for CustomCompositor {
    const NAME: &'static str = "CustomCompositor";
    type Type = super::CustomCompositor;
    type ParentType = gst_base::Aggregator;
    type Interfaces = (gst::ChildProxy,);
}

impl GstObjectImpl for CustomCompositor {}

impl ChildProxyImpl for CustomCompositor {
    fn child_by_name(&self, name: &str) -> Option<glib::Object> {
        let object = self.obj();
        object
            .pads()
            .into_iter()
            .find(|p| p.name() == name)
            .map(|p| p.upcast())
    }

    fn child_by_index(&self, index: u32) -> Option<glib::Object> {
        let object = self.obj();
        object
            .pads()
            .into_iter()
            .nth(index as usize)
            .map(|p| p.upcast())
    }

    fn children_count(&self) -> u32 {
        let object = self.obj();
        object.num_pads() as u32
    }
}

impl ObjectImpl for CustomCompositor {
    fn constructed(&self) {
        self.parent_constructed();
    }
}

impl ElementImpl for CustomCompositor {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "Custom Compositor",
                "Filter/Editor/Video/Compositor",
                "Custom Compositor",
                "Sebastian Dröge <sebastian@centricular.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            let src_pad_template = gst::PadTemplate::with_gtype(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &gst::Caps::builder("video/x-raw")
                    .field("format", &"UYVY")
                    .field("width", &gst::IntRange::<i32>::new(1, i32::MAX))
                    .field("height", &gst::IntRange::<i32>::new(1, i32::MAX))
                    .field(
                        "framerate",
                        &gst::FractionRange::new(
                            gst::Fraction::new(1, i32::MAX),
                            gst::Fraction::new(i32::MAX, 1),
                        ),
                    )
                    .build(),
                gst_base::AggregatorPad::static_type(),
            )
            .unwrap();

            let sink_pad_template = gst::PadTemplate::with_gtype(
                "sink_%u",
                gst::PadDirection::Sink,
                gst::PadPresence::Request,
                &gst::Caps::builder("video/x-raw")
                    .field("format", &gst::List::new(&[&"UYVY", &"AYUV"]))
                    .field("width", &gst::IntRange::<i32>::new(1, i32::MAX))
                    .field("height", &gst::IntRange::<i32>::new(1, i32::MAX))
                    .field(
                        "framerate",
                        &gst::FractionRange::new(
                            gst::Fraction::new(1, i32::MAX),
                            gst::Fraction::new(i32::MAX, 1),
                        ),
                    )
                    .build(),
                super::CustomCompositorPad::static_type(),
            )
            .unwrap();

            vec![src_pad_template, sink_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }

    fn request_new_pad(
        &self,
        templ: &gst::PadTemplate,
        name: Option<&str>,
        caps: Option<&gst::Caps>,
    ) -> Option<gst::Pad> {
        let obj = self.obj();
        let sink_templ = obj.pad_template("sink_%u").unwrap();
        if templ != &sink_templ {
            gst::error!(CAT, imp = self, "Wrong pad template");
            return None;
        }

        let pad = self
            .parent_request_new_pad(templ, name, caps)?
            .downcast::<gst_base::AggregatorPad>()
            .unwrap();
        let cpad = pad.downcast_ref::<super::CustomCompositorPad>().unwrap();
        let imp = cpad.imp();
        imp.settings.lock().unwrap().zorder = obj.num_sink_pads() as u32;

        let proxy = obj.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        proxy.child_added(&pad, &pad.name());

        Some(pad.upcast())
    }

    fn release_pad(&self, pad: &gst::Pad) {
        self.obj().child_removed(pad, &pad.name());
        self.parent_release_pad(pad);
    }
}

impl AggregatorImpl for CustomCompositor {
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        Ok(())
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        // Drop our state now
        let _ = self.state.lock().unwrap().take();
        Ok(())
    }

    fn next_time(&self) -> Option<gst::ClockTime> {
        self.obj().simple_get_next_time()
    }

    fn flush(&self) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state = self.state.lock().unwrap();
        *state = if let Some(old) = state.take() {
            Some(State {
                info: old.info.clone(),
                start_time: gst::ClockTime::ZERO,
                num_frames: 0,
            })
        } else {
            None
        };

        self.obj()
            .src_pad()
            .segment()
            .set_position(None::<gst::ClockTime>);

        Ok(gst::FlowSuccess::Ok)
    }

    fn clip(
        &self,
        agg_pad: &gst_base::AggregatorPad,
        mut buffer: gst::Buffer,
    ) -> Option<gst::Buffer> {
        let segment = match agg_pad.segment().downcast::<gst::ClockTime>() {
            Ok(segment) => segment,
            Err(_) => {
                gst::error!(CAT, imp = self, "Only TIME segments supported");
                return Some(buffer);
            }
        };

        let pts = buffer.pts();
        if pts.is_none() {
            gst::error!(CAT, imp = self, "Only buffers with PTS supported");
            return Some(buffer);
        }

        let duration = if buffer.duration().is_some() {
            buffer.duration()
        } else {
            None
        };

        let pts = pts.unwrap();
        let pts_end = if let Some(duration) = duration {
            pts + duration
        } else {
            pts
        };

        gst::trace!(
            CAT,
            obj = agg_pad,
            "Clipping buffer {:?} with PTS {} and duration {}",
            buffer,
            pts,
            duration.display()
        );

        segment.clip(pts, pts_end).map(|(start, stop)| {
            {
                gst::trace!(
                    CAT,
                    obj = agg_pad,
                    "Clipped to start {} stop {}",
                    start.display(),
                    stop.display(),
                );

                let buffer = buffer.make_mut();
                buffer.set_pts(start);
                buffer.set_duration(
                    stop.zip(start)
                        .and_then(|(stop, start)| stop.checked_sub(start)),
                );
            }

            buffer
        })
    }

    fn aggregate(&self, timeout: bool) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state_guard = self.state.lock().unwrap();

        let state = match &mut *state_guard {
            None => return Err(gst::FlowError::NotNegotiated),
            Some(ref mut state) => state,
        };

        let src_segment = self
            .obj()
            .src_pad()
            .segment()
            .downcast::<gst::ClockTime>()
            .expect("Non-TIME segment");

        let time =
            if src_segment.position().is_none() || src_segment.position() < src_segment.start() {
                src_segment.start().unwrap()
            } else {
                src_segment.position().unwrap()
            };

        if state.num_frames == 0 {
            state.start_time = time;
        }

        let end_time = state.start_time
            + gst::ClockTime::from_nseconds(
                (state.num_frames + 1)
                    .mul_div_ceil(
                        gst::ClockTime::SECOND.nseconds() * state.info.fps().denom() as u64,
                        state.info.fps().numer() as u64,
                    )
                    .unwrap(),
            );

        gst::debug!(
            CAT,
            imp = self,
            "Aggregating for {} with framerate {} (timeout: {})",
            time,
            state.info.fps(),
            timeout
        );

        // Get all negotiated sinkpads and sort them by their zorder
        let mut pads = self
            .obj()
            .sink_pads()
            .into_iter()
            .filter_map(|p| {
                let apad = p.downcast::<gst_base::AggregatorPad>().unwrap();
                let cpad = apad.downcast_ref::<super::CustomCompositorPad>().unwrap();
                let imp = cpad.imp();
                if imp.state.lock().unwrap().is_some() {
                    Some(apad)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        pads.sort_by(|a, b| {
            let cpad_a = a.downcast_ref::<super::CustomCompositorPad>().unwrap();
            let cpad_b = b.downcast_ref::<super::CustomCompositorPad>().unwrap();

            let a_zorder = cpad_a.imp().settings.lock().unwrap().zorder;
            let b_zorder = cpad_b.imp().settings.lock().unwrap().zorder;

            a_zorder.cmp(&b_zorder)
        });

        // Note: pads[0] is the bottom pad, pads[last] is the top one
        self.fill_queues(&pads, timeout, time, end_time)?;

        drop(state_guard);

        self.obj()
            .selected_samples(time, None, end_time - time, None);

        let mut state_guard = self.state.lock().unwrap();

        let state = match &mut *state_guard {
            None => return Err(gst::FlowError::NotNegotiated),
            Some(ref mut state) => state,
        };

        self.convert_frames(&pads)?;

        // Check if we can use the first pad's buffer as background
        let first_pad_buffer = pads.get(0).and_then(|first_pad| {
            let cpad = first_pad
                .downcast_ref::<super::CustomCompositorPad>()
                .unwrap();
            let imp = cpad.imp();
            let pad_state_guard = imp.state.lock().unwrap();
            let pad_settings = imp.settings.lock().unwrap().clone();

            if let Some(ref pad_state) = &*pad_state_guard {
                if pad_state.info.width() == state.info.width()
                    && pad_state.info.height() == state.info.height()
                    && pad_state.info.format() == state.info.format()
                    && pad_settings.xpos == 0
                    && pad_settings.ypos == 0
                    && pad_settings.width == 0
                    && pad_settings.height == 0
                    && pad_settings.alpha == 1.0
                    && pad_state.current_frame.is_some()
                {
                    let current_frame = pad_state.current_frame.as_ref().unwrap();
                    gst::debug!(CAT, imp = self, "Taking first pad's buffer as background");
                    Some(current_frame.frame.buffer().to_owned())
                } else {
                    None
                }
            } else {
                None
            }
        });

        // Allocate the output buffer and fill ith the background as needed
        let (mut outbuf, needs_background) = match first_pad_buffer {
            Some(buffer) => {
                // No need to composite the bottom frame another time
                pads.remove(0);
                (buffer, false)
            }
            None => {
                gst::debug!(CAT, imp = self, "Allocating background");
                (gst::Buffer::with_size(state.info.size()).unwrap(), true)
            }
        };

        {
            let outbuf = outbuf.get_mut().unwrap();
            outbuf.set_pts(time);
            outbuf.set_duration(end_time - time);
        }

        // If we don't have any pads left at this point and don't need to draw the background then
        // we can simply pass through the input buffer of the first pad with modified pts/duration
        // as above. This prevents a copy of all memories below when mapping the buffer writable.
        if !pads.is_empty() || needs_background {
            let mut out_frame =
                gst_video::VideoFrame::from_buffer_writable(outbuf, &state.info).unwrap();

            // TODO: If we didn't simply copy the first pad's buffer, check if one of the
            // pads completely covers the output. If not, initialize the output with black
            if needs_background {
                let width = out_frame.width() as usize;
                let stride = out_frame.plane_stride()[0] as usize;
                for line in out_frame
                    .plane_data_mut(0)
                    .unwrap()
                    .chunks_exact_mut(stride)
                {
                    for pixel in line[0..(2 * width)].chunks_exact_mut(2) {
                        pixel[0] = 128;
                        pixel[1] = 0;
                    }
                }
            }

            self.composite(&pads, &mut out_frame)?;

            outbuf = out_frame.into_buffer();
        }

        state.num_frames += 1;
        drop(state_guard);

        self.obj().set_position(end_time);
        self.obj().finish_buffer(outbuf)
    }

    fn sink_event(&self, pad: &gst_base::AggregatorPad, event: gst::Event) -> bool {
        use gst::EventView;

        match event.view() {
            EventView::Caps(caps) => {
                let cpad = pad.downcast_ref::<super::CustomCompositorPad>().unwrap();
                let imp = cpad.imp();
                let caps = caps.caps();
                let mut pad_state_guard = imp.state.lock().unwrap();
                let info = gst_video::VideoInfo::from_caps(caps).unwrap();
                let buf_duration = if info.fps().numer() > 0 && info.fps().denom() > 0 {
                    Some(gst::ClockTime::from_nseconds(
                        gst::ClockTime::SECOND
                            .nseconds()
                            .mul_div_ceil(info.fps().denom() as u64, info.fps().numer() as u64)
                            .unwrap(),
                    ))
                } else {
                    None
                };

                match &mut *pad_state_guard {
                    None => {
                        *pad_state_guard = Some(PadState {
                            info,
                            current_frame: None,
                            buf_duration,
                        });
                    }
                    Some(ref mut state) => {
                        state.info = info;
                        state.buf_duration = buf_duration;
                    }
                }
            }
            _ => (),
        }

        self.parent_sink_event(pad, event)
    }

    fn negotiate(&self) -> bool {
        let obj = self.obj();
        let srcpad = obj.src_pad();
        let templ_caps = srcpad.pad_template_caps();

        let mut caps = srcpad.peer_query_caps(Some(&templ_caps));

        if caps.is_empty() {
            gst::error!(CAT, imp = self, "No supported downstream caps");
            return false;
        }

        caps.truncate();
        {
            let caps = caps.make_mut();
            let s = caps.structure_mut(0).unwrap();

            s.fixate_field_nearest_int("width", 320);
            s.fixate_field_nearest_int("height", 240);
            if s.has_field("pixel-aspect-ratio") {
                s.fixate_field_nearest_fraction("pixel-aspect-ratio", gst::Fraction::new(1, 1));
            }
            s.fixate_field_nearest_fraction("framerate", gst::Fraction::new(25, 1));
        }
        caps.fixate();

        let mut state = self.state.lock().unwrap();

        let info = gst_video::VideoInfo::from_caps(&caps).unwrap();
        let frame_duration = gst::ClockTime::SECOND
            .mul_div_ceil(info.fps().denom() as u64, info.fps().numer() as u64)
            .unwrap();

        *state = Some(State {
            info,
            start_time: gst::ClockTime::ZERO,
            num_frames: 0,
        });

        self.obj().set_src_caps(&caps);
        self.obj().set_latency(frame_duration, frame_duration);

        gst::debug!(CAT, imp = self, "Negotiated {}", caps,);

        true
    }

    fn peek_next_sample(&self, pad: &gst_base::AggregatorPad) -> Option<gst::Sample> {
        let obj = pad.downcast_ref::<super::CustomCompositorPad>().unwrap();
        let imp = obj.imp();

        let mut state_guard = imp.state.lock().unwrap();
        if state_guard.is_none() {
            return None;
        }

        let PadState {
            ref mut current_frame,
            ..
        } = state_guard.as_mut().unwrap();

        let current_frame = match current_frame {
            None => {
                return None;
            }
            Some(frame) => frame,
        };

        let vframe = current_frame
            .converted_frame
            .as_ref()
            .unwrap_or(&current_frame.frame);

        Some(
            gst::Sample::builder()
                .buffer(&vframe.buffer_owned())
                .caps(&vframe.info().to_caps().unwrap())
                .segment(&pad.segment())
                .build(),
        )
    }
}

struct PadCurrentFrame {
    frame: gst_video::VideoFrame<gst_video::video_frame::Readable>,
    converted_frame: Option<gst_video::VideoFrame<gst_video::video_frame::Readable>>,
    start_time: gst::ClockTime,
    end_time: gst::ClockTime,
    is_repeat: bool,
}

struct PadState {
    info: gst_video::VideoInfo,
    current_frame: Option<PadCurrentFrame>,
    buf_duration: Option<gst::ClockTime>,
}

#[derive(Clone)]
struct PadSettings {
    zorder: u32,
    xpos: i32,
    ypos: i32,
    width: i32,
    height: i32,
    alpha: f64,
    repeat_black_on_eos: bool,
}

impl Default for PadSettings {
    fn default() -> Self {
        Self {
            zorder: 0,
            xpos: 0,
            ypos: 0,
            width: 0,
            height: 0,
            alpha: 1.0,
            repeat_black_on_eos: false,
        }
    }
}

#[derive(Default)]
pub struct CustomCompositorPad {
    // State after we're negotiated
    state: Mutex<Option<PadState>>,
    // Pad properties
    settings: Mutex<PadSettings>,
}

#[glib::object_subclass]
impl ObjectSubclass for CustomCompositorPad {
    const NAME: &'static str = "CustomCompositorPad";
    type Type = super::CustomCompositorPad;
    type ParentType = gst_base::AggregatorPad;
}

impl ObjectImpl for CustomCompositorPad {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpecUInt::builder("zorder")
                    .nick("Z Order")
                    .blurb("Z order of the picture")
                    .minimum(0)
                    .maximum(std::u32::MAX)
                    .default_value(0)
                    .build(),
                glib::ParamSpecInt::builder("xpos")
                    .nick("X Position")
                    .blurb("X position of the picture")
                    .minimum(std::i32::MIN)
                    .maximum(std::i32::MAX)
                    .default_value(0)
                    .build(),
                glib::ParamSpecInt::builder("ypos")
                    .nick("Y Position")
                    .blurb("Y position of the picture")
                    .minimum(std::i32::MIN)
                    .maximum(std::i32::MAX)
                    .default_value(0)
                    .build(),
                glib::ParamSpecInt::builder("width")
                    .nick("Width")
                    .blurb("Width of the picture")
                    .minimum(std::i32::MIN)
                    .maximum(std::i32::MAX)
                    .default_value(0)
                    .build(),
                glib::ParamSpecInt::builder("height")
                    .nick("Height")
                    .blurb("Height of the picture")
                    .minimum(std::i32::MIN)
                    .maximum(std::i32::MAX)
                    .default_value(0)
                    .build(),
                glib::ParamSpecDouble::builder("alpha")
                    .nick("Alpha")
                    .blurb("Alpha of the picture")
                    .minimum(0.0)
                    .maximum(1.0)
                    .default_value(1.0)
                    .build(),
                glib::ParamSpecBoolean::builder("repeat-black-on-eos")
                    .nick("Repeat black on EOS")
                    .blurb("Whether the pad should repeat black frames on EOS until removed")
                    .default_value(false)
                    .build(),
            ]
        });

        &PROPERTIES
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        match pspec.name() {
            "zorder" => {
                let mut settings = self.settings.lock().unwrap();
                let zorder = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing zorder from {} to {}",
                    settings.zorder,
                    zorder,
                );
                settings.zorder = zorder;
            }
            "xpos" => {
                let mut settings = self.settings.lock().unwrap();
                let xpos = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing xpos from {} to {}",
                    settings.xpos,
                    xpos,
                );
                settings.xpos = xpos;
            }
            "ypos" => {
                let mut settings = self.settings.lock().unwrap();
                let ypos = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing ypos from {} to {}",
                    settings.ypos,
                    ypos,
                );
                settings.ypos = ypos;
            }
            "width" => {
                let mut settings = self.settings.lock().unwrap();
                let width = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing width from {} to {}",
                    settings.width,
                    width,
                );
                settings.width = width;
            }
            "height" => {
                let mut settings = self.settings.lock().unwrap();
                let height = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing height from {} to {}",
                    settings.height,
                    height,
                );
                settings.height = height;
            }
            "alpha" => {
                let mut settings = self.settings.lock().unwrap();
                let alpha = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing alpha from {} to {}",
                    settings.alpha,
                    alpha,
                );
                settings.alpha = alpha;
            }
            "repeat-black-on-eos" => {
                let mut settings = self.settings.lock().unwrap();
                let repeat = value.get().expect("type checked upstream");
                gst::info!(
                    CAT,
                    imp = self,
                    "Changing repeat black on EOS from {} to {}",
                    settings.repeat_black_on_eos,
                    repeat,
                );
                settings.repeat_black_on_eos = repeat;
            }
            _ => unimplemented!(),
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "zorder" => {
                let settings = self.settings.lock().unwrap();
                settings.zorder.to_value()
            }
            "xpos" => {
                let settings = self.settings.lock().unwrap();
                settings.xpos.to_value()
            }
            "ypos" => {
                let settings = self.settings.lock().unwrap();
                settings.ypos.to_value()
            }
            "width" => {
                let settings = self.settings.lock().unwrap();
                settings.width.to_value()
            }
            "height" => {
                let settings = self.settings.lock().unwrap();
                settings.height.to_value()
            }
            "alpha" => {
                let settings = self.settings.lock().unwrap();
                settings.alpha.to_value()
            }
            "repeat-black-on-eos" => {
                let settings = self.settings.lock().unwrap();
                settings.repeat_black_on_eos.to_value()
            }
            _ => unimplemented!(),
        }
    }
}

impl PadImpl for CustomCompositorPad {}
impl GstObjectImpl for CustomCompositorPad {}

impl AggregatorPadImpl for CustomCompositorPad {
    fn skip_buffer(&self, agg: &gst_base::Aggregator, buf: &gst::Buffer) -> bool {
        let pts = match buf.pts() {
            Some(pts) => pts,
            _ => {
                gst::debug!(CAT, imp = self, "Skipping buffer without pts {:?}", buf,);
                return true;
            }
        };

        let mut end_pts = pts;
        if let Some(dur) = buf.duration() {
            end_pts += dur;
        } else {
            let pad_state = self.state.lock().unwrap();
            if let Some(ref state) = &*pad_state {
                if let Some(dur) = state.buf_duration {
                    end_pts += dur;
                }
            }
        };

        let sink_segment = self.obj().segment();
        if sink_segment.format() != gst::Format::Time {
            gst::error!(
                CAT,
                imp = self,
                "Non-time segment, dropping buffer {:?}",
                buf,
            );
            return true;
        }

        let src_segment = agg.src_pad().segment();
        if src_segment.format() != gst::Format::Time {
            gst::debug!(CAT, imp = self, "Src segment is not configured");

            return false;
        }

        let sink_segment = sink_segment
            .downcast_ref::<gst::format::Time>()
            .unwrap()
            .clone();
        if sink_segment.start().unwrap() > pts {
            gst::debug!(CAT, imp = self, "Dropping out of segment buffer {:?}", buf,);

            return true;
        }

        let src_segment = src_segment
            .downcast_ref::<gst::format::Time>()
            .unwrap()
            .clone();
        let position = match src_segment.position() {
            Some(position) => position,
            _ => {
                gst::debug!(CAT, imp = self, "Src segment's position is not configured");

                return false;
            }
        };

        let out_running_time = src_segment.to_running_time(position).unwrap();
        let buffer_running_time = match sink_segment.to_running_time(end_pts) {
            Some(running_time) => running_time,
            None => {
                gst::debug!(
                    CAT,
                    imp = self,
                    "Drop out of segment buffer, segment {:?}, buffer {:?}",
                    sink_segment,
                    buf
                );

                return true;
            }
        };

        if buffer_running_time < out_running_time {
            gst::debug!(
                CAT,
                imp = self,
                "Dropping late buffer, buffer time {}, output time {}",
                buffer_running_time,
                out_running_time,
            );

            true
        } else {
            false
        }
    }
}
