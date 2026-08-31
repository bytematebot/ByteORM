
use dioxus::prelude::*;

#[component]
pub fn Callout(kind: Option<String>, title: Option<String>, children: Element) -> Element {
    let kind = kind.unwrap_or_else(|| "note".to_string());
    let label = title.unwrap_or_else(|| kind.to_uppercase());
    let (frame, heading) = palette(&kind);

    rsx! {
        div { class: "my-5 rounded-lg border border-l-[3px] px-4 py-3.5 [&>:first-child]:mt-0 [&>:last-child]:mb-0 {frame}",
            div { class: "mb-1.5 text-xs font-semibold uppercase tracking-wide {heading}", "{label}" }
            {children}
        }
    }
}

fn palette(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "tip" => (
            "border-line border-l-emerald-600 bg-emerald-500/8",
            "text-emerald-700 dark:text-emerald-400",
        ),
        "info" => (
            "border-line border-l-blue-600 bg-blue-500/8",
            "text-blue-700 dark:text-blue-400",
        ),
        "warning" => (
            "border-line border-l-amber-600 bg-amber-500/10",
            "text-amber-700 dark:text-amber-400",
        ),
        "danger" => (
            "border-line border-l-rose-600 bg-rose-500/8",
            "text-rose-700 dark:text-rose-400",
        ),
        _ => ("border-line border-l-slate-500 bg-surface", "text-muted"),
    }
}
