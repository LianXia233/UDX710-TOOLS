//! 静态资源内嵌（rust-embed）。
//!
//! 报告 03/05 章：静态资源由 Rust 服务内嵌托管，支持单二进制发布，
//! 设备端不再需要 Node.js 运行时或单独的 web/dist 目录。

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/"]
pub struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }
    // 防路径穿越；内嵌资源本身也是编译期固定的。
    if path.contains("..") || path.contains('\0') {
        return StatusCode::BAD_REQUEST.into_response();
    }

    // /api/* 未命中路由时必须返回 404 JSON，绝不能回退成 index.html：
    // 否则前端拿到的 response.ok 为真、body 却是 HTML，JSON 解析失败后
    // 报错信息会失真（表现为「页面报错但看不出原因」）。
    if path == "api" || path.starts_with("api/") {
        let body = serde_json::json!({
            "status": "error",
            "message": "接口不存在",
            "code": "not_found"
        });
        return (StatusCode::NOT_FOUND, axum::Json(body)).into_response();
    }

    serve(&path)
}

fn serve(path: &str) -> Response {
    if let Some(file) = Assets::get(path) {
        return file_response(path, file.data.into_owned());
    }
    // SPA / hash 路由回退到入口页。
    if let Some(index) = Assets::get("index.html") {
        return file_response("index.html", index.data.into_owned());
    }
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

fn file_response(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut resp = Body::from(bytes).into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime.as_ref()) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_embedded() {
        assert!(Assets::get("index.html").is_some());
    }
}
