//! Bits shared by the public short-link routes.

use qrcode::render::svg;
use qrcode::QrCode;

use crate::error::AppError;

/// Minimum QR code edge length, in pixels.
const QR_MIN_DIMENSION: u32 = 200;

/// Render a URL as an inline SVG QR code.
///
/// `info` and `qr` each had their own copy of this, differing only in how they
/// handled failure: one propagated an error, the other silently rendered an
/// empty string.
pub fn qr_svg(url: &str) -> Result<String, AppError> {
    let qr = QrCode::new(url.as_bytes()).map_err(AppError::internal)?;
    Ok(qr
        .render::<svg::Color>()
        .min_dimensions(QR_MIN_DIMENSION, QR_MIN_DIMENSION)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_svg_renders() {
        let svg = qr_svg("https://atpr.to/@alice.test/abc123").unwrap();
        assert!(svg.contains("<svg"));
    }
}
