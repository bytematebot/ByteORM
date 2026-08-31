
use dioxus::prelude::*;

#[component]
pub fn Counter(initial: i32) -> Element {
    let mut value = use_signal(|| initial);

    rsx! {
        div {
            style: "display:flex;gap:.75rem;align-items:center;margin:1.2rem 0",
            button {
                class: "dp-btn",
                onclick: move |_| value += 1,
                "Increment"
            }
            span { "Count: {value}" }
        }
    }
}
