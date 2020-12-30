use glib;
use glib::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use gst;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base;
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;
use gst_video;

use std::i32;
use std::sync::Mutex;

// FIXME: Does not enforce that all inputs/outputs have the same pixel aspect ratio

static CAT: once_cell::sync::Lazy<gst::DebugCategory> = once_cell::sync::Lazy::new(|| {
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

struct CustomCompositor {
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

                let o_u = i_a1 * ((i_u1 + i_u2) / 2) + (255 - i_a1) * o_u;
                let o_u = (o_u / 255) as u8;
                let o_v = i_a2 * ((i_v1 + i_v2) / 2) + (255 - i_a2) * o_v;
                let o_v = (o_v / 255) as u8;
                let o_y1 = i_a1 * i_y1 + (255 - i_a1) * o_y1;
                let o_y1 = (o_y1 / 255) as u8;
                let o_y2 = i_a2 * i_y2 + (255 - i_a1) * o_y2;
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

                let o_u = i_a1 * ((i_u1 + i_u2) / 2) + (255 - i_a1) * o_u;
                let o_u = (o_u / 255) as u8;
                let o_v = i_a2 * ((i_v1 + i_v2) / 2) + (255 - i_a2) * o_v;
                let o_v = (o_v / 255) as u8;
                let o_y1 = i_a1 * i_y1 + (255 - i_a1) * o_y1;
                let o_y1 = (o_y1 / 255) as u8;
                let o_y2 = i_a2 * i_y2 + (255 - i_a1) * o_y2;
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

        gst_trace!(CAT, obj: &self.get_instance(), "Filling queues for {}-{}", time, end_time);

        for pad in pads {
            let imp = CustomCompositorPad::from_instance(pad);
            let settings = imp.settings.lock().unwrap().clone();
            let mut state_guard = imp.state.lock().unwrap();
            if state_guard.is_none() {
                continue;
            }
            let segment = pad.get_segment();

            if segment.get_format() != gst::Format::Time {
                gst_warning!(CAT, obj: pad, "Don't have a Time segment");
                continue;
            }

            let segment = segment.downcast_ref::<gst::format::Time>().unwrap().clone();

            let PadState {
                ref info,
                ref mut current_frame,
                ..
            } = state_guard.as_mut().unwrap();

            let next_frame = pad.peek_buffer().and_then(|b| {
                // Gap buffers
                if b.get_size() == 0 {
                    pad.drop_buffer();
                    return None;
                }
                let pts = b.get_pts();
                let duration = b.get_duration();
                assert!(pts.is_some());
                assert!(duration.is_some());

                let running_time = segment.to_running_time(pts);
                let running_time_end = segment.to_running_time(pts + duration);
                let frame = gst_video::VideoFrame::from_buffer_readable(b, info).unwrap();

                Some(PadCurrentFrame {
                    frame,
                    converted_frame: None,
                    start_time: running_time,
                    end_time: running_time_end,
                    is_repeat: false,
                })
            });

            if !pad.is_eos() {
                all_eos = false;
            }

            if let Some(ref next) = next_frame {
                gst_trace!(
                    CAT,
                    obj: pad,
                    "Got next buffer {}-{}",
                    next.start_time,
                    next.end_time
                );

                if Some(next.end_time) < current_frame.as_ref().map(|f| f.end_time) {
                    gst_warning!(CAT, obj: pad, "Frame from the past, dropping");
                    pad.drop_buffer();
                    continue;
                }

                if next.end_time >= time && next.start_time < end_time {
                    gst_debug!(
                        CAT,
                        obj: pad,
                        "Taking next buffer {}-{}",
                        next.start_time,
                        next.end_time
                    );
                    *current_frame = next_frame;
                    pad.drop_buffer();
                } else if next.start_time >= end_time {
                    if let Some(ref current) = current_frame {
                        gst_debug!(
                            CAT,
                            obj: pad,
                            "Keeping for later {}-{}, using current frame {}-{}",
                            next.start_time,
                            next.end_time,
                            current.start_time,
                            current.end_time,
                        );
                    } else {
                        gst_debug!(
                            CAT,
                            obj: pad,
                            "Keeping for later {}-{}, no current frame",
                            next.start_time,
                            next.end_time
                        );
                    }
                } else {
                    gst_debug!(
                        CAT,
                        obj: pad,
                        "Taking next buffer {}-{} but waiting",
                        next.start_time,
                        next.end_time
                    );
                    *current_frame = next_frame;
                    pad.drop_buffer();
                    need_wait = true;
                }
            } else {
                if let Some(current) = current_frame {
                    // When a pad is marked to repeat black on EOS, we substitute the current frame
                    // with a black one, which we never time out (is_repeat = true).
                    if !current.is_repeat && pad.is_eos() && current.end_time <= time {
                        gst_debug!(
                            CAT,
                            obj: pad,
                            "Timing out old frame {}-{}",
                            current.start_time,
                            current.end_time
                        );

                        if settings.repeat_black_on_eos {
                            let info = gst_video::VideoInfo::builder(
                                gst_video::VideoFormat::Uyvy,
                                current.frame.info().width(),
                                current.frame.info().height(),
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
                                start_time: gst::CLOCK_TIME_NONE,
                                end_time: gst::CLOCK_TIME_NONE,
                                is_repeat: true,
                            });

                            gst_debug!(CAT, obj: pad, "Back in black");
                        } else {
                            gst_debug!(CAT, obj: pad, "Pad is fully EOS now");
                            *current_frame = None;
                        }
                    } else {
                        gst_trace!(
                            CAT,
                            obj: pad,
                            "Re-using old frame {}-{}",
                            current.start_time,
                            current.end_time
                        );
                        if !current.is_repeat && current.end_time >= time {
                            all_eos = false;
                        }
                    }
                } else if !pad.is_eos() {
                    gst_debug!(CAT, obj: pad, "Waiting for more data");
                    need_wait = true;
                }
            }
        }

        gst_debug!(CAT, obj: &self.get_instance(), "Need waiting {}, timeout {}, all eos {}", need_wait, timeout, all_eos);

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
            let imp = CustomCompositorPad::from_instance(pad);
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

            gst_debug!(
                CAT,
                obj: pad,
                "Converting frame from {}x{} to {}x{}",
                current_info.width(),
                current_info.height(),
                target_width,
                target_height
            );

            let converter = gst_video::VideoConverter::new(&current_info, &target_info, None)
                .map_err(|_| {
                    gst_error!(CAT, obj: pad, "Can't create converter");
                    gst::FlowError::NotNegotiated
                })?;

            let converted_buf = gst::Buffer::with_size(target_info.size()).map_err(|_| {
                gst_error!(CAT, obj: pad, "Can't allocate converted buffer");
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
            let imp = CustomCompositorPad::from_instance(pad);

            let settings = imp.settings.lock().unwrap().clone();
            if settings.alpha == 0.0 {
                gst_debug!(CAT, obj: pad, "Skipping fully transparent pad");
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
                    gst_debug!(CAT, obj: pad, "Skipping fully invisible pad");
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
                    gst_debug!(CAT, obj: pad, "Skipping fully invisible pad");
                    continue;
                };

            let start_row =
                if settings.ypos < 0 && settings.ypos.abs() as u64 >= vframe.height() as u64 {
                    // Nothing to copy, completely outside the output frame
                    gst_debug!(CAT, obj: pad, "Skipping fully invisible pad");
                    continue;
                } else if settings.ypos <= 0 {
                    // Need to copy to 0, start at height + ypos
                    (0, (-settings.ypos) as usize)
                } else if (settings.ypos as usize) < out_height {
                    // Need to copy to ypos, start at 0
                    (settings.ypos as usize, 0)
                } else {
                    // Completely outside the output frame
                    gst_debug!(CAT, obj: pad, "Skipping fully invisible pad");
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

            gst_debug!(
                CAT,
                obj: pad,
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

impl ObjectSubclass for CustomCompositor {
    const NAME: &'static str = "CustomCompositor";
    type ParentType = gst_base::Aggregator;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "Custom Compositor",
            "Filter/Editor/Video/Compositor",
            "Custom Compositor",
            "Sebastian Dröge <sebastian@centricular.com>",
        );

        let caps = gst::Caps::builder("video/x-raw")
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
            .build();
        let src_pad_template = gst::PadTemplate::with_gtype(
            "src",
            gst::PadDirection::Src,
            gst::PadPresence::Always,
            &caps,
            gst_base::AggregatorPad::static_type(),
        )
        .unwrap();
        klass.add_pad_template(src_pad_template);

        let caps = gst::Caps::builder("video/x-raw")
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
            .build();
        let sink_pad_template = gst::PadTemplate::with_gtype(
            "sink_%u",
            gst::PadDirection::Sink,
            gst::PadPresence::Request,
            &caps,
            CustomCompositorPad::get_type(),
        )
        .unwrap();
        klass.add_pad_template(sink_pad_template);
    }

    fn type_init(type_: &mut subclass::InitializingType<Self>) {
        type_.add_interface::<gst::ChildProxy>();
    }
}

impl ChildProxyImpl for CustomCompositor {
    fn get_child_by_name(&self, proxy: &gst::ChildProxy, name: &str) -> Option<glib::Object> {
        let element = proxy.dynamic_cast_ref::<gst::Element>().unwrap();
        let pad = element.get_static_pad(name);
        if pad.as_ref().map(|p| p.get_direction()) == Some(gst::PadDirection::Sink) {
            pad.map(|p| p.upcast())
        } else {
            None
        }
    }

    fn get_child_by_index(&self, proxy: &gst::ChildProxy, index: u32) -> Option<glib::Object> {
        let element = proxy.dynamic_cast_ref::<gst::Element>().unwrap();
        element
            .get_sink_pads()
            .get(index as usize)
            .map(|p| p.clone().upcast())
    }

    fn get_children_count(&self, proxy: &gst::ChildProxy) -> u32 {
        let element = proxy.dynamic_cast_ref::<gst::Element>().unwrap();
        element.num_sink_pads() as u32
    }
}

impl ObjectImpl for CustomCompositor {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        self.parent_constructed(obj);
    }
}

impl ElementImpl for CustomCompositor {
    fn request_new_pad(
        &self,
        element: &gst::Element,
        templ: &gst::PadTemplate,
        name: Option<String>,
        caps: Option<&gst::Caps>,
    ) -> Option<gst::Pad> {
        let agg = element.downcast_ref::<gst_base::Aggregator>().unwrap();
        let sink_templ = agg.get_pad_template("sink_%u").unwrap();
        if templ != &sink_templ {
            gst_error!(CAT, obj: agg, "Wrong pad template");
            return None;
        }

        let pad = self
            .parent_request_new_pad(element, templ, name, caps)?
            .downcast::<gst_base::AggregatorPad>()
            .unwrap();

        let imp = CustomCompositorPad::from_instance(&pad);
        imp.settings.lock().unwrap().zorder = agg.num_sink_pads() as u32;

        let proxy = agg.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        proxy.child_added(&pad, &pad.get_name());

        Some(pad.upcast())
    }

    fn release_pad(&self, element: &gst::Element, pad: &gst::Pad) {
        let agg = element.downcast_ref::<gst_base::Aggregator>().unwrap();

        let proxy = agg.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        proxy.child_removed(pad, &pad.get_name());

        self.parent_release_pad(element, pad);
    }
}

impl AggregatorImpl for CustomCompositor {
    fn start(&self, _agg: &gst_base::Aggregator) -> Result<(), gst::ErrorMessage> {
        Ok(())
    }

    fn stop(&self, _agg: &gst_base::Aggregator) -> Result<(), gst::ErrorMessage> {
        // Drop our state now
        let _ = self.state.lock().unwrap().take();
        Ok(())
    }

    fn get_next_time(&self, agg: &gst_base::Aggregator) -> gst::ClockTime {
        let state_guard = self.state.lock().unwrap();
        let state = match &*state_guard {
            None => {
                gst_debug!(CAT, obj: agg, "Have no state yet");
                return gst::CLOCK_TIME_NONE;
            }
            Some(ref state) => state,
        };

        let next_time = state.start_time
            + gst::ClockTime::from(state.num_frames.mul_div_ceil(
                gst::SECOND_VAL * *state.info.fps().denom() as u64,
                *state.info.fps().numer() as u64,
            ));

        gst_trace!(CAT, obj: agg, "Next time {}", next_time);

        next_time
    }

    fn clip(
        &self,
        agg: &gst_base::Aggregator,
        agg_pad: &gst_base::AggregatorPad,
        mut buffer: gst::Buffer,
    ) -> Option<gst::Buffer> {
        let segment = match agg_pad.get_segment().downcast::<gst::ClockTime>() {
            Ok(segment) => segment,
            Err(_) => {
                gst_error!(CAT, obj: agg, "Only TIME segments supported");
                return Some(buffer);
            }
        };

        let pts = buffer.get_pts();
        if pts.is_none() {
            gst_error!(CAT, obj: agg, "Only buffers with PTS supported");
            return Some(buffer);
        }

        let duration = if buffer.get_duration().is_some() {
            buffer.get_duration()
        } else {
            gst::CLOCK_TIME_NONE
        };

        gst_trace!(
            CAT,
            obj: agg_pad,
            "Clipping buffer {:?} with PTS {} and duration {}",
            buffer,
            pts,
            duration
        );

        segment.clip(pts, pts + duration).map(|(start, stop)| {
            {
                gst_trace!(
                    CAT,
                    obj: agg_pad,
                    "Clipped to start {} stop {}",
                    start,
                    stop,
                );

                let buffer = buffer.make_mut();
                buffer.set_pts(start);
                if duration.is_some() {
                    buffer.set_duration(stop - start);
                }
            }

            buffer
        })
    }

    fn aggregate(
        &self,
        agg: &gst_base::Aggregator,
        timeout: bool,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state_guard = self.state.lock().unwrap();

        let state = match &mut *state_guard {
            None => return Err(gst::FlowError::NotNegotiated),
            Some(ref mut state) => state,
        };

        let time = state.start_time
            + gst::ClockTime::from(state.num_frames.mul_div_ceil(
                gst::SECOND_VAL * *state.info.fps().denom() as u64,
                *state.info.fps().numer() as u64,
            ));

        let end_time = state.start_time
            + gst::ClockTime::from((state.num_frames + 1).mul_div_ceil(
                gst::SECOND_VAL * *state.info.fps().denom() as u64,
                *state.info.fps().numer() as u64,
            ));

        gst_debug!(
            CAT,
            obj: agg,
            "Aggregating for {} with framerate {} (timeout: {})",
            time,
            state.info.fps(),
            timeout
        );

        // Get all negotiated sinkpads and sort them by their zorder
        let mut pads = agg
            .get_sink_pads()
            .into_iter()
            .filter_map(|p| {
                let p = p.downcast::<gst_base::AggregatorPad>().unwrap();
                let imp = CustomCompositorPad::from_instance(&p);
                if imp.state.lock().unwrap().is_some() {
                    Some(p)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        pads.sort_by(|a, b| {
            let imp_a = CustomCompositorPad::from_instance(a);
            let imp_b = CustomCompositorPad::from_instance(b);

            let a_zorder = imp_a.settings.lock().unwrap().zorder;

            let b_zorder = imp_b.settings.lock().unwrap().zorder;

            a_zorder.cmp(&b_zorder)
        });

        // Note: pads[0] is the bottom pad, pads[last] is the top one
        self.fill_queues(&pads, timeout, time, end_time)?;

        drop(state_guard);

        agg.selected_samples(time, gst::CLOCK_TIME_NONE, end_time - time, None);

        let mut state_guard = self.state.lock().unwrap();

        let state = match &mut *state_guard {
            None => return Err(gst::FlowError::NotNegotiated),
            Some(ref mut state) => state,
        };

        self.convert_frames(&pads)?;

        // Check if we can use the first pad's buffer as background
        let first_pad_buffer = pads.get(0).and_then(|first_pad| {
            let imp = CustomCompositorPad::from_instance(first_pad);
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
                    gst_debug!(CAT, obj: agg, "Taking first pad's buffer as background");
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
                gst_debug!(CAT, obj: agg, "Allocating background");
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

        agg.finish_buffer(outbuf)
    }

    fn sink_event(
        &self,
        agg: &gst_base::Aggregator,
        pad: &gst_base::AggregatorPad,
        event: gst::Event,
    ) -> bool {
        use gst::EventView;

        match event.view() {
            EventView::Caps(caps) => {
                let imp = CustomCompositorPad::from_instance(pad);
                let caps = caps.get_caps();
                let mut pad_state_guard = imp.state.lock().unwrap();

                match &mut *pad_state_guard {
                    None => {
                        *pad_state_guard = Some(PadState {
                            info: gst_video::VideoInfo::from_caps(caps).unwrap(),
                            current_frame: None,
                        });
                    }
                    Some(ref mut state) => {
                        state.info = gst_video::VideoInfo::from_caps(caps).unwrap();
                    }
                }
            }
            _ => (),
        }

        self.parent_sink_event(agg, pad, event)
    }

    fn negotiate(&self, agg: &gst_base::Aggregator) -> bool {
        let srcpad = agg
            .get_static_pad("src")
            .unwrap()
            .downcast::<gst_base::AggregatorPad>()
            .unwrap();
        let templ_caps = srcpad.get_pad_template_caps().unwrap();

        let mut caps = srcpad
            .peer_query_caps(Some(&templ_caps))
            .unwrap_or(templ_caps);

        if caps.is_empty() {
            gst_error!(CAT, obj: agg, "No supported downstream caps");
            return false;
        }

        caps.truncate();
        {
            let caps = caps.make_mut();
            let s = caps.get_mut_structure(0).unwrap();

            s.fixate_field_nearest_int("width", 320);
            s.fixate_field_nearest_int("height", 240);
            if s.has_field("pixel-aspect-ratio") {
                s.fixate_field_nearest_fraction("pixel-aspect-ratio", gst::Fraction::new(1, 1));
            }
            s.fixate_field_nearest_fraction("framerate", gst::Fraction::new(25, 1));
        }
        caps.fixate();

        let mut state = self.state.lock().unwrap();

        let start_time = if let Some(ref state) = &*state {
            state.start_time
                + gst::ClockTime::from(state.num_frames.mul_div_ceil(
                    gst::SECOND_VAL * *state.info.fps().denom() as u64,
                    *state.info.fps().numer() as u64,
                ))
        } else {
            let position = srcpad
                .get_segment()
                .downcast_ref::<gst::format::Time>()
                .unwrap()
                .get_position();
            if position.is_none() {
                0.into()
            } else {
                position
            }
        };

        let info = gst_video::VideoInfo::from_caps(&caps).unwrap();
        let frame_duration = gst::SECOND
            .mul_div_ceil(*info.fps().denom() as u64, *info.fps().numer() as u64)
            .unwrap();

        *state = Some(State {
            info,
            start_time,
            num_frames: 0,
        });

        agg.set_src_caps(&caps);
        agg.set_latency(frame_duration, frame_duration);

        gst_debug!(
            CAT,
            obj: agg,
            "Negotiated {}, new start time {}",
            caps,
            start_time
        );

        true
    }

    fn peek_next_sample(
        &self,
        _aggregator: &gst_base::Aggregator,
        pad: &gst_base::AggregatorPad,
    ) -> Option<gst::Sample> {
        let imp = CustomCompositorPad::from_instance(pad);

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
                .segment(&pad.get_segment())
                .build(),
        )
    }
}

static PAD_PROPERTIES: [subclass::Property; 7] = [
    subclass::Property("zorder", |name| {
        glib::ParamSpec::uint(
            name,
            "Z Order",
            "Z order of the picture",
            0,
            std::u32::MAX,
            0,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("xpos", |name| {
        glib::ParamSpec::int(
            name,
            "X Position",
            "X position of the picture",
            std::i32::MIN,
            std::i32::MAX,
            0,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("ypos", |name| {
        glib::ParamSpec::int(
            name,
            "Y Position",
            "Y position of the picture",
            std::i32::MIN,
            std::i32::MAX,
            0,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("width", |name| {
        glib::ParamSpec::int(
            name,
            "Width",
            "Width of the picture",
            std::i32::MIN,
            std::i32::MAX,
            0,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("height", |name| {
        glib::ParamSpec::int(
            name,
            "Height",
            "Height of the picture",
            std::i32::MIN,
            std::i32::MAX,
            0,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("alpha", |name| {
        glib::ParamSpec::double(
            name,
            "Alpha",
            "Alpha of the picture",
            0.0,
            1.0,
            1.0,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("repeat-black-on-eos", |name| {
        glib::ParamSpec::boolean(
            name,
            "Repeat black on EOS",
            "Whether the pad should repeat black frames on EOS until removed",
            false,
            glib::ParamFlags::READWRITE,
        )
    }),
];

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

struct CustomCompositorPad {
    // State after we're negotiated
    state: Mutex<Option<PadState>>,
    // Pad properties
    settings: Mutex<PadSettings>,
}

impl ObjectSubclass for CustomCompositorPad {
    const NAME: &'static str = "CustomCompositorPad";
    type ParentType = gst_base::AggregatorPad;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            state: Mutex::new(None),
            settings: Mutex::new(PadSettings::default()),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.install_properties(&PAD_PROPERTIES);
    }
}

impl ObjectImpl for CustomCompositorPad {
    glib_object_impl!();

    fn set_property(&self, obj: &glib::Object, id: usize, value: &glib::Value) {
        let prop = &PAD_PROPERTIES[id];
        let pad = obj.downcast_ref::<gst_base::AggregatorPad>().unwrap();

        match *prop {
            subclass::Property("zorder", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let zorder = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing zorder from {} to {}",
                    settings.zorder,
                    zorder,
                );
                settings.zorder = zorder;
            }
            subclass::Property("xpos", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let xpos = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing xpos from {} to {}",
                    settings.xpos,
                    xpos,
                );
                settings.xpos = xpos;
            }
            subclass::Property("ypos", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let ypos = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing ypos from {} to {}",
                    settings.ypos,
                    ypos,
                );
                settings.ypos = ypos;
            }
            subclass::Property("width", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let width = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing width from {} to {}",
                    settings.width,
                    width,
                );
                settings.width = width;
            }
            subclass::Property("height", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let height = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing height from {} to {}",
                    settings.height,
                    height,
                );
                settings.height = height;
            }
            subclass::Property("alpha", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let alpha = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing alpha from {} to {}",
                    settings.alpha,
                    alpha,
                );
                settings.alpha = alpha;
            }
            subclass::Property("repeat-black-on-eos", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let repeat = value.get_some().expect("type checked upstream");
                gst_info!(
                    CAT,
                    obj: pad,
                    "Changing repeat black on EOS from {} to {}",
                    settings.repeat_black_on_eos,
                    repeat,
                );
                settings.repeat_black_on_eos = repeat;
            }
            _ => unimplemented!(),
        }
    }

    fn get_property(&self, _obj: &glib::Object, id: usize) -> Result<glib::Value, ()> {
        let prop = &PAD_PROPERTIES[id];

        match *prop {
            subclass::Property("zorder", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.zorder.to_value())
            }
            subclass::Property("xpos", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.xpos.to_value())
            }
            subclass::Property("ypos", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.ypos.to_value())
            }
            subclass::Property("width", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.width.to_value())
            }
            subclass::Property("height", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.height.to_value())
            }
            subclass::Property("alpha", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.alpha.to_value())
            }
            subclass::Property("repeat-black-on-eos", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.repeat_black_on_eos.to_value())
            }
            _ => unimplemented!(),
        }
    }
}

impl PadImpl for CustomCompositorPad {}
impl AggregatorPadImpl for CustomCompositorPad {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "custom-compositor",
        gst::Rank::None,
        CustomCompositor::get_type(),
    )
}
