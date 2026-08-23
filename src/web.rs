use actix_web::{HttpRequest, HttpResponse};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Frontend;

pub async fn assets(req: HttpRequest) -> HttpResponse {
    let path = req.match_info().query("path");
    let requested = if path.is_empty() { "index.html" } else { path };
    let asset = Frontend::get(requested).or_else(|| Frontend::get("index.html"));
    match asset {
        Some(asset) => {
            let content_type = match requested.rsplit('.').next() {
                Some("js") => "text/javascript; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("svg") => "image/svg+xml",
                _ => "text/html; charset=utf-8",
            };
            HttpResponse::Ok()
                .insert_header(("content-type", content_type))
                .body(asset.data)
        }
        None => HttpResponse::ServiceUnavailable()
            .body("Frontend assets have not been built. Run `pnpm --dir frontend build`."),
    }
}
