//! Configure and stream a 435i sensor.
//!
//! Notice that the streaming configuration changes based on the USB speed of the sensor.
//! If one attemps to set a streaming configuration that is too much for the current USB
//! speed, RealSense will return with an error. However, that error is non-descript and will
//! not help identify the underlying problem, i.e. the bandwidth of the connection.

use anyhow::{ensure, Result};
use realsense_rust::{
    base::Rs2Intrinsics, config::Config, context::Context, frame::{AccelFrame, ColorFrame, DepthFrame, FrameEx, GyroFrame, PoseFrame}, kind::{Rs2CameraInfo, Rs2Format, Rs2Option, Rs2ProductLine, Rs2StreamKind}, pipeline::InactivePipeline
};
use tokio::net::unix::pipe;
use zenoh_types::{get_data_from_pixel, ColorFrameSerializable, CombinedFrameWire, DepthFrameSerializable, MotionFrameData};
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
            .enable_stream(Rs2StreamKind::Gyro, None, 0, 0, Rs2Format::Any, 0)?
            .enable_stream(Rs2StreamKind::Accel, None, 0, 0, Rs2Format::Any, 0)?;
    } else {
        config
            .enable_device_from_serial(devices[0].info(Rs2CameraInfo::SerialNumber).unwrap())?
            .disable_all_streams()?
            .enable_stream(Rs2StreamKind::Depth, None, 640, 0, Rs2Format::Z16, 30)?
            .enable_stream(Rs2StreamKind::Infrared, Some(1), 640, 0, Rs2Format::Y8, 30)?
            .enable_stream(Rs2StreamKind::Gyro, None, 0, 0, Rs2Format::Any, 0)?
            .enable_stream(Rs2StreamKind::Accel, None, 0, 0, Rs2Format::Any, 0)?;

    }

    // Change pipeline's type from InactivePipeline -> ActivePipeline
    let mut pipeline = pipeline.start(Some(config)).unwrap();
    enable_system_time(&devices[0])?;
    let mut gyro = [0.0, 0.0, 0.0];
    let mut accel = [0.0, 0.0, 0.0];

    // process frames
    loop {
        let timeout = Duration::from_millis(500);
        let frames = pipeline.wait(Some(timeout))?;

        // Get depth
        let mut depth_frames = frames.frames_of_type::<DepthFrame>();
        let mut rgb_frame = frames.frames_of_type::<ColorFrame>();
        println!("{} {}", depth_frames.is_empty(), rgb_frame.is_empty());
        if !depth_frames.is_empty() &&  !rgb_frame.is_empty() {
            let depth_frame = depth_frames.pop().unwrap();
            let rgb_frame = rgb_frame.pop().unwrap();
            // let timestamp = depth_frame.timestamp();
            // let depth_serializable = DepthFrameSerializable::new(&depth_frame, timestamp);
            // let encoded_depth = depth_serializable.encodeAndCompress();
            // let timestamp = rgb_frame.timestamp();
            // let rgb_serializable = ColorFrameSerializable::new(&rgb_frame, timestamp);
            // let encoded_rgb = rgb_serializable.encodeAndCompress();
            let combined_frame = CombinedFrameWire::from_frames(&depth_frame, &rgb_frame);
            // session.put("camera/rgb", encoded_rgb).await.map_err(|e| anyhow::anyhow!(e))?;
            // session.put("camera/depth", encoded_depth).await.map_err(|e| anyhow::anyhow!(e))?;
            println!("sending frame...");
            session.put("camera/combined", combined_frame.encode()).await.map_err(|e| anyhow::anyhow!(e))?;
        }

        // Get gyro
        let gyro_frames = frames.frames_of_type::<GyroFrame>();
        let accel_frames = frames.frames_of_type::<AccelFrame>();

        if !gyro_frames.is_empty() && !accel_frames.is_empty() {
            gyro = *gyro_frames[0].rotational_velocity();
            accel = *accel_frames[0].acceleration();
            let timestamp = gyro_frames[0].timestamp();
            let motion_frame_data = MotionFrameData::new(gyro, accel, timestamp);
            let encoded_motion = motion_frame_data.encodeAndCompress();
            session.put("camera/motion", encoded_motion).await.map_err(|e| anyhow::anyhow!(e))?;
        }

    }

}

fn enable_system_time(device: &realsense_rust::device::Device) -> anyhow::Result<()> {
    for mut sensor in device.sensors() {
        // 0.0 = SYSTEM_TIME, 1.0 = GLOBAL_TIME
        sensor.set_option(Rs2Option::GlobalTimeEnabled, 0.0)?;   // ← key line
        println!("{} global-time-enabled = {}", sensor.info(Rs2CameraInfo::Name).unwrap().to_str().unwrap(), sensor.get_option(Rs2Option::GlobalTimeEnabled).unwrap()); // should print 0

    }
    Ok(())
}
