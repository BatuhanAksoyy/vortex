//! Pictures to and from the API's `image` blocks, whose bytes travel as
//! base64.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use kiln_agent::Picture;
use serde_json::{Value, json};

use crate::wire::ImageSource;

/// `None` for a source that holds no bytes, such as a URL, or bad base64.
pub fn decode(source: &ImageSource) -> Option<Picture> {
    if source.kind != "base64" {
        return None;
    }
    let data = STANDARD.decode(source.data.as_bytes()).ok()?;
    Some(Picture {
        media_type: source.media_type.clone(),
        data: data.into(),
    })
}

pub fn block(picture: &Picture) -> Value {
    json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": picture.media_type,
            "data": STANDARD.encode(&picture.data),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_survives_the_way_there_and_back() {
        let picture = Picture {
            media_type: "image/png".into(),
            data: vec![0x89, b'P', b'N', b'G', 0, 255].into(),
        };
        let block = block(&picture);
        let source: ImageSource =
            serde_json::from_value(block["source"].clone()).unwrap_or_default();
        assert_eq!(decode(&source), Some(picture));
    }
}
