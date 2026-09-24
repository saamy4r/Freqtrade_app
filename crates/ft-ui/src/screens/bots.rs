//! The Bots screen: list, add, delete, reorder, liveness.

use dioxus::prelude::*;

use ft_types::api::{AddBotRequest, BotSummary, ErrorKind};

use crate::api;
use crate::components::ErrorView;
use crate::state::App;

/// Liveness of one bot, for its dot.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Liveness {
    Unknown,
    Online,
    Offline,
}

#[component]
pub fn Bots() -> Element {
    let mut app = App::get();
    let mut adding = use_signal(|| false);
    let mut confirm_delete = use_signal(|| None::<BotSummary>);

    // Pinged when the bot list changes. One bot being down is ordinary, so this
    // never surfaces an error -- it just colours a dot.
    //
    // The signal is read in the closure body so it registers as a dependency.
    // Capturing a clone taken outside would make this run once and never
    // refresh after a bot is added or removed. Note it must NOT be read inside
    // the async block either: that subscribes the resource to something its
    // own completion changes, and it restarts forever.
    let liveness = use_resource(move || {
        let ids: Vec<String> = app.bots.read().iter().map(|b| b.id.clone()).collect();
        async move {
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                let alive = api::ping(&id).await.map(|p| p.online).unwrap_or(false);
                out.push((
                    id,
                    if alive {
                        Liveness::Online
                    } else {
                        Liveness::Offline
                    },
                ));
            }
            out
        }
    });

    let status_of = |id: &str| -> Liveness {
        liveness
            .read_unchecked()
            .as_ref()
            .and_then(|list| list.iter().find(|(bot, _)| bot == id).map(|(_, s)| *s))
            .unwrap_or(Liveness::Unknown)
    };

    let bots = app.bots.read().clone();
    let active = app.active.read().clone();
    let loading = *app.loading.read();

    rsx! {
        div { class: "content",
            if let Some(message) = app.error.read().clone() {
                ErrorView {
                    kind: ErrorKind::Internal,
                    message,
                    on_retry: move |_| {
                        spawn(async move {
                            let mut app = app;
                            app.reload_bots(None).await;
                        });
                    },
                }
            } else if loading {
                div { class: "empty", span { class: "spinner" } }
            } else if bots.is_empty() {
                div { class: "empty",
                    div { class: "empty__icon", "🤖" }
                    div { class: "empty__title", "No bots yet" }
                    p { "Add a Freqtrade instance to start monitoring it." }
                    div { style: "margin-top:18px",
                        button {
                            class: "btn",
                            onclick: move |_| adding.set(true),
                            "Add your first bot"
                        }
                    }
                }
            } else {
                BotList {
                    bots: bots.clone(),
                    active: active.clone(),
                    status: bots.iter().map(|b| status_of(&b.id)).collect::<Vec<_>>(),
                    on_select: move |id: String| app.select(id),
                    on_delete: move |bot: BotSummary| confirm_delete.set(Some(bot)),
                }
                div { style: "margin-top:16px",
                    button {
                        class: "btn btn--ghost",
                        onclick: move |_| adding.set(true),
                        "Add another bot"
                    }
                }
            }
        }

        if adding() {
            AddBotDialog {
                on_close: move |_| adding.set(false),
                on_added: move |bot: BotSummary| {
                    adding.set(false);
                    spawn(async move {
                        let mut app = app;
                        app.reload_bots(Some(bot.id.clone())).await;
                        app.select(bot.id);
                    });
                },
            }
        }

        if let Some(bot) = confirm_delete.read().clone() {
            ConfirmDelete {
                bot: bot.clone(),
                on_cancel: move |_| confirm_delete.set(None),
                on_confirm: move |id: String| {
                    confirm_delete.set(None);
                    spawn(async move {
                        let mut app = app;
                        let _ = api::delete_bot(&id).await;
                        app.reload_bots(None).await;
                    });
                },
            }
        }
    }
}

