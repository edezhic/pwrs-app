fn shared_routes() -> pwrs::Router {
    pwrs::Router::new().route("/", pwrs::get(|| async { pwrs::maud_to_response(
        maud::html!(
            (pwrs::head("Prest app", Some(maud::html!(link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/water.css@2/out/dark.css"{}))))
            body {
                h1{"Progressive RESTful application"}
            }
        )
    )}))
}

#[cfg(feature = "host")]
#[derive(rust_embed::RustEmbed, Clone)]
#[folder = "$OUT_DIR/assets"]
struct Assets;

#[cfg(feature = "host")]
#[tokio::main]
async fn main() {
    let service = shared_routes().layer(pwrs::host::embed(Assets));
    pwrs::host::serve(service, 80).await.unwrap();
}

#[cfg(feature = "sw")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn serve(sw: pwrs::sw::ServiceWorkerGlobalScope, event: pwrs::sw::FetchEvent) {
    pwrs::sw::process_fetch_event(shared_routes, sw, event).await
}