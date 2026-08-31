
use dioxus_press::types::Token;
use dioxus::prelude::*;

#[component]
pub fn CodeBlock(lang: Option<String>, code: String, tokens: &'static [Token]) -> Element {
    let copy_source = code.clone();
    rsx! {
        div { class: "group relative my-5 overflow-hidden rounded-lg border border-line bg-code",
            if let Some(lang) = lang.clone() {
                span { class: "absolute right-12 top-2 font-mono text-[0.7rem] uppercase text-faint", "{lang}" }
            }
            button {
                class: "absolute right-1.5 top-1.5 cursor-pointer rounded-md border border-line bg-bg px-2 py-0.5 text-[0.72rem] text-muted opacity-0 transition-opacity group-hover:opacity-100 focus:opacity-100 hover:text-fg",
                r#type: "button",
                aria_label: "Copy code",
                onclick: move |_| {
                    document::eval(&copy_script(&copy_source));
                },
                "Copy"
            }
            pre { class: "m-0 overflow-x-auto p-4 font-mono text-[0.85rem] leading-relaxed",
                code { class: "font-[inherit]",
                    for (index, token) in tokens.iter().enumerate() {
                        span {
                            key: "{index}",
                            class: "text-(--dp-tok-light) dark:text-(--dp-tok-dark)",
                            style: "--dp-tok-light:{token.light};--dp-tok-dark:{token.dark}",
                            "{token.text}"
                        }
                    }
                }
            }
        }
    }
}

fn copy_script(code: &str) -> String {
    format!(
        "navigator.clipboard && navigator.clipboard.writeText({});",
        js_string(code)
    )
}

pub(crate) fn js_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_js_strings() {
        assert_eq!(js_string("a\"b\n"), "\"a\\\"b\\n\"");
        assert_eq!(js_string("</script>"), "\"</script>\"");
    }
}
