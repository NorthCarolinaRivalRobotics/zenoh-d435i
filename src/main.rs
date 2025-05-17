//! Configure and stream a 435i sensor.
//!
//! Notice that the streaming configuration changes based on the USB speed of the sensor.
//! If one attemps to set a streaming configuration that is too much for the current USB
//! speed, RealSense will return with an error. However, that error is non-descript and will
//! not help identify the underlying problem, i.e. the bandwidth of the connection.

use anyhow::{ensure, Result};
use realsense_rust::{
    base::Rs2Intrinsics, config::Config, context::Context, frame::{ColorFrame, DepthFrame, FrameEx, GyroFrame, PoseFrame}, kind::{Rs2CameraInfo, Rs2Format, Rs2ProductLine, Rs2StreamKind}, pipeline::InactivePipeline
};
use zenoh_types::{get_data_from_pixel, ColorFrameSerializable, DepthFrameSerializable};
use std::{
    collections::HashSet,
    convert::TryFrom,
    io::{self, Write},
    time::Duration,
};
use snap::raw::Encoder;

mod zenoh_types;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let session = zenoh::open(zenoh::Config::default()).await.unwrap();

    // Check for depth or color-compatible devices.
    let mut queried_devices = HashSet::new();
    queried_devices.insert(Rs2ProductLine::D400);
    let context = Context::new().unwrap();
    let devices = context.query_devices(queried_devices);
    ensure!(!devices.is_empty(), "No devices found");

    // create pipeline
    let pipeline = InactivePipeline::try_from(&context).unwrap();
    let mut config = Config::new();

    // Check the USB speed of our connection
    // CStr => str => f32
    let usb_cstr = devices[0].info(Rs2CameraInfo::UsbTypeDescriptor).unwrap();
    let usb_val: f32 = usb_cstr.to_str().unwrap().parse().unwrap();
    if usb_val >= 3.0 {
        config
            .enable_device_from_serial(devices[0].info(Rs2CameraInfo::SerialNumber).unwrap())?
            .disable_all_streams()?
            .enable_stream(Rs2StreamKind::Depth, None, 640, 0, Rs2Format::Z16, 30)?
            .enable_stream(Rs2StreamKind::Color, None, 640, 0, Rs2Format::Rgb8, 30)?
            .enable_stream(Rs2StreamKind::Gyro, None, 0, 0, Rs2Format::Any, 0)?;
    } else {
        config
            .enable_device_from_serial(devices[0].info(Rs2CameraInfo::SerialNumber).unwrap())?
            .disable_all_streams()?
            .enable_stream(Rs2StreamKind::Depth, None, 640, 0, Rs2Format::Z16, 30)?
            .enable_stream(Rs2StreamKind::Infrared, Some(1), 640, 0, Rs2Format::Y8, 30)?
            .enable_stream(Rs2StreamKind::Gyro, None, 0, 0, Rs2Format::Any, 0)?;
    }

    // Change pipeline's type from InactivePipeline -> ActivePipeline
    let mut pipeline = pipeline.start(Some(config)).unwrap();
    let mut motion = [0.0, 0.0, 0.0];

    // process frames
    loop {
        let timeout = Duration::from_millis(500);
        let frames = pipeline.wait(Some(timeout))?;

        // Get depth
        let mut depth_frames = frames.frames_of_type::<DepthFrame>();
        let mut rgb_frame = frames.frames_of_type::<ColorFrame>();

        if !depth_frames.is_empty() &&  !rgb_frame.is_empty() {
            let depth_frame = depth_frames.pop().unwrap();
            let rgb_frame = rgb_frame.pop().unwrap();
            let timestamp = depth_frame.timestamp();
            let depth_serializable = DepthFrameSerializable::new(depth_frame, timestamp);
            let encoded_depth = depth_serializable.encodeAndCompress();
            let timestamp = rgb_frame.timestamp();
            let rgb_serializable = ColorFrameSerializable::new(rgb_frame, timestamp);
            let encoded_rgb = rgb_serializable.encodeAndCompress();
            session.put("camera/rgb", encoded_rgb).await.map_err(|e| anyhow::anyhow!(e))?;
            session.put("camera/depth", encoded_depth).await.map_err(|e| anyhow::anyhow!(e))?;

        }

        // Get gyro
        let motion_frames = frames.frames_of_type::<GyroFrame>();
        if !motion_frames.is_empty() {
            motion = *motion_frames[0].rotational_velocity();
        }
    }

}
