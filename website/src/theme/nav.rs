
use dioxus_press::types::{PageMeta, SidebarItem, TocItem};
use dioxus::prelude::*;
use dioxus_router::navigation::NavigationTarget;
use dioxus_router::router;

const LINK: &str = "-ml-2 my-px block rounded-lg px-2 py-1.5 leading-snug text-muted \
                    no-underline hover:bg-surface hover:text-fg";
const LINK_ACTIVE: &str = "-ml-2 my-px block rounded-lg bg-accent-soft px-2 py-1.5 \
                           leading-snug font-medium text-accent no-underline";
const CRUMB: &str = "text-muted no-underline hover:text-accent";
const PAGER: &str = "flex-1 rounded-lg border border-line px-4 py-3 text-fg no-underline \
                     hover:border-accent";
const PAGER_LABEL: &str = "block text-[0.72rem] uppercase tracking-wide text-faint";

pub fn current_route() -> String {
    normalize(&router().full_route_string())
}

pub fn normalize(route: &str) -> String {
    let route = route.split(['?', '#']).next().unwrap_or("/");
    let trimmed = route.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn go(route: &str) {
    router().push(NavigationTarget::Internal(route.to_string()));
}

#[component]
pub fn DocLink(route: String, class: String, children: Element) -> Element {
    let target = route.clone();
    rsx! {
        a {
            href: "{route}",
            class: "{class}",
            onclick: move |event: MouseEvent| {
                if event.modifiers().is_empty() {
                    event.prevent_default();
                    go(&target);
                }
            },
            {children}
        }
    }
}

fn contains(items: &[SidebarItem], current: &str) -> bool {
    items.iter().any(|item| {
        item.route == Some(current) || contains(item.items, current)
    })
}

#[component]
pub fn Sidebar(items: &'static [SidebarItem], current: String, depth: u8) -> Element {
    let list = if depth == 0 {
        "list-none m-0 p-0"
    } else {
        "list-none m-0 ml-1 border-l border-line pl-3"
    };

    rsx! {
        ul { class: "{list}",
            for (index, item) in items.iter().enumerate() {
                li { key: "{index}",
                    if item.items.is_empty() {
                        DocLink {
                            route: item.route.unwrap_or("/").to_string(),
                            class: if item.route == Some(current.as_str()) { LINK_ACTIVE } else { LINK },
                            "{item.title}"
                        }
                    } else {
                        SidebarGroup { item: *item, current: current.clone(), depth }
                    }
                }
            }
        }
    }
}

#[component]
fn SidebarGroup(item: SidebarItem, current: String, depth: u8) -> Element {
    let holds_current = use_memo({
        let current = current.clone();
        move || item.route == Some(current.as_str()) || contains(item.items, &current)
    });
    let mut open = use_signal(|| true);
    use_effect(move || {
        if holds_current() {
            open.set(true);
        }
    });

    let row = if depth == 0 {
        "flex items-center gap-1 mt-4 mb-1"
    } else {
        "flex items-center gap-1 mt-1.5 mb-0.5"
    };
    let label = if depth == 0 {
        "flex-1 py-0.5 text-xs font-bold uppercase tracking-widest text-faint no-underline"
    } else {
        "flex-1 py-0.5 text-sm font-medium text-fg no-underline hover:text-accent"
    };

    rsx! {
        div { class: "my-1",
            div { class: "{row}",
                button {
                    class: "inline-flex h-4 w-4 cursor-pointer items-center justify-center border-0 bg-transparent p-0 text-faint",
                    r#type: "button",
                    aria_expanded: "{open()}",
                    aria_label: "Toggle section",
                    onclick: move |_| open.toggle(),
                    span {
                        class: if open() {
                            "inline-block text-base leading-none transition-transform rotate-90"
                        } else {
                            "inline-block text-base leading-none transition-transform"
                        },
                        "\u{203A}"
                    }
                }
                match item.route {
                    Some(route) => rsx! {
                        DocLink {
                            route: route.to_string(),
                            class: if item.route == Some(current.as_str()) {
                                format!("{label} rounded-lg bg-accent-soft px-2 text-accent")
                            } else {
                                label.to_string()
                            },
                            "{item.title}"
                        }
                    },
                    None => rsx! { span { class: "{label}", "{item.title}" } },
                }
            }
            if open() {
                Sidebar { items: item.items, current: current.clone(), depth: depth + 1 }
            }
        }
    }
}

#[component]
pub fn Toc(items: &'static [TocItem]) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "mb-2 text-xs font-semibold uppercase tracking-widest text-faint", "On this page" }
        ul { class: "list-none m-0 p-0 border-l border-line",
            for item in items.iter() {
                li { key: "{item.id}",
                    a {
                        class: if item.level >= 3 {
                            "block py-1 pl-6 pr-3 text-[0.82rem] text-muted no-underline hover:text-accent"
                        } else {
                            "block py-1 px-3 text-muted no-underline hover:text-accent"
                        },
                        href: "#{item.id}",
                        "{item.title}"
                    }
                }
            }
        }
    }
}

#[component]
pub fn Breadcrumbs(pages: &'static [PageMeta], current: String) -> Element {
    if current == "/" {
        return rsx! {};
    }
    let mut crumbs: Vec<(String, String)> = Vec::new();
    let mut prefix = String::new();
    for segment in current.split('/').filter(|s| !s.is_empty()) {
        prefix.push('/');
        prefix.push_str(segment);
        let title = pages
            .iter()
            .find(|page| page.route == prefix)
            .map(|page| page.title.to_string())
            .unwrap_or_else(|| super::titleize(segment));
        crumbs.push((prefix.clone(), title));
    }

    rsx! {
        nav { class: "mb-5 flex flex-wrap gap-1.5 text-xs text-faint", aria_label: "Breadcrumb",
            DocLink { route: "/".to_string(), class: CRUMB, "Home" }
            for (index, (route, title)) in crumbs.iter().enumerate() {
                span { key: "{route}", "/" }
                if index + 1 == crumbs.len() {
                    span { "{title}" }
                } else {
                    DocLink { route: route.clone(), class: CRUMB, "{title}" }
                }
            }
        }
    }
}

