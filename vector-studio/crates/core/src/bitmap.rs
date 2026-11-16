//! Bitmaps placed in the drawing. The file's own bytes are kept (and saved base64), so
//! nothing is lost to re-encoding; decoding is left to the renderer and exporters.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use kurbo::Rect;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BitmapFormat {
    Png,
    Jpeg,
}

impl BitmapFormat {
    pub fn mime(self) -> &'static str {
        match self {
            BitmapFormat::Png => "image/png",
            BitmapFormat::Jpeg => "image/jpeg",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bitmap {
    pub format: BitmapFormat,
    /// Size in pixels.
    pub width: u32,
    pub height: u32,
    /// Where the image sits in the object's local space, in points.
    pub rect: Rect,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub data: Arc<Vec<u8>>,
}

impl Bitmap {
    /// Base64 of the file's bytes, for a `data:` URL.
    pub fn base64(&self) -> String {
        STANDARD.encode(self.data.as_slice())
    }
}

fn to_base64<S: Serializer>(data: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&STANDARD.encode(data.as_slice()))
}

fn from_base64<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
    let text = String::deserialize(d)?;
    STANDARD.decode(text).map(Arc::new).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_through_base64() {
        let b = Bitmap { format: BitmapFormat::Png, width: 2, height: 1, rect: Rect::new(0.0, 0.0, 2.0, 1.0), data: Arc::new(vec![0, 1, 2, 255]) };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("\"AAEC/w==\""), "{json}");
        assert_eq!(serde_json::from_str::<Bitmap>(&json).unwrap(), b);
    }
}
