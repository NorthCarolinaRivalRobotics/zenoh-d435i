use bincode::{Decode, Encode};
use realsense_rust::{frame::{ColorFrame, DepthFrame, ImageFrame, PixelKind}, kind};
use serde::{Serialize, Deserialize};
use snap::raw::{Decoder, Encoder};
use turbojpeg::{compress_image, decompress_image, image::{ImageBuffer, Rgb}, OwnedBuf, PixelFormat, Subsamp};

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

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct DepthFrameSerializable {
    pub width: usize,
    pub height: usize,
    pub timestamp: f64,
    pub data: Vec<f32>, // distances in meters
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
        let mut data: Vec<f32> = Vec::new();
        for row in 0..frame.height() {
            for col in 0..frame.width() {
                data.push(frame.distance(col, row).unwrap());
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
        // use snap here
        let mut encoder = Encoder::new();
        let compressed_encoded = encoder.compress_vec(&encoded).unwrap();
        compressed_encoded
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