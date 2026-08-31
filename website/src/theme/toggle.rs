
use dioxus::prelude::*;

pub(crate) const INIT_THEME_JS: &str = r#"
(function () {
  try {
    var stored = localStorage.getItem('dp-theme');
    if (stored === 'dark' || stored === 'light') {
      document.documentElement.setAttribute('data-theme', stored);
    }
  } catch (e) {}
})();
"#;

const TOGGLE_JS: &str = r#"
(function () {
  var root = document.documentElement;

  var current = root.getAttribute('data-theme');
  if (!current) {
    var prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
    current = prefersDark ? 'dark' : 'light';
  }
  var next = current === 'dark' ? 'light' : 'dark';

  var apply = function () {
    root.setAttribute('data-theme', next);
    try { localStorage.setItem('dp-theme', next); } catch (e) {}
  };

  var reduceMotion = window.matchMedia
    && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (reduceMotion) {
    apply();
    return;
  }

  if (document.startViewTransition) {
    document.startViewTransition(apply);
    return;
  }

  root.classList.add('dp-theme-switching');
  apply();
  window.setTimeout(function () {
    root.classList.remove('dp-theme-switching');
  }, 260);
})();
"#;

#[component]
pub fn ThemeStyles() -> Element {
    use_hook(|| {
        let _ = super::THEME_CSS;
        document::eval(INIT_THEME_JS)
    });
    rsx! {}
}

#[component]
pub fn ThemeToggle() -> Element {
    rsx! {
        button {
            class: super::ICON_BTN,
            r#type: "button",
            aria_label: "Toggle dark mode",
            title: "Toggle dark mode",
            onclick: move |_| {
                document::eval(TOGGLE_JS);
            },
            "◐"
        }
    }
}
