use bincode::{Decode, Encode};
use realsense_rust::{frame::{ColorFrame, DepthFrame, FrameEx, ImageFrame, PixelKind}, kind};
use serde::{Serialize, Deserialize};
use turbojpeg::{compress_image, decompress_image, image::{ImageBuffer, Rgb}, OwnedBuf, PixelFormat, Subsamp};
use zstd::stream::{copy_encode, decode_all, encode_all};

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct CombinedFrame {
    pub rgb: ColorFrameSerializable,
    pub depth: DepthFrameSerializable,
    pub timestamp: f64,
}

impl CombinedFrame {
    pub fn new(rgb: ColorFrameSerializable, depth: DepthFrameSerializable, timestamp: f64) -> Self {
        Self { rgb, depth, timestamp }
    }

    pub fn encodeAndCompress(&self) -> Vec<u8> {
        let encoded = bincode::encode_to_vec(&self, bincode::config::standard()).unwrap();
        let mut result = Vec::new();
        copy_encode(&encoded[..], &mut result, 6).unwrap();
        result
    }

    pub fn decodeAndDecompress(encoded: Vec<u8>) -> Self {
        let (wire, _): (CombinedFrame, _) =
        bincode::decode_from_slice(&encoded, bincode::config::standard()).unwrap();
        wire
    }
}

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
    pub data: Vec<u16>, // distances in meters
    pub timestamp: f64,
}


#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct ColorFrameSerializable {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>, // RGB8
    pub timestamp: f64,
}

#[derive(Encode, Decode)]
pub struct ImageForWire {
    pub image: Vec<u8>,
    pub timestamp: f64,
}


impl DepthFrameSerializable {
    pub fn new(frame: &DepthFrame, timestamp: f64) -> Self {
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
    pub fn new(frame: &ColorFrame, timestamp: f64) -> Self {
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



#[derive(Serialize, Deserialize, Encode, Decode, Debug, Clone)]
pub struct CombinedFrameWire {
    /// JPEG-compressed RGB image
    pub rgb_jpeg: Vec<u8>,
    /// Zstd-compressed depth buffer (u16)
    pub depth_zstd: Vec<u8>,
    pub width:  u16,
    pub height: u16,
    pub timestamp: f64,          // seconds, SYSTEM_TIME domain
}

impl CombinedFrameWire {
    /// build from already-captured RealSense frames
    pub fn from_frames(depth: &DepthFrame, color: &ColorFrame) -> Self {
        // ---------- depth ----------
        let depth_ser = DepthFrameSerializable::new(depth, depth.timestamp());
        let depth_bytes = bincode::encode_to_vec(&depth_ser, bincode::config::standard()).unwrap();
        let depth_zstd = {
            let mut v = Vec::new();
            copy_encode(&depth_bytes[..], &mut v, /*level*/ 3).unwrap();
            v
        };

        // ---------- colour ----------
        let rgb = {
            let mut tmp = Vec::<u8>::with_capacity((color.width() * color.height() * 3) as usize);
            for row in 0..color.height() {
                for col in 0..color.width() {
                    let RGB8Local { r, g, b } = get_data_from_pixel(color.get(col, row).unwrap());
                    tmp.extend([r, g, b]);
                }
            }
            tmp
        };
        let rgb_jpeg = compress_image::<Rgb<u8>>(
            &ImageBuffer::from_vec(color.width() as u32,
                                   color.height() as u32,
                                   rgb).unwrap(),
            /*quality*/ 75,
            Subsamp::Sub2x2,
        ).unwrap();

        Self {
            rgb_jpeg: rgb_jpeg.to_vec(),
            depth_zstd,
            width:  color.width()  as u16,
            height: color.height() as u16,
            timestamp: depth.timestamp(),  // pick one clock domain
        }
    }

    /// final packing for the wire
    pub fn encode(&self) -> Vec<u8> {
        let payload = bincode::encode_to_vec(self, bincode::config::standard()).unwrap();
        // a light Zstd pass mainly helps small RGB frames; level-1 keeps latency down
        let mut out = Vec::new();
        copy_encode(&payload[..], &mut out, 1).unwrap();
        out
    }

    pub fn decode(buf: &[u8]) -> Self {
        let raw = decode_all(buf).unwrap();
        let (me, _): (CombinedFrameWire, _) =
            bincode::decode_from_slice(&raw, bincode::config::standard()).unwrap();
        me
    }

    // helper to get fully-expanded data back out
    pub fn unpack(self) -> (Vec<u8>, Vec<u16>, u16, u16, f64) {
        let rgb_raw = turbojpeg::decompress_image::<Rgb<u8>>(&self.rgb_jpeg).unwrap().into_raw();
        let depth_raw = {
            let bytes = decode_all(&self.depth_zstd[..]).unwrap();
            let (d, _): (DepthFrameSerializable, _) =
                bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
            d.data
        };
        (rgb_raw, depth_raw, self.width, self.height, self.timestamp)
    }
}
