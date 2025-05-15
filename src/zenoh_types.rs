use bincode::{Decode, Encode};
use realsense_rust::{frame::{ColorFrame, DepthFrame, ImageFrame, PixelKind}, kind};
use serde::{Serialize, Deserialize};
use snap::raw::{Decoder, Encoder};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum ImageEncoding {
    RGB8,
    Z16,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Encode, Decode)]
pub struct RGB8 {
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
    pub data: Vec<RGB8>, // RGB8
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
        let mut data: Vec<RGB8> = Vec::new();
        for row in 0..frame.height() {
            for col in 0..frame.width() {
                data.push(get_data_from_pixel(frame.get(col, row).unwrap()));
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
    pub fn decodeAndDecompress(encoded: Vec<u8>) -> Self {
        let mut decoder = Decoder::new();
        let decompressed_encoded = decoder.decompress_vec(&encoded).unwrap();
        let decoded: (Self, usize) = bincode::decode_from_slice(&decompressed_encoded, bincode::config::standard()).unwrap();   
        decoded.0
    }
}

pub fn get_data_from_pixel(pixel: PixelKind<'_>) ->RGB8 {
    match pixel {
        PixelKind::Bgr8 { b, g, r } => RGB8 { b: *b, g: *g, r: *r },
        _ => panic!("Unsupported pixel format"),
    }
}