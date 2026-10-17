//! The stylesheet, the script and the icon, built into the binary.

use std::sync::LazyLock;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use super::Incoming;
use crate::files::hex;

struct Asset {
    name: &'static str,
    content_type: &'static str,
    body: &'static [u8],
    etag: String,
}

/// By name, the order the asset version is computed in.
const FILES: [(&str, &str, &[u8]); 3] = [
    (
        "app.css",
        "text/css; charset=utf-8",
        include_bytes!("../../static/app.css"),
    ),
    (
        "app.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../static/app.js"),
    ),
    (
        "favicon.svg",
        "image/svg+xml",
        include_bytes!("../../static/favicon.svg"),
    ),
];

static ASSETS: LazyLock<Vec<Asset>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|(name, content_type, body)| Asset {
            name,
            content_type,
            body,
            etag: format!("\"{}\"", &hex(&Sha256::digest(body))[..16]),
        })
        .collect()
});

/// Changes with any asset, so a page asks for the new ones after an upgrade.
pub static VERSION: LazyLock<String> = LazyLock::new(|| {
    let mut digest = Sha256::new();
    for (_, _, body) in FILES {
        digest.update(body);
    }
    hex(&digest.finalize())[..10].to_string()
});

pub fn serve(request: &Incoming, name: &str) -> Option<Response> {
    let asset = ASSETS.iter().find(|asset| asset.name == name)?;
    // A link with the current version never changes; one without it is checked every time.
    let cache = if request.query("v") == Some(VERSION.as_str()) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let unchanged = request.header("if-none-match").is_some_and(|tags| {
        tags.split(',')
            .any(|tag| tag.trim() == asset.etag || tag.trim() == "*")
    });
    let mut response = if unchanged {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        ([(header::CONTENT_TYPE, asset.content_type)], asset.body).into_response()
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&asset.etag).expect("a hex digest is a valid header"),
    );
    Some(response)
}
