
use crate::theme::{DocLink, SiteFooter, SiteHeader, ThemeStyles, Wordmark};
use dioxus::prelude::*;

use crate::docs::{pages, SITE_CONFIG};

#[component]
pub fn Landing() -> Element {
    rsx! {
        ThemeStyles {}
        document::Title { "{SITE_CONFIG.title}" }

        div { class: "flex min-h-screen flex-col",
            SiteHeader { config: SITE_CONFIG, pages: pages() }

            main { class: "flex-1 px-5 pt-20 pb-8",

            section { class: "mx-auto max-w-3xl text-center",
                h1 { class: "m-0 mb-8",
                    Wordmark {
                        text: SITE_CONFIG.title.to_string(),
                        class: "text-[clamp(2.75rem,8vw,4.5rem)] leading-none",
                    }
                }

                p { class: "mx-auto mb-8 max-w-xl text-lg text-muted",
                    "ByteORM is a fast, type-safe ORM for PostgreSQL in Rust."
                }

                div { class: "flex flex-wrap items-center justify-center gap-3",
                    DocLink {
                        route: SITE_CONFIG.docs_root.to_string(),
                        class: "{CTA} border-brand-alt bg-brand-alt text-on-brand hover:brightness-95",
                        "Getting Started"
                    }
                    if let Some(repository) = SITE_CONFIG.repository {
                        a {
                            class: "{CTA} border-line text-fg hover:border-accent",
                            href: "{repository}",
                            rel: "noreferrer",
                            target: "_blank",
                            "Source"
                        }
                    }
                }
            }

            section { class: "mx-auto mt-8 max-w-6xl rounded-2xl bg-surface px-5 py-8 sm:px-14 sm:py-12",
                div { class: "grid grid-cols-1 gap-y-8 sm:grid-cols-2 sm:gap-x-12 sm:gap-y-10",
                    Feature {
                        icon: rsx! {
                            Icon {
                                rect { x: "3", y: "4", width: "18", height: "16", rx: "2" }
                                path { d: "M3 9h18M9 9v11" }
                            }
                        },
                        title: "Schema-first",
                        body: "A Prisma-like schema file becomes a fully typed client crate.",
                    }
                    Feature {
                        icon: rsx! {
                            Icon {
                                path { d: "M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6l7-3z" }
                                path { d: "M9.5 12l1.8 1.8L15 10" }
                            }
                        },
                        title: "Guarded migrations",
                        body: "Anything that would drop data is refused until you allow it.",
                    }
                    Feature {
                        icon: rsx! {
                            Icon {
                                path { d: "M9 5c-2 0-3 1-3 3v2c0 1-1 2-2 2 1 0 2 1 2 2v2c0 2 1 3 3 3" }
                                path { d: "M15 5c2 0 3 1 3 3v2c0 1 1 2 2 2-1 0-2 1-2 2v2c0 2-1 3-3 3" }
                            }
                        },
                        title: "Typed client",
                        body: "Builders over your own models, pooled and compiler-checked.",
                    }
                    Feature {
                        icon: rsx! {
                            Icon {
                                path { d: "M21 8l-9-5-9 5 9 5 9-5z" }
                                path { d: "M3 8v8l9 5 9-5V8" }
                                path { d: "M12 13v8" }
                            }
                        },
                        title: "Self-contained client crate",
                        body: "Version-matched macros ship inside it. Nothing to install alongside.",
                    }
                }
            }

        }
            SiteFooter { config: SITE_CONFIG, pages: pages() }
        }
    }
}

const CTA: &str = "inline-flex h-12 items-center rounded-lg border px-6 text-base \
                   font-semibold no-underline transition";

#[component]
fn Feature(icon: Element, title: String, body: String) -> Element {
    rsx! {
        div { class: "min-w-0 text-left",
            div { class: "flex items-center gap-3",
                span { class: "shrink-0 text-accent", {icon} }
                h3 { class: "m-0 min-w-0 text-base font-semibold", "{title}" }
            }
            p { class: "m-0 mt-4 text-sm leading-relaxed text-muted", "{body}" }
        }
    }
}

#[component]
fn Icon(children: Element) -> Element {
    rsx! {
        svg {
            class: "h-5 w-5",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.75",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            {children}
        }
    }
}
