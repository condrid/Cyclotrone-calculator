use dioxus::prelude::*;

use crate::calculations::{format_time_input, is_valid_time};

#[derive(Clone, PartialEq)]
pub(crate) struct PickerOption {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) emphasized: bool,
}

#[component]
pub(crate) fn SelectPicker(
    value: String,
    options: Vec<PickerOption>,
    onselect: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let selected_label = options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "consumer-picker select-picker",
            input {
                class: "select-picker-input",
                value: "{selected_label}",
                readonly: true,
                onfocus: move |_| open.set(true),
                onclick: move |_| open.set(true),
                onblur: move |_| open.set(false),
            }
            if open() {
                div { class: "consumer-menu select-picker-menu",
                    for option in options {
                        button {
                            r#type: "button",
                            class: if option.emphasized {
                                "consumer-create"
                            } else if option.value == value {
                                "consumer-option selected"
                            } else {
                                "consumer-option"
                            },
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                onselect.call(option.value.clone());
                                open.set(false);
                            },
                            "{option.label}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(unused_braces)]
pub(crate) fn Field(label: String, value: String, oninput: EventHandler<String>) -> Element {
    rsx! {
        label { "{label}" }
        input { value: "{value}", oninput: move |event| oninput.call(event.value()) }
    }
}

#[component]
#[allow(unused_braces)]
pub(crate) fn UnitField(
    label: String,
    value: String,
    unit: String,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { "{label}" }
        div { class: "input-with-unit",
            input { value: "{value}", oninput: move |event| oninput.call(event.value()) }
            span { class: "field-unit", "{unit}" }
        }
    }
}

#[component]
pub(crate) fn ConsumerPicker(
    value: String,
    options: Vec<String>,
    oninput: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let query = value.to_lowercase();
    let matches = options
        .iter()
        .filter(|name| name.to_lowercase().contains(&query))
        .cloned()
        .collect::<Vec<_>>();
    let matches_empty = matches.is_empty();
    let can_create = !value.trim().is_empty()
        && !options
            .iter()
            .any(|name| name.eq_ignore_ascii_case(value.trim()));
    let create_value = value.clone();

    rsx! {
        div { class: "consumer-picker",
            input {
                class: if value.trim().is_empty() { "consumer-name-input" } else { "consumer-name-input filled" },
                value: "{value}",
                placeholder: "Выберите или введите центр",
                onfocus: move |_| open.set(true),
                onblur: move |_| open.set(false),
                oninput: move |event| {
                    open.set(true);
                    oninput.call(event.value());
                }
            }
            if open() {
                div { class: "consumer-menu",
                    for name in matches {
                        button {
                            r#type: "button",
                            class: "consumer-option",
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                oninput.call(name.clone());
                                open.set(false);
                            },
                            "{name}"
                        }
                    }
                    if can_create {
                        button {
                            r#type: "button",
                            class: "consumer-create",
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                oninput.call(create_value.clone());
                                open.set(false);
                            },
                            "Создать «{value}»"
                        }
                    }
                    if matches_empty && !can_create {
                        span { class: "consumer-empty", "Нет сохраненных потребителей" }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn TimeField(value: String, oninput: EventHandler<String>) -> Element {
    let valid = value.is_empty() || is_valid_time(&value);
    let mut open = use_signal(|| false);

    rsx! {
        div { class: "time-field",
            input {
                r#type: "text",
                value: "{value}",
                placeholder: "ЧЧ:ММ",
                class: if valid { "" } else { "invalid" },
                onfocus: move |_| open.set(true),
                onblur: move |_| open.set(false),
                oninput: move |event| {
                    open.set(true);
                    oninput.call(format_time_input(&event.value()));
                }
            }
            if open() {
                div { class: "time-menu",
                    for hour in 4..24 {
                        for minute in [0, 15, 30, 45] {
                            button {
                                r#type: "button",
                                class: "time-option",
                                onmousedown: move |event| event.prevent_default(),
                                onclick: move |_| {
                                    oninput.call(format!("{hour:02}:{minute:02}"));
                                    open.set(false);
                                },
                                "{hour:02}:{minute:02}"
                            }
                        }
                    }
                }
            }
            if !valid {
                small { "Формат ЧЧ:ММ, шаг 15 минут" }
            }
        }
    }
}
