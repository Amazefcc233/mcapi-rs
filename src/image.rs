use image::{ImageBuffer, ImageEncoder, Rgba, RgbaImage};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_text_mut},
    rect::Rect,
};
use rusttype::{Font, Scale};

/// Theme for generated image. Defaults to light.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    TrueLight,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Light
    }
}

/// Generate an image for a server given request information and valid ping
/// data.
pub fn server_image(
    request: &crate::ServerImageRequest,
    ping: crate::types::ServerPing,
) -> Vec<u8> {
    let (background_color, text_color) = match request.theme.unwrap_or_default() {
        Theme::Light => (
            Rgba([255u8, 255u8, 255u8, 255u8]),
            Rgba([65u8, 105u8, 225u8, 255u8]),
        ),
        Theme::TrueLight => (
            Rgba([255u8, 255u8, 255u8, 255u8]),
            Rgba([0u8, 0u8, 0u8, 255u8]),
        ),
        Theme::Dark => (
            Rgba([0u8, 0u8, 0u8, 255u8]),
            Rgba([255u8, 255u8, 255u8, 255u8]),
        ),
    };

    let mut image = RgbaImage::new(325, 64);
    // let mut image = RgbaImage::new(400, 64);

    // let font_data: &[u8] = include_bytes!("../static/assets/Inconsolata-Regular.ttf");
    let font_data: &[u8] = include_bytes!("../static/assets/msyh.ttc");
    let font: Font<'static> = Font::try_from_bytes(font_data).unwrap();

    let fill = Rect::at(0, 0).of_size(325, 64);
    // let fill = Rect::at(0, 0).of_size(400, 64);
    draw_filled_rect_mut(&mut image, fill, background_color);

    let height = 16.0;
    let scale = Scale {
        x: height,
        y: height,
    };

    let title = if let Some(title) = &request.title {
        title.to_owned()
    } else if let Some(port) = request.port {
        format!("{}:{}", request.host, port)
    } else {
        request.host.to_owned()
    };

    draw_text_mut(&mut image, text_color, 68, 2, scale, &font, &title);

    // let status = if ping.online {
    //     format!("Online! {}/{} players", ping.players.now, ping.players.max)
    // } else {
    //     "Offline".to_owned()
    // };
    let status = if ping.online {
        format!("在线! {}/{} 玩家", ping.players.now, ping.players.max)
    } else {
        "离线".to_owned()
    };

    draw_text_mut(&mut image, text_color, 68, 18, scale, &font, &status);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mins = (now - ping.last_updated) / 60;

    // let updated = format!("Updated {} mins ago · mcapi.us", mins);
    let updated = format!("数据于 {} 分钟前更新 · 代码源自 mcapi.us", mins);

    draw_text_mut(
        &mut image,
        text_color,
        68,
        64 - 16 - 2,
        scale,
        &font,
        &updated,
    );

    let favicon = server_icon(&ping.favicon);

    let (x, y) = ((64 - favicon.width()) / 2, (64 - favicon.height()) / 2);

    image::imageops::overlay(&mut image, &favicon, x as i64, y as i64);

    encode_png(image)
}

/// Convert a base64-encoded server favicon into an image buffer.
pub fn server_icon(favicon: &Option<String>) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    favicon
        .as_deref()
        .and_then(|favicon| {
            // Some server seemed to be returning the base64 data with newlines
            // like it had been word wrapped in a text editor. We can replace
            // each newline with nothing to fix the issue.
            let b64 = &favicon[22..].replace('\n', "");

            let data = match base64::decode(b64) {
                Ok(data) => data,
                Err(err) => {
                    tracing::warn!("favicon could not be decoded as base64: {:?}", err);
                    return None;
                }
            };

            match image::load_from_memory(&data) {
                Ok(image) => Some(image.into_rgba8()),
                Err(err) => {
                    tracing::warn!("favicon could not be loaded as image: {:?}", err);
                    None
                }
            }
        })
        .unwrap_or_else(|| {
            let grass = Vec::from(include_bytes!("../static/assets/grass_sm.png") as &[u8]);
            image::load_from_memory(&grass).unwrap().into_rgba8()
        })
}

/// Encode an image buffer into a PNG.
pub fn encode_png(image: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<u8> {
    let mut buf: Vec<u8> = vec![];
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder
        .write_image(
            &image,
            image.width(),
            image.height(),
            image::ColorType::Rgba8,
        )
        .expect("Unable to encode PNG");

    buf
}
