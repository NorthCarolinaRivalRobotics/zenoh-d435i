//! Configure and stream a 435i sensor.
//!
//! Notice that the streaming configuration changes based on the USB speed of the sensor.
//! If one attemps to set a streaming configuration that is too much for the current USB
//! speed, RealSense will return with an error. However, that error is non-descript and will
//! not help identify the underlying problem, i.e. the bandwidth of the connection.

use anyhow::{ensure, Result};
use realsense_rust::{
    config::Config,
    context::Context,
    frame::{ColorFrame, DepthFrame, FrameEx, GyroFrame, PoseFrame},
    kind::{Rs2CameraInfo, Rs2Format, Rs2ProductLine, Rs2StreamKind},
    pipeline::InactivePipeline,
};
use zenoh_types::{get_data_from_pixel, ColorFrameSerializable, DepthFrameSerializable};
use std::{
    collections::HashSet,
    convert::TryFrom,
    io::{self, Write},
    time::Duration,
};

mod zenoh_types;

#[tokio::main]
async fn main() -> Result<()> {
    let session = zenoh::open(zenoh::Config::default()).await.unwrap();

    // Check for depth or color-compatible devices.
    let mut queried_devices = HashSet::new();
    queried_devices.insert(Rs2ProductLine::D400);
    let context = Context::new()?;
    let devices = context.query_devices(queried_devices);
    ensure!(!devices.is_empty(), "No devices found");

    // create pipeline
    let pipeline = InactivePipeline::try_from(&context)?;
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
    let mut pipeline = pipeline.start(Some(config))?;
    let mut distance = 0.0;
    let mut motion = [0.0, 0.0, 0.0];

    // process frames
    for i in 0..1000 {
        let timeout = Duration::from_millis(500);
        let frames = pipeline.wait(Some(timeout))?;

        if i % 5 == 0 {
            // Get depth
            let mut depth_frames = frames.frames_of_type::<DepthFrame>();
            if !depth_frames.is_empty() {
                let depth_frame = depth_frames.pop().unwrap();
                let timestamp = depth_frame.timestamp();
                let depth_serializable = DepthFrameSerializable::new(depth_frame, timestamp);
                let encoded = bincode::encode_to_vec(&depth_serializable, bincode::config::standard()).unwrap();
                println!("depth number of pixels: {}", depth_serializable.data.len());
                session.put("camera/depth", encoded).await.unwrap();
            }

            let mut rgb_frame = frames.frames_of_type::<ColorFrame>();
            if !rgb_frame.is_empty() {
                let rgb_frame = rgb_frame.pop().unwrap();
                let timestamp = rgb_frame.timestamp();
                let rgb_serializable = ColorFrameSerializable::new(rgb_frame, timestamp);
                println!("rgb number of pixels: {}", rgb_serializable.data.len());  
                let encoded = bincode::encode_to_vec(&rgb_serializable, bincode::config::standard()).unwrap();
                session.put("camera/rgb", encoded).await.unwrap();
            }
            // Get gyro
            let motion_frames = frames.frames_of_type::<GyroFrame>();
            if !motion_frames.is_empty() {
                motion = *motion_frames[0].rotational_velocity();
            }


        }

        // Print our results
        print!(
            "\rDistance of center pixel: {:<10} m | Gyro reading: {:<15}, {:<15}, {:<15}",
            distance, motion[0], motion[1], motion[2]
        );
        io::stdout().flush().unwrap();
    }

    Ok(())
}
