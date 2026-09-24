//! Stand-in for screens landing in M6-M9, so the shell is navigable now.

use dioxus::prelude::*;

#[component]
pub fn Placeholder(title: String, milestone: String) -> Element {
    rsx! {
        div { class: "placeholder",
            div { class: "empty__icon", "🚧" }
            div { class: "empty__title", "{title}" }
            p { "Arrives in {milestone}." }
        }
    }
}
