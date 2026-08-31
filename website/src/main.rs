mod components;
mod landing;

mod theme;

#[path = "../generated/docs.rs"]
mod docs;

use dioxus::prelude::*;
use docs::Route;

fn main() {
    let builder = dioxus::LaunchBuilder::new();

    #[cfg(feature = "server")]
    let builder = builder.with_cfg(
        dioxus::server::ServeConfig::default().incremental(
            dioxus::server::IncrementalRendererConfig::default().static_dir("public"),
        ),
    );

    builder.launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        dioxus_router::Router::<Route> {}
    }
}

#[cfg(feature = "server")]
#[server(endpoint = "static_routes")]
async fn static_routes() -> ServerFnResult<Vec<String>> {
    Ok(Route::static_routes().iter().map(ToString::to_string).collect())
}
