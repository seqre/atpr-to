//! Bits shared by the public short-link routes.

use qrcode::render::svg;
use qrcode::QrCode;

use crate::error::AppError;

/// Minimum QR code edge length, in pixels.
const QR_MIN_DIMENSION: u32 = 200;

/// The prolog `qrcode` always emits.
///
/// Correct at the top of a standalone `.svg` document, invalid inside HTML —
/// a parser there treats it as a bogus comment. The crate offers no switch, so
/// the inline renderer removes it after the fact.
const XML_PROLOG: &str = r#"<?xml version="1.0" standalone="yes"?>"#;

/// Render a URL as a QR code, painted with the given SVG values.
///
/// `svg::Color` is interpolated straight into `fill="…"`, so these are any
/// legal paint value and not just hex colours.
fn render(url: &str, dark: &str, light: &str) -> Result<String, AppError> {
    let qr = QrCode::new(url.as_bytes()).map_err(AppError::internal)?;
    Ok(qr
        .render::<svg::Color>()
        .dark_color(svg::Color(dark))
        .light_color(svg::Color(light))
        .min_dimensions(QR_MIN_DIMENSION, QR_MIN_DIMENSION)
        .build())
}

/// Render a URL as a standalone SVG document.
///
/// Fixed black on white: this is served on its own and downloaded, where there
/// is no page around it to inherit anything from.
///
/// `info` and `qr` each had their own copy of this, differing only in how they
/// handled failure: one propagated an error, the other silently rendered an
/// empty string.
pub fn qr_svg(url: &str) -> Result<String, AppError> {
    render(url, "#000", "#fff")
}

/// Render a URL as an SVG fragment, for inlining into a page.
///
/// The modules paint with `currentColor` and the ground is `none`, so the code
/// takes its ink from the surrounding CSS and the page's own surface shows
/// through as the quiet zone.
///
/// That is a control, not an instruction to invert it: a light-on-dark QR
/// fails on a meaningful share of scanners, so the container is expected to
/// pin ink and ground in both renditions. Having it as `currentColor` means a
/// change of ink is one CSS declaration rather than a change here.
pub fn qr_svg_inline(url: &str) -> Result<String, AppError> {
    let mut svg = render(url, "currentColor", "none")?;
    if svg.starts_with(XML_PROLOG) {
        svg.replace_range(..XML_PROLOG.len(), "");
    }
    Ok(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://atpr.to/@alice.test/abc123";

    #[test]
    fn test_qr_svg_renders() {
        let svg = qr_svg(URL).unwrap();
        assert!(svg.contains("<svg"));
    }

    /// The standalone route serves this as a file and people download it, so
    /// it stays a self-contained document painted in real colours.
    #[test]
    fn test_standalone_qr_is_a_document_in_real_colours() {
        let svg = qr_svg(URL).unwrap();
        assert!(svg.starts_with(XML_PROLOG), "prolog belongs on the file");
        assert!(svg.contains(r##"fill="#000""##));
        assert!(svg.contains(r##"fill="#fff""##));
    }

    #[test]
    fn test_inline_qr_takes_its_ink_from_the_page() {
        let svg = qr_svg_inline(URL).unwrap();
        assert!(svg.contains(r#"fill="currentColor""#));
        assert!(
            svg.contains(r#"fill="none""#),
            "the page's own surface is the quiet zone"
        );
        assert!(!svg.contains("#000"), "no hardcoded ink survives");
    }

    /// An XML declaration inside `<body>` is parsed as a bogus comment.
    #[test]
    fn test_inline_qr_carries_no_xml_prolog() {
        let svg = qr_svg_inline(URL).unwrap();
        assert!(!svg.starts_with(XML_PROLOG));
        assert!(
            svg.starts_with("<svg"),
            "got: {}",
            &svg[..40.min(svg.len())]
        );
    }
}