/// The reorderable list.
///
/// Uses HTML drag-and-drop with an explicit grip, matching the legacy
/// `ReorderableDragStartListener`: dragging anywhere on the row would fight
/// with tap-to-switch and, on a phone, with scrolling.
#[component]
fn BotList(
    bots: Vec<BotSummary>,
    active: Option<String>,
    status: Vec<Liveness>,
    on_select: EventHandler<String>,
    on_delete: EventHandler<BotSummary>,
) -> Element {
    let mut dragging = use_signal(|| None::<usize>);
    let mut over = use_signal(|| None::<usize>);
    // Captured here, in the component body. `App::get` wraps `use_context`,
    // which is a hook: calling it from inside an event handler runs a hook
    // outside render and corrupts hook ordering.
    let mut app = App::get();

    let mut commit = move |from: usize, to: usize| {
        let mut ids: Vec<String> = app.bots.peek().iter().map(|b| b.id.clone()).collect();
        if from >= ids.len() || to > ids.len() || from == to {
            return;
        }
        let moved = ids.remove(from);
        ids.insert(to.min(ids.len()), moved);

        // Reorder locally first so the row does not snap back while the
        // request is in flight.
        let reordered: Vec<BotSummary> = ids
            .iter()
            .filter_map(|id| app.bots.peek().iter().find(|b| &b.id == id).cloned())
            .collect();
        app.bots.set(reordered);

        spawn(async move {
            if let Ok(saved) = api::reorder_bots(ids).await {
                app.bots.set(saved);
            }
        });
    };

    rsx! {
        div {
            for (index, bot) in bots.iter().enumerate() {
                {
                    let is_active = active.as_deref() == Some(bot.id.as_str());
                    let is_dragging = dragging() == Some(index);
                    let is_over = over() == Some(index) && !is_dragging;
                    let class = format!(
                        "bot{}{}{}",
                        if is_active { " bot--active" } else { "" },
                        if is_dragging { " bot--dragging" } else { "" },
                        if is_over { " bot--over" } else { "" },
                    );
                    let dot = match status.get(index).copied().unwrap_or(Liveness::Unknown) {
                        Liveness::Online => "dot dot--on",
                        Liveness::Offline => "dot dot--off",
                        Liveness::Unknown => "dot dot--unknown",
                    };
                    let bot_for_select = bot.id.clone();
                    let bot_for_delete = bot.clone();

                    rsx! {
                        div {
                            key: "{bot.id}",
                            class: "{class}",
                            ondragover: move |e| {
                                e.prevent_default();
                                over.set(Some(index));
                            },
                            ondrop: move |e| {
                                e.prevent_default();
                                if let Some(from) = dragging() {
                                    commit(from, index);
                                }
                                dragging.set(None);
                                over.set(None);
                            },
                            button {
                                class: "bot__grip",
                                draggable: true,
                                title: "Drag to reorder",
                                ondragstart: move |_| dragging.set(Some(index)),
                                ondragend: move |_| {
                                    dragging.set(None);
                                    over.set(None);
                                },
                                "⠿"
                            }
                            button {
                                class: "bot__body",
                                onclick: move |_| on_select.call(bot_for_select.clone()),
                                div { class: "bot__name",
                                    span { class: "{dot}" }
                                    span { "{bot.name}" }
                                }
                                div { class: "bot__url", "{bot.url}" }
                            }
                            button {
                                class: "bot__del",
                                title: "Remove bot",
                                onclick: move |_| on_delete.call(bot_for_delete.clone()),
                                "🗑"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Add-bot form.
///
/// The server validates by really logging in before saving, so the errors here
/// distinguish a wrong password from an unreachable host — the same three
/// cases `LoginScreen._testAndSave` separated by catching TimeoutException and
/// SocketException.
#[component]
fn AddBotDialog(on_close: EventHandler<()>, on_added: EventHandler<BotSummary>) -> Element {
    let mut name = use_signal(String::new);
    let mut url = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<(ErrorKind, String)>);

    let can_submit = !name().trim().is_empty() && !url().trim().is_empty() && !busy();

    let submit = move |_| {
        if !can_submit {
            return;
        }
        busy.set(true);
        error.set(None);
        let request = AddBotRequest {
            name: name(),
            url: url(),
            username: username(),
            password: password(),
        };
        spawn(async move {
            match api::add_bot(request).await {
                Ok(bot) => on_added.call(bot),
                Err(e) => {
                    error.set(Some((e.kind, e.message)));
                    busy.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "modal__scrim",
            onclick: move |_| if !busy() { on_close.call(()) },
            div { class: "modal", onclick: move |e| e.stop_propagation(),
                h2 { class: "modal__title", "Add a bot" }

                div { class: "field",
                    label { class: "field__label", "Name" }
                    input {
                        class: "input",
                        placeholder: "Production",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div { class: "field",
                    label { class: "field__label", "URL" }
                    input {
                        class: "input",
                        placeholder: "192.168.1.10:8080",
                        value: "{url}",
                        oninput: move |e| url.set(e.value()),
                    }
                    // Normalization happens server-side; say so rather than
                    // making the user guess whether /api/v1 belongs here.
                    div { class: "muted", style: "font-size:12px;margin-top:4px",
                        "http:// and /api/v1 are added automatically"
                    }
                }
                div { class: "field",
                    label { class: "field__label", "Username" }
                    input {
                        class: "input",
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                    }
                }
                div { class: "field",
                    label { class: "field__label", "Password" }
                    input {
                        class: "input",
                        r#type: "password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                }

                if let Some((kind, message)) = error.read().clone() {
                    div {
                        class: if kind == ErrorKind::Auth { "error error--auth" } else { "error" },
                        match kind {
                            ErrorKind::Auth => "That username or password was rejected.",
                            ErrorKind::Offline => "Couldn't reach that address. Is the bot running and the API enabled?",
                            _ => "",
                        }
                        if !matches!(kind, ErrorKind::Auth | ErrorKind::Offline) {
                            "{message}"
                        }
                    }
                }

                div { class: "row", style: "margin-top:16px",
                    button {
                        class: "btn btn--ghost btn--row",
                        disabled: busy(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn--row",
                        disabled: !can_submit,
                        onclick: submit,
                        if busy() {
                            span { class: "spinner" }
                            "Checking…"
                        } else {
                            "Add bot"
                        }
                    }
                }
            }
        }
    }
}

/// Delete confirmation, warning about the cached data as the legacy dialog did.
#[component]
fn ConfirmDelete(
    bot: BotSummary,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<String>,
) -> Element {
    let id = bot.id.clone();
    rsx! {
        div { class: "modal__scrim", onclick: move |_| on_cancel.call(()),
            div { class: "modal", onclick: move |e| e.stop_propagation(),
                h2 { class: "modal__title", "Remove this bot?" }
                div { class: "modal__body",
                    strong { "{bot.name}" }
                    " will be removed, along with its saved credentials and all "
                    "locally cached trades and history. The bot itself is not "
                    "affected and keeps trading."
                }
                div { class: "row",
                    button {
                        class: "btn btn--ghost btn--row",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn--danger btn--row",
                        onclick: move |_| on_confirm.call(id.clone()),
                        "Remove"
                    }
                }
            }
        }
    }
}
