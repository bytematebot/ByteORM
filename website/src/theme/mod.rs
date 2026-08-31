
pub mod callout;
pub mod code;
pub mod nav;
pub mod progress;
pub mod toggle;

pub use callout::Callout;
pub use code::CodeBlock;
pub use nav::{Breadcrumbs, DocLink, Pager, Search, Sidebar, Toc};
pub use progress::{PageProgress, ProgressCard};
pub use toggle::{ThemeStyles, ThemeToggle};

use dioxus::prelude::*;
use dioxus_press::types::{PageMeta, SidebarItem, SiteConfig};

pub const THEME_CSS: Asset = asset!(
    "/assets/tailwind.css",
    CssAssetOptions::new().with_static_head(true).with_preload(true)
);

const INSET: &str = "px-[max(1.25rem,calc((100%-96rem)/2+1.25rem))]";

const BYTEMATE_LOGO: Asset = asset!("/assets/static/bytemate-logo.png");

const PROGRESS_PAGE: Option<(&str, &str)> = Some(("/roadmap", "v1.0 release"));

#[component]
pub fn DocsChrome(
    config: SiteConfig,
    sidebar: &'static [SidebarItem],
    pages: &'static [PageMeta],
    children: Element,
) -> Element {
    let mut menu_open = use_signal(|| false);
    let current = nav::current_route();
    let page = pages.iter().find(|page| page.route == current);
    let toc = page.map(|page| page.toc).unwrap_or(&[]);
    let description = page.and_then(|page| page.description).or(config.description);

    let initial_description = use_hook(|| description);
    use_effect(move || {
        if let Some(description) = description {
            document::eval(&set_description_js(description));
        }
    });

    let sidebar_visible = if menu_open() { "block" } else { "hidden md:block" };

    rsx! {
        ThemeStyles {}
        document::Title { "{page.map(|p| p.title).unwrap_or(config.title)} | {config.title}" }
        if let Some(description) = initial_description {
            document::Meta { name: "description", content: "{description}" }
        }

        SiteHeader {
            config,
            pages,
            on_menu: move |_| menu_open.toggle(),
        }

        div { class: "mx-auto grid max-w-[96rem] items-start grid-cols-1 md:grid-cols-[17rem_minmax(0,1fr)] xl:grid-cols-[17rem_minmax(0,1fr)_15rem]",
            aside {
                class: "{sidebar_visible} md:sticky md:top-[5rem] md:max-h-[calc(100vh-5rem)] overflow-y-auto border-b md:border-b-0 md:border-r border-line px-4 pt-6 pb-16 text-sm",
                nav { aria_label: "Documentation",
                    Sidebar { items: sidebar, current: current.clone(), depth: 0 }
                }
            }
            main { class: "min-w-0 px-5 pt-9 pb-24 md:px-12",
                article { class: "dp-article",
                    Breadcrumbs { pages, current: current.clone() }
                    {children}
                    Pager { pages, current: current.clone() }
                }
            }
            aside { class: "hidden xl:block sticky top-[5rem] max-h-[calc(100vh-5rem)] overflow-y-auto border-l border-line px-4 pt-6 pb-16 text-sm",
                Toc { items: toc }
            }
        }

        SiteFooter { config, pages }
    }
}

#[component]
pub fn SiteHeader(
    config: SiteConfig,
    pages: &'static [PageMeta],
    on_menu: Option<EventHandler<MouseEvent>>,
) -> Element {
    rsx! {
        header { class: "sticky top-0 z-20 grid min-h-[5rem] grid-cols-[minmax(0,1fr)_auto] grid-rows-[auto_auto] items-center gap-x-3 gap-y-3 border-b border-line bg-bg/90 py-3 backdrop-blur sm:gap-x-6 md:h-[5rem] md:min-h-0 md:grid-cols-[minmax(0,1fr)_minmax(16rem,28rem)_minmax(0,1fr)] md:grid-rows-1 md:gap-y-0 md:py-0 {INSET}",
            div { class: "col-start-1 row-start-1 flex min-w-0 items-center gap-2 sm:gap-5",
                if let Some(on_menu) = on_menu {
                    button {
                        class: "{ICON_BTN} md:hidden",
                        r#type: "button",
                        aria_label: "Toggle navigation",
                        onclick: move |event| on_menu.call(event),
                        "☰"
                    }
                }
                DocLink { route: "/".to_string(), class: "min-w-0 no-underline",
                    Wordmark { text: config.title.to_string(), class: "text-xl" }
                }
                DocLink {
                    route: config.docs_root.to_string(),
                    class: "{NAV_LINK} hidden sm:block",
                    "Docs"
                }
                DocLink {
                    route: format!("{}/roadmap", config.docs_root),
                    class: "{NAV_LINK} hidden sm:block",
                    "Roadmap"
                }
            }
            div { class: "col-span-2 row-start-2 flex min-w-0 justify-center md:col-span-1 md:col-start-2 md:row-start-1",
                Search { pages }
            }
            div { class: "col-start-2 row-start-1 flex items-center justify-end gap-2 md:col-start-3",
                if let Some((route, title)) = PROGRESS_PAGE {
                    div { class: "hidden lg:block",
                        ProgressCard {
                            pages,
                            route: format!("{}{route}", config.docs_root.trim_end_matches('/')),
                            title: title.to_string(),
                        }
                    }
                }
                ThemeToggle {}
                if let Some(repository) = config.repository {
                    a {
                        class: "{BTN}",
                        href: "{repository}",
                        rel: "noreferrer",
                        target: "_blank",
                        "GitHub"
                    }
                }
            }
        }
    }
}

