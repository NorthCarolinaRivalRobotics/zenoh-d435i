use bincode::{Decode, Encode};
use realsense_rust::{frame::{ColorFrame, DepthFrame, ImageFrame, PixelKind}, kind};
use serde::{Serialize, Deserialize};
use turbojpeg::{compress_image, decompress_image, image::{ImageBuffer, Rgb}, OwnedBuf, PixelFormat, Subsamp};
use zstd::stream::{copy_encode, decode_all, encode_all};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum ImageEncoding {
    RGB8,
    Z16,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Encode, Decode)]
pub struct RGB8Local {
    b: u8,
    g: u8,
    r: u8,
}

const DEPTH_SCALE_FACTOR: u16 = 8738; // multiply by this to convert meters to u16
const MINIMUM_DISTANCE_METERS: f32 = 0.5;

pub fn encode_meters_to_u16(meters: f32)     -> u16 {
    ((meters - MINIMUM_DISTANCE_METERS) * DEPTH_SCALE_FACTOR as f32) as u16
}

pub fn decode_u16_to_meters(code: u16) -> f32 {
    (code as f32) / DEPTH_SCALE_FACTOR as f32 + MINIMUM_DISTANCE_METERS
}


#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct DepthFrameSerializable {
    pub width: usize,
    pub height: usize,
    pub timestamp: f64,
    pub data: Vec<u16>, // distances in meters
}


#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct ColorFrameSerializable {
    pub width: usize,
    pub height: usize,
    pub timestamp: f64,
    pub data: Vec<u8>, // RGB8
}

#[derive(Encode, Decode)]
pub struct ImageForWire {
    pub image: Vec<u8>,
    pub timestamp: f64,
}


impl DepthFrameSerializable {
    pub fn new(frame: DepthFrame, timestamp: f64) -> Self {
        let mut data: Vec<u16> = Vec::new();
        for row in 0..frame.height() {
            for col in 0..frame.width() {
                data.push(encode_meters_to_u16(frame.distance(col, row).unwrap()));
            }
        }
        Self {
            width: frame.width(),
            height: frame.height(),
            timestamp: timestamp,
            data: data,
        }
    }

    pub fn encodeAndCompress(&self) -> Vec<u8> {
        let encoded = bincode::encode_to_vec(&self, bincode::config::standard()).unwrap();
        let mut result = Vec::new();
        copy_encode(&encoded[..], &mut result, 6).unwrap();
        result
    }
}


impl ColorFrameSerializable {
    pub fn new(frame: ColorFrame, timestamp: f64) -> Self {
        let mut data: Vec<u8> = Vec::new();
        for row in 0..frame.height() {
            for col in 0..frame.width() {
                let px = get_data_from_pixel(frame.get(col, row).unwrap());
                data.push(px.r);
                data.push(px.g);
                data.push(px.b);
            }
        }   

        Self {
            width: frame.width(),
            height: frame.height(),
            timestamp: timestamp,
            data: data,
        }
    }
    pub fn encodeAndCompress(&self) -> Vec<u8> {
        let jpeg = compress_image::<Rgb<u8>>(&ImageBuffer::from_vec(self.width as u32, self.height as u32, self.data.clone()).unwrap(), 75, Subsamp::Sub2x2).unwrap();
        let envelope: ImageForWire = ImageForWire {
            image: jpeg.to_vec(),
            timestamp: self.timestamp,
        };
        let encoded = bincode::encode_to_vec(&envelope, bincode::config::standard()).unwrap();
        encoded
    }
    pub fn decodeAndDecompress(encoded: Vec<u8>) -> (Vec<u8>, f64) {
        let (wire, _): (ImageForWire, _) =
        bincode::decode_from_slice(&encoded, bincode::config::standard()).unwrap();

        // JPEG --------------------------------------------------------------
        debug_assert!(wire.image.starts_with(&[0xFF, 0xD8]));
        let rgb = turbojpeg::decompress_image::<Rgb<u8>>(&wire.image).unwrap();

        (rgb.to_vec(), wire.timestamp)

    }
}

pub fn get_data_from_pixel(pixel: PixelKind<'_>) ->RGB8Local {
    match pixel {
        PixelKind::Bgr8 { b, g, r } => RGB8Local { b: *b, g: *g, r: *r },
        _ => panic!("Unsupported pixel format"),
    }
}


#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct MotionFrameData {
    pub gyro: [f32; 3], // rad/s
    pub accel: [f32; 3], // m/s^2
    pub timestamp: f64, // seconds
}

impl MotionFrameData {
    pub fn new(gyro: [f32; 3], accel: [f32; 3], timestamp: f64) -> Self {
        Self { gyro, accel, timestamp }
    }

    pub fn encodeAndCompress(&self) -> Vec<u8> {
        let encoded = bincode::encode_to_vec(&self, bincode::config::standard()).unwrap();
        encoded
    }
    pub fn decodeAndDecompress(encoded: Vec<u8>) -> Self {
        let (wire, _): (MotionFrameData, _) =
        bincode::decode_from_slice(&encoded, bincode::config::standard()).unwrap();
        wire
    }
}