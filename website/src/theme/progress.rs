
use dioxus::prelude::*;
use dioxus_press::types::{PageMeta, Tasks};

use super::{nav, DocLink};

#[component]
pub fn Progress(percent: u8, #[props(default)] label: Option<String>) -> Element {
    let percent = percent.min(100);
    let fill = format!("right: calc({}% - 1px)", 100 - percent);
    let text = label.unwrap_or_else(|| format!("{percent}% complete"));

    rsx! {
        div { class: "dp-progress-track",
            role: "progressbar",
            aria_valuemin: "0",
            aria_valuemax: "100",
            aria_valuenow: "{percent}",
            aria_label: "{text}",
            div { class: "dp-progress-fill", style: "{fill}" }
        }
    }
}

#[component]
pub fn ProgressCard(
    pages: &'static [PageMeta],
    route: String,
    title: String,
) -> Element {
    let tasks = page_tasks(pages, &route);
    if !tasks.any() {
        return rsx! {};
    }
    let percent = tasks.percent();

    rsx! {
        DocLink { route: route.clone(), class: "dp-progress-card w-72 shrink-0 text-left",
            div { class: "flex items-center justify-between gap-2 font-medium",
                span { class: "truncate font-mono text-sm", "{title}" }
                span { class: "shrink-0 text-sm font-semibold text-progress",
                    "{percent}%"
                }
            }
            div { class: "mt-2",
                Progress {
                    percent,
                    label: format!("{title}: {} of {} done", tasks.done, tasks.total),
                }
            }
        }
    }
}

pub fn page_tasks(pages: &'static [PageMeta], route: &str) -> Tasks {
    dioxus_press::types::page(pages, route)
        .map(|page| page.tasks)
        .unwrap_or_default()
}

#[component]
pub fn PageProgress(pages: &'static [PageMeta]) -> Element {
    let tasks = page_tasks(pages, &nav::current_route());
    if !tasks.any() {
        return rsx! {};
    }

    rsx! {
        div { class: "my-6",
            div { class: "mb-2 flex items-baseline justify-between text-sm text-muted",
                span { "{tasks.done} of {tasks.total} done" }
                span { class: "font-semibold text-progress", "{tasks.percent()}%" }
            }
            Progress { percent: tasks.percent() }
        }
    }
}