#[component]
pub fn Pager(pages: &'static [PageMeta], current: String) -> Element {
    let index = pages.iter().position(|page| page.route == current);
    let Some(index) = index else {
        return rsx! {};
    };
    let prev = pages[..index].iter().rev().find(|page| page.pager);
    let next = pages[index + 1..].iter().find(|page| page.pager);
    if prev.is_none() && next.is_none() {
        return rsx! {};
    }

    rsx! {
        nav { class: "mt-14 flex gap-4 border-t border-line pt-6",
            if let Some(prev) = prev {
                DocLink { route: prev.route.to_string(), class: PAGER,
                    span { class: PAGER_LABEL, "Previous" }
                    "{prev.title}"
                }
            }
            if let Some(next) = next {
                DocLink { route: next.route.to_string(), class: "{PAGER} text-right",
                    span { class: PAGER_LABEL, "Next" }
                    "{next.title}"
                }
            }
        }
    }
}

#[component]
pub fn Search(pages: &'static [PageMeta]) -> Element {
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| false);

    let results = use_memo(move || search(pages, &query()));

    rsx! {
        div { class: "relative w-full min-w-0 max-w-[28rem] md:min-w-[20rem]",
            span {
                class: "pointer-events-none absolute inset-y-0 left-3 flex items-center text-muted",
                aria_hidden: "true",
                svg {
                    class: "h-4 w-4",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "11", cy: "11", r: "6.5" }
                    path { d: "m16 16 4 4" }
                }
            }
            input {
                class: "h-10 w-full rounded-xl border border-line bg-surface/80 pl-10 pr-4 text-sm text-fg shadow-sm transition-colors placeholder:text-muted/70 focus:border-accent focus:bg-bg focus:outline-none focus:ring-2 focus:ring-accent-soft",
                r#type: "search",
                placeholder: "Search docs…",
                aria_label: "Search documentation",
                value: "{query}",
                oninput: move |event| {
                    query.set(event.value());
                    open.set(true);
                },
                onfocusin: move |_| open.set(true),
                onkeydown: move |event| {
                    if event.key() == Key::Escape {
                        open.set(false);
                    }
                },
            }
            if open() && !query().trim().is_empty() {
                div { class: "absolute inset-x-0 top-12 z-30 max-h-[60vh] overflow-y-auto rounded-xl border border-line bg-bg p-1.5 shadow-xl",
                    if results().is_empty() {
                        div { class: "p-2 text-sm text-muted", "No results for “{query}”" }
                    }
                    for hit in results().iter() {
                        div {
                            key: "{hit.route}",
                            class: "block cursor-pointer rounded-md px-2.5 py-2 text-fg hover:bg-surface",
                            onclick: {
                                let route = hit.route.to_string();
                                move |_| {
                                    go(&route);
                                    open.set(false);
                                    query.set(String::new());
                                }
                            },
                            div { class: "text-sm font-medium", "{hit.title}" }
                            div { class: "truncate text-xs text-muted", "{hit.snippet}" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Hit {
    pub route: &'static str,
    pub title: &'static str,
    pub snippet: String,
}

pub fn search(pages: &'static [PageMeta], query: &str) -> Vec<Hit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut hits: Vec<(usize, Hit)> = Vec::new();
    for page in pages {
        let title = page.title.to_lowercase();
        let text = page.text.to_lowercase();
        let (score, at) = if let Some(at) = title.find(&needle) {
            (at, None)
        } else if let Some(at) = text.find(&needle) {
            (1000 + at, Some(at))
        } else {
            continue;
        };
        hits.push((
            score,
            Hit {
                route: page.route,
                title: page.title,
                snippet: snippet(page, at),
            },
        ));
    }

    hits.sort_by_key(|(score, _)| *score);
    hits.truncate(8);
    hits.into_iter().map(|(_, hit)| hit).collect()
}

fn snippet(page: &PageMeta, at: Option<usize>) -> String {
    let Some(at) = at else {
        return page
            .description
            .map(str::to_string)
            .unwrap_or_else(|| truncate(page.text, 90));
    };
    let start = floor_boundary(page.text, at.saturating_sub(40));
    truncate(&page.text[start..], 110)
}

fn truncate(text: &str, max: usize) -> String {
    let end = floor_boundary(text, max.min(text.len()));
    let mut out = text[..end].trim().to_string();
    if end < text.len() {
        out.push('…');
    }
    out
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    static PAGES: &[PageMeta] = &[
        PageMeta {
            route: "/",
            title: "Introduction",
            description: None,
            toc: &[],
            text: "Welcome to the docs",
        },
        PageMeta {
            route: "/signals",
            title: "Signals",
            description: None,
            toc: &[],
            text: "Signals store reactive state and implement Copy",
        },
    ];

    #[test]
    fn normalizes_routes() {
        assert_eq!(normalize("/guides/"), "/guides");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("/a?b=1#c"), "/a");
    }

    #[test]
    fn title_matches_outrank_body_matches() {
        let hits = search(PAGES, "signals");
        assert_eq!(hits[0].route, "/signals");
    }

    #[test]
    fn finds_body_only_matches() {
        let hits = search(PAGES, "reactive");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("reactive"));
    }
}