#[component]
pub fn SiteFooter(config: SiteConfig, pages: &'static [PageMeta]) -> Element {
    let _ = pages;
    rsx! {
        footer { class: "border-t border-line bg-surface/30 {INSET}",
            div { class: "mx-auto grid max-w-[96rem] gap-x-8 gap-y-3 py-8 sm:grid-cols-[minmax(0,1fr)_auto] sm:grid-rows-[auto_auto] sm:items-center",
                DocLink {
                    route: "/".to_string(),
                    class: "inline-flex no-underline sm:col-start-1 sm:row-start-1",
                    Wordmark { text: config.title.to_string(), class: "text-base" }
                }
                if let Some(description) = config.description {
                    p { class: "min-w-0 max-w-md text-sm leading-relaxed text-muted sm:col-start-1 sm:row-start-2",
                        "{description}"
                    }
                }
                nav {
                    class: "flex flex-wrap items-center gap-1.5 sm:col-start-2 sm:row-start-1 sm:justify-self-end",
                    aria_label: "Footer",
                    DocLink { route: config.docs_root.to_string(), class: "{NAV_LINK}", "Docs" }
                    DocLink {
                        route: format!("{}/roadmap", config.docs_root),
                        class: "{NAV_LINK}",
                        "Roadmap"
                    }
                    if let Some(repository) = config.repository {
                        a {
                            class: "{NAV_LINK}",
                            href: "{repository}",
                            rel: "noreferrer",
                            target: "_blank",
                            "GitHub"
                        }
                    }
                }
                a {
                    class: "inline-flex items-center gap-3 no-underline sm:col-start-2 sm:row-start-2 sm:justify-self-end",
                    href: "https://bytemate.xyz",
                    rel: "noreferrer",
                    target: "_blank",
                    span { class: "text-xs uppercase tracking-[0.12em] text-faint",
                        "Project backed by"
                    }
                    img {
                        src: BYTEMATE_LOGO,
                        alt: "ByteMate",
                        class: "h-14 w-14 shrink-0 object-contain",
                    }
                }
            }
        }
    }
}

#[component]
pub fn Wordmark(text: String, #[props(default)] class: String) -> Element {
    let split = text.strip_suffix("ORM").map(|head| head.to_string());

    rsx! {
        span { class: "whitespace-nowrap font-brand font-extrabold tracking-tight {class}",
            match split {
                Some(head) => rsx! {
                    span { class: "text-brand", "{head}" }
                    span { class: "text-brand-alt", "ORM" }
                },
                None => rsx! {
                    span { class: "text-brand", "{text}" }
                },
            }
        }
    }
}

pub const BTN: &str = "inline-flex h-8 min-w-8 cursor-pointer items-center justify-center \
                       rounded-lg border border-line px-2 text-sm text-muted no-underline \
                       transition-colors hover:bg-surface hover:text-fg";

pub const ICON_BTN: &str = "inline-flex h-8 w-8 cursor-pointer items-center justify-center \
                            rounded-full border-0 bg-transparent p-0 text-lg leading-none \
                            text-muted transition-colors hover:bg-surface hover:text-fg \
                            focus:outline-none focus:ring-2 focus:ring-accent-soft";

pub const NAV_LINK: &str = "rounded-lg px-2 py-1 text-sm text-muted no-underline \
                            transition-colors hover:bg-surface hover:text-fg";

fn set_description_js(description: &str) -> String {
    format!(
        r#"(function () {{
  var tag = document.querySelector('meta[name="description"]');
  if (!tag) {{
    tag = document.createElement('meta');
    tag.setAttribute('name', 'description');
    document.head.appendChild(tag);
  }}
  tag.setAttribute('content', {});
}})();"#,
        code::js_string(description)
    )
}

pub fn titleize(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
