use dioxus::prelude::*;
use palette::{Mix, Srgb};

use crate::calculations::*;
use crate::components::{
    ConsumerPicker, Field, ManualTimeField, PickerOption, SelectPicker, TimeField, UnitField,
};
use crate::database::{
    count_saved_calculations, delete_drug, delete_saved_calculation, initialize_database,
    load_centers, load_drug_profile, load_drugs, load_interface_color, load_interface_font_step,
    load_isotopes, load_saved_calculation, load_saved_calculation_page,
    load_saved_calculation_title, save_calculation, save_drug_profile, save_interface_color,
    save_interface_font_step, save_isotope, update_calculation, update_drug_profile,
    CalculationSettings, SavedCalculationSummary,
};
use crate::models::{Consumer, DrugListItem, DrugProfile, Isotope};

pub(crate) fn launch() {
    if let Err(error) = initialize_database() {
        eprintln!("Не удалось инициализировать базу данных: {error}");
    }
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("Калькулятор радиопрепаратов")
                    .with_decorations(true)
                    .with_resizable(true)
                    .with_maximized(true),
            ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    let mut interface_color = use_signal(|| {
        load_interface_color()
            .ok()
            .filter(|color| is_valid_hex_color(color))
            .unwrap_or_else(|| "#3974d8".into())
    });
    let mut interface_font_step = use_signal(|| load_interface_font_step().unwrap_or(0));
    let mut tabs = use_signal(|| {
        vec![CalculationTabInfo {
            id: 1,
            title: "Новый расчёт".into(),
            dirty: false,
            loaded: false,
        }]
    });
    let mut active_tab = use_signal(|| 1_u64);
    let mut next_tab_id = use_signal(|| 2_u64);
    let mut close_requested = use_signal(|| None::<u64>);
    let mut rename_requested = use_signal(|| None::<u64>);

    rsx! {
        for tab in tabs.read().iter().cloned() {
            div {
                key: "{tab.id}",
                style: if active_tab() == tab.id { "display:block" } else { "display:none" },
                CalculationTab {
                    interface_color: interface_color(),
                    interface_font_step: interface_font_step(),
                    tab_id: tab.id,
                    tabs: tabs.read().clone(),
                    active_tab: active_tab(),
                    close_requested: close_requested(),
                    rename_requested: rename_requested(),
                    on_add_tab: move |_| {
                        let id = next_tab_id();
                        next_tab_id.set(id + 1);
                        tabs.write().push(CalculationTabInfo {
                            id,
                            title: "Новый расчёт".into(),
                            dirty: false,
                            loaded: false,
                        });
                        active_tab.set(id);
                    },
                    on_activate_tab: move |id| active_tab.set(id),
                    on_rename_tab: move |(id, title): (u64, String)| {
                        if let Some(tab) = tabs.write().iter_mut().find(|tab| tab.id == id) {
                            tab.title = title;
                        }
                    },
                    on_request_close: move |id| {
                        let can_close_immediately = tabs
                            .read()
                            .iter()
                            .find(|tab| tab.id == id)
                            .is_some_and(|tab| tab.loaded && !tab.dirty);
                        if can_close_immediately {
                            let mut directory = tabs.write();
                            directory.retain(|tab| tab.id != id);
                            if directory.is_empty() {
                                let new_id = next_tab_id();
                                next_tab_id.set(new_id + 1);
                                directory.push(CalculationTabInfo {
                                    id: new_id,
                                    title: "Новый расчёт".into(),
                                    dirty: false,
                                    loaded: false,
                                });
                            }
                            active_tab.set(directory.last().map(|tab| tab.id).unwrap_or(1));
                        } else {
                            active_tab.set(id);
                            close_requested.set(Some(id));
                        }
                    },
                    on_tab_state: move |(id, dirty, loaded): (u64, bool, bool)| {
                        if let Some(tab) = tabs.write().iter_mut().find(|tab| tab.id == id) {
                            tab.dirty = dirty;
                            tab.loaded = loaded;
                        }
                    },
                    on_cancel_close: move |_| close_requested.set(None),
                    on_request_rename: move |id| {
                        active_tab.set(id);
                        rename_requested.set(Some(id));
                    },
                    on_cancel_rename: move |_| rename_requested.set(None),
                    on_close_tab: move |id| {
                        let next_active = {
                            let mut directory = tabs.write();
                            directory.retain(|tab| tab.id != id);
                            if directory.is_empty() {
                                let new_id = next_tab_id();
                                next_tab_id.set(new_id + 1);
                                directory.push(CalculationTabInfo {
                                    id: new_id,
                                    title: "Новый расчёт".into(),
                                    dirty: false,
                                    loaded: false,
                                });
                            }
                            directory.last().map(|tab| tab.id).unwrap_or(1)
                        };
                        active_tab.set(next_active);
                        close_requested.set(None);
                        rename_requested.set(None);
                    },
                    on_interface_color_change: move |color: String| interface_color.set(color),
                    on_interface_font_step_change: move |step: u8| interface_font_step.set(step),
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct CalculationTabInfo {
    id: u64,
    title: String,
    dirty: bool,
    loaded: bool,
}

#[derive(Clone)]
struct ActualFillGroupView {
    vials: Vec<ActualFillVialView>,
    name: String,
    requested_activity_gbq: String,
    requested_time: String,
    deviation: Option<ActualFillDeviation>,
}

#[derive(Clone)]
struct ActualFillVialView {
    consumer_index: usize,
    name: String,
    requested_activity_gbq: String,
    actual_fill_time: String,
    actual_fill_activity_mbq: String,
    deviation: Option<ActualFillDeviation>,
}

#[derive(Props, Clone, PartialEq)]
struct CalculationTabProps {
    interface_color: String,
    interface_font_step: u8,
    tab_id: u64,
    tabs: Vec<CalculationTabInfo>,
    active_tab: u64,
    close_requested: Option<u64>,
    rename_requested: Option<u64>,
    on_add_tab: EventHandler<()>,
    on_activate_tab: EventHandler<u64>,
    on_rename_tab: EventHandler<(u64, String)>,
    on_request_close: EventHandler<u64>,
    on_request_rename: EventHandler<u64>,
    on_tab_state: EventHandler<(u64, bool, bool)>,
    on_cancel_close: EventHandler<()>,
    on_cancel_rename: EventHandler<()>,
    on_close_tab: EventHandler<u64>,
    on_interface_color_change: EventHandler<String>,
    on_interface_font_step_change: EventHandler<u8>,
}

fn calculation_snapshot(
    drug_id: Option<i64>,
    drug_name: &str,
    consumers: &[Consumer],
    settings: &CalculationSettings,
) -> String {
    let mut normalized_consumers = consumers.to_vec();
    for consumer in &mut normalized_consumers {
        if consumer.is_mandatory {
            consumer.requested_time = settings.filling_start.clone();
        }
    }
    serde_json::to_string(&(drug_id, drug_name, normalized_consumers, settings)).unwrap_or_default()
}

fn clamp_current(value: &str, maximum: f64) -> String {
    parse_decimal(value)
        .map(|current| current.clamp(0.0, maximum).to_string().replace('.', ","))
        .unwrap_or_else(|| value.to_string())
}

fn slider_fill(value: &str, maximum: f64) -> String {
    let percentage = parse_decimal(value)
        .map(|current| (current / maximum * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    format!("--current-fill:{percentage}%")
}

fn vial_noun(count: usize) -> &'static str {
    let last_two = count % 100;
    let last = count % 10;
    if (11..=14).contains(&last_two) {
        "флаконов"
    } else {
        match last {
            1 => "флакон",
            2..=4 => "флакона",
            _ => "флаконов",
        }
    }
}

fn format_deviation_activity(value_gbq: f64) -> String {
    let absolute = value_gbq.abs();
    if absolute < 1.0 {
        format!("{} МБк", format_activity_value(absolute * 1000.0))
    } else {
        format!("{} ГБк", format_activity_value(absolute))
    }
}

fn format_deviation_percent(value: f64) -> String {
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "−"
    } else {
        ""
    };
    format!("{sign}{}%", format_activity_value(value.abs()))
}

#[component]
fn ActualDeviationResult(deviation: Option<ActualFillDeviation>) -> Element {
    rsx! {
        if let Some(deviation) = deviation {
            div {
                class: if deviation.deviation_at_request_gbq < 0.0 {
                    "actual-result-badge deficit"
                } else {
                    "actual-result-badge excess"
                },
                strong {
                    if deviation.deviation_at_request_gbq < 0.0 {
                        "Недостаток"
                    } else if deviation.deviation_at_request_gbq > 0.0 {
                        "Избыток"
                    } else {
                        "Соответствует"
                    }
                }
                span {
                    "{format_deviation_activity(deviation.deviation_at_request_gbq)} · {format_deviation_percent(deviation.deviation_percent)}"
                }
            }
            small { class: "actual-comparison",
                "К фасовке: план {format_activity_value(deviation.requested_at_filling_gbq)} · факт {format_activity_value(deviation.actual_at_filling_gbq)} ГБк"
            }
        } else {
            span { class: "actual-result-pending", "Ожидает данных" }
        }
    }
}

#[component]
fn CompactActualDeviationResult(deviation: Option<ActualFillDeviation>) -> Element {
    rsx! {
        if let Some(deviation) = deviation {
            div {
                class: if deviation.deviation_at_request_gbq < 0.0 {
                    "actual-result-badge deficit"
                } else {
                    "actual-result-badge excess"
                },
                strong {
                    if deviation.deviation_at_request_gbq < 0.0 {
                        "Недостаток"
                    } else if deviation.deviation_at_request_gbq > 0.0 {
                        "Избыток"
                    } else {
                        "Соответствует"
                    }
                }
                span {
                    "{format_activity_value(deviation.deviation_at_request_gbq.abs())} ГБк · {format_deviation_percent(deviation.deviation_percent)}"
                }
            }
        } else {
            span { class: "actual-result-pending", "Ожидает данные" }
        }
    }
}

#[component]
fn ActualAtRequestBadge(deviation: Option<ActualFillDeviation>) -> Element {
    rsx! {
        if let Some(deviation) = deviation {
            span {
                class: if deviation.deviation_at_request_gbq < 0.0 {
                    "actual-at-request-badge deficit"
                } else {
                    "actual-at-request-badge excess"
                },
                "{format_activity_value(deviation.actual_at_request_gbq)}"
            }
        } else {
            span { class: "actual-fill-pending-value", "—" }
        }
    }
}

fn evaluate_calculator_expression(expression: &str) -> Option<f64> {
    struct Parser {
        chars: Vec<char>,
        position: usize,
    }

    impl Parser {
        fn skip_spaces(&mut self) {
            while self
                .chars
                .get(self.position)
                .is_some_and(|value| value.is_whitespace())
            {
                self.position += 1;
            }
        }

        fn expression(&mut self) -> Option<f64> {
            let mut value = self.term()?;
            loop {
                self.skip_spaces();
                match self.chars.get(self.position).copied() {
                    Some('+') => {
                        self.position += 1;
                        value += self.term()?;
                    }
                    Some('-' | '−') => {
                        self.position += 1;
                        value -= self.term()?;
                    }
                    _ => return Some(value),
                }
            }
        }

        fn term(&mut self) -> Option<f64> {
            let mut value = self.number()?;
            loop {
                self.skip_spaces();
                match self.chars.get(self.position).copied() {
                    Some('*' | '×') => {
                        self.position += 1;
                        value *= self.number()?;
                    }
                    Some('/' | '÷') => {
                        self.position += 1;
                        let divisor = self.number()?;
                        if divisor == 0.0 {
                            return None;
                        }
                        value /= divisor;
                    }
                    _ => return value.is_finite().then_some(value),
                }
            }
        }

        fn number(&mut self) -> Option<f64> {
            self.skip_spaces();
            let start = self.position;
            if matches!(self.chars.get(self.position), Some('+' | '-' | '−')) {
                self.position += 1;
            }
            while matches!(self.chars.get(self.position), Some('0'..='9' | '.' | ',')) {
                self.position += 1;
            }
            if self.position == start {
                return None;
            }
            self.chars[start..self.position]
                .iter()
                .collect::<String>()
                .replace('−', "-")
                .replace(',', ".")
                .parse::<f64>()
                .ok()
        }
    }

    let mut parser = Parser {
        chars: expression.chars().collect(),
        position: 0,
    };
    let value = parser.expression()?;
    parser.skip_spaces();
    (parser.position == parser.chars.len() && value.is_finite()).then_some(value)
}

#[component]
fn ActivityCalculator(isotope_name: String, half_life_minutes: f64) -> Element {
    let mut decay_mode = use_signal(|| false);
    let mut display = use_signal(String::new);
    let mut activity = use_signal(String::new);
    let mut activity_in_mbq = use_signal(|| false);
    let mut source_time = use_signal(String::new);
    let mut target_time = use_signal(String::new);

    let decay_result = {
        let parsed_activity = parse_decimal(&activity());
        let parse_time = |value: &str| -> Option<i32> {
            let (hours, minutes) = value.split_once(':')?;
            let hours = hours.parse::<i32>().ok()?;
            let minutes = minutes.parse::<i32>().ok()?;
            (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
        };
        parsed_activity
            .zip(parse_time(&source_time()))
            .zip(parse_time(&target_time()))
            .and_then(|((value, source), target)| {
                if value < 0.0 || !half_life_minutes.is_finite() || half_life_minutes <= 0.0 {
                    return None;
                }
                let elapsed = (target - source + 12 * 60).rem_euclid(24 * 60) - 12 * 60;
                Some(value * 2_f64.powf(-(elapsed as f64) / half_life_minutes))
            })
    };
    let standard_result = evaluate_calculator_expression(&display());

    rsx! {
        aside { class: "activity-calculator",
            div { class: "calculator-mode-switch",
                button {
                    class: if !decay_mode() { "active" } else { "" },
                    onclick: move |_| decay_mode.set(false),
                    "Обычный"
                }
                button {
                    class: if decay_mode() { "active" } else { "" },
                    onclick: move |_| decay_mode.set(true),
                    "Активность"
                }
            }
            if decay_mode() {
                div { class: "decay-calculator",
                    div { class: "calculator-isotope",
                        strong { "{isotope_name}" }
                        span { "T½ {format_activity_value(half_life_minutes)} мин" }
                    }
                    label {
                        div { class: "calculator-field-heading",
                            span { "Исходная активность" }
                            div { class: "calculator-unit-switch",
                                button {
                                    class: if !activity_in_mbq() { "active" } else { "" },
                                    onclick: move |_| activity_in_mbq.set(false),
                                    "ГБк"
                                }
                                button {
                                    class: if activity_in_mbq() { "active" } else { "" },
                                    onclick: move |_| activity_in_mbq.set(true),
                                    "МБк"
                                }
                            }
                        }
                        div { class: "input-with-unit calculator-activity-field",
                            input {
                                value: "{activity}",
                                inputmode: "decimal",
                                oninput: move |event| activity.set(event.value())
                            }
                            span { class: "field-unit",
                                if activity_in_mbq() { "МБк" } else { "ГБк" }
                            }
                        }
                    }
                    div { class: "calculator-time-fields",
                        label {
                            span { "Исходное время" }
                            ManualTimeField {
                                value: source_time(),
                                oninput: move |value| source_time.set(value)
                            }
                        }
                        label {
                            span { "Время пересчёта" }
                            ManualTimeField {
                                value: target_time(),
                                oninput: move |value| target_time.set(value)
                            }
                        }
                    }
                    div { class: "decay-calculator-result",
                        span { "Результат" }
                        if let Some(result) = decay_result {
                            strong {
                                if activity_in_mbq() {
                                    "{format_activity_value(result)} МБк"
                                } else {
                                    "{format_activity_value(result)} ГБк"
                                }
                            }
                            small {
                                if activity_in_mbq() {
                                    "{format_activity_value(result / 1000.0)} ГБк"
                                } else {
                                    "{format_activity_value(result * 1000.0)} МБк"
                                }
                            }
                        } else {
                            strong { "—" }
                        }
                    }
                }
            } else {
                div { class: "standard-calculator",
                    input {
                        class: "calculator-display",
                        value: "{display}",
                        placeholder: "0",
                        inputmode: "decimal",
                        oninput: move |event| {
                            display.set(event.value());
                        }
                    }
                    div { class: "calculator-live-result",
                        span { "Результат" }
                        output {
                            if let Some(result) = standard_result {
                                "{format_activity_value(result)}"
                            } else {
                                "—"
                            }
                        }
                    }
                    div { class: "calculator-keypad",
                        for key in ["C", "⌫", "÷", "×", "7", "8", "9", "−", "4", "5", "6", "+", "1", "2", "3", "=", "0", ","] {
                            button {
                                class: if key == "0" {
                                    "calculator-zero"
                                } else if ["÷", "×", "−", "+", "="].contains(&key) {
                                    "operator"
                                } else {
                                    ""
                                },
                                onclick: {
                                    let key = key.to_string();
                                    move |_| {
                                        match key.as_str() {
                                            "C" => {
                                                display.set(String::new());
                                            }
                                            "⌫" => {
                                                let mut value = display();
                                                value.pop();
                                                display.set(value);
                                            }
                                            "÷" | "×" | "−" | "+" => {
                                                let mut value = display().trim_end().to_string();
                                                if !value.is_empty()
                                                    && !matches!(
                                                        value.chars().last(),
                                                        Some('+' | '-' | '−' | '*' | '×' | '/' | '÷')
                                                    )
                                                {
                                                    value.push(' ');
                                                    value.push_str(&key);
                                                    value.push(' ');
                                                    display.set(value);
                                                }
                                            }
                                            "=" => {
                                                if let Some(result) =
                                                    evaluate_calculator_expression(&display())
                                                {
                                                    display.set(format_activity_value(result));
                                                }
                                            }
                                            digit => {
                                                let mut value = display();
                                                let digit = if digit == "," { "," } else { digit };
                                                let current_number = value
                                                    .rsplit(['+', '-', '−', '*', '×', '/', '÷'])
                                                    .next()
                                                    .unwrap_or_default();
                                                if digit != ","
                                                    || (!current_number.contains(',')
                                                        && !current_number.contains('.'))
                                                {
                                                    value.push_str(digit);
                                                }
                                                display.set(value);
                                            }
                                        }
                                    }
                                },
                                "{key}"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sanitize_print_title(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            _ => character,
        })
        .collect::<String>()
}

fn color_to_hex(color: Srgb<f32>) -> String {
    let color: Srgb<u8> = color.into_format();
    format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue)
}

fn is_valid_hex_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn interface_theme(accent_hex: &str) -> (String, String, &'static str) {
    let component = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&accent_hex[range], 16).unwrap_or(0) as f32 / 255.0
    };
    let accent = if is_valid_hex_color(accent_hex) {
        Srgb::new(component(1..3), component(3..5), component(5..7))
    } else {
        Srgb::new(0.224, 0.455, 0.847)
    };
    let perceived_brightness = accent.red * 0.299 + accent.green * 0.587 + accent.blue * 0.114;
    (
        color_to_hex(accent.mix(Srgb::new(1.0, 1.0, 1.0), 0.88)),
        color_to_hex(accent.mix(Srgb::new(0.0, 0.0, 0.0), 0.25)),
        if perceived_brightness > 0.62 {
            "#172b4d"
        } else {
            "#ffffff"
        },
    )
}

#[component]
fn CalculationTab(props: CalculationTabProps) -> Element {
    let initial_isotopes = load_isotopes().unwrap_or_default();
    let default_isotope_id = initial_isotopes
        .iter()
        .find(|isotope| isotope.code == "f18")
        .map(|isotope| isotope.id);
    let initial_drugs = load_drugs().unwrap_or_default();
    let initial_drug = initial_drugs.first().cloned();
    let initial_drug_id = initial_drug.as_ref().map(|drug| drug.id);
    let initial_drug_name = initial_drug
        .as_ref()
        .map(|drug| drug.name.clone())
        .unwrap_or_default();
    let initial_profile = initial_drug_id
        .and_then(|id| load_drug_profile(id).ok().flatten())
        .unwrap_or_default();
    let initial_isotope_id = initial_profile.isotope_id.or(default_isotope_id);
    let mut drug_id = use_signal(|| initial_drug_id);
    let mut drug = use_signal(|| initial_drug_name);
    let mut drugs = use_signal(|| initial_drugs);
    let mut isotopes = use_signal(|| initial_isotopes);
    let mut selected_isotope_id = use_signal(|| initial_isotope_id);
    let mut show_isotope_settings = use_signal(|| false);
    let mut show_interface_settings = use_signal(|| false);
    let mut interface_color_input = use_signal(|| props.interface_color.clone());
    let mut interface_font_step_input = use_signal(|| props.interface_font_step);
    let mut show_settings = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut editing_drug_id = use_signal(|| None::<i64>);
    let mut new_drug_name = use_signal(String::new);
    let mut new_drug_yield = use_signal(|| initial_profile.radiochemical_yield);
    let mut maximum_vial_volume = use_signal(|| initial_profile.maximum_vial_volume);
    let mut semi_product_volume = use_signal(|| initial_profile.semi_product_volume);
    let mut synthesis_time = use_signal(|| initial_profile.synthesis_time);
    let mut activity_transfer_time = use_signal(|| initial_profile.activity_transfer_time);
    let mut consumers = use_signal(|| vec![Consumer::sampling(), Consumer::line_flush()]);
    let mut split_new_consumers = use_signal(|| false);
    let mut compact_actual_fill = use_signal(|| false);
    let mut show_activity_calculator = use_signal(|| false);
    let mut centers = use_signal(|| load_centers().unwrap_or_default());
    let mut target_count = use_signal(|| "2".to_string());
    let mut target_constant = use_signal(|| "8".to_string());
    let mut target_current_1 = use_signal(|| "65".to_string());
    let mut target_current_2 = use_signal(|| "65".to_string());
    let mut volumetric_activity = use_signal(|| "6".to_string());
    let mut filling_start = use_signal(|| "04:30".to_string());
    let mut notice = use_signal(String::new);
    let mut show_load_report = use_signal(|| false);
    let mut show_print_preview = use_signal(|| false);
    let mut report_page = use_signal(|| 0_usize);
    let mut saved_reports = use_signal(Vec::<SavedCalculationSummary>::new);
    let mut saved_report_count = use_signal(|| 0_usize);
    let mut source_report_id = use_signal(|| None::<i64>);
    let mut source_report_name = use_signal(|| None::<String>);
    let mut saved_snapshot = use_signal(|| None::<String>);
    let mut show_save_report = use_signal(|| false);
    let mut report_name_input = use_signal(String::new);
    let mut rename_report_input = use_signal(String::new);
    let mut report_to_delete = use_signal(|| None::<i64>);
    let selected_isotope = isotopes
        .read()
        .iter()
        .find(|isotope| Some(isotope.id) == selected_isotope_id())
        .cloned();
    let isotope_half_life_minutes = selected_isotope
        .as_ref()
        .and_then(|isotope| parse_decimal(&isotope.half_life_minutes))
        .unwrap_or(F18_HALF_LIFE_MINUTES);
    let selected_isotope_name = selected_isotope
        .as_ref()
        .map(|isotope| isotope.name.clone())
        .unwrap_or_else(|| "F-18".into());
    let cyclotron_enabled = selected_isotope
        .as_ref()
        .is_some_and(|isotope| matches!(isotope.code.as_str(), "f18" | "c11" | "n13"));
    let (interface_light_color, interface_dark_color, interface_contrast_color) =
        interface_theme(&props.interface_color);
    let interface_preview_color = if is_valid_hex_color(&interface_color_input.read()) {
        interface_color_input.read().clone()
    } else {
        "#3974d8".into()
    };
    let interface_theme_style = format!(
        "--interface-accent:{};--interface-light:{};--interface-dark:{};--interface-on-accent:{};--font-increase:{}pt",
        props.interface_color,
        interface_light_color,
        interface_dark_color,
        interface_contrast_color,
        props.interface_font_step * 2
    );
    let cyclotron_offset = cyclotron_unloading_minutes(&target_count.read())
        .unwrap_or(11)
        .to_string();
    let before_synthesis = time_before_synthesis(
        &filling_start.read(),
        &synthesis_time.read(),
        &activity_transfer_time.read(),
    )
    .unwrap_or_else(|| "—".into());
    let cyclotron_time =
        time_before(&before_synthesis, &cyclotron_offset).unwrap_or_else(|| "—".into());
    let saved_before_synthesis = before_synthesis.clone();
    let saved_cyclotron_offset = cyclotron_offset.clone();
    let saved_cyclotron_time = cyclotron_time.clone();
    let rename_before_synthesis = before_synthesis.clone();
    let rename_cyclotron_offset = cyclotron_offset.clone();
    let rename_cyclotron_time = cyclotron_time.clone();
    let close_before_synthesis = before_synthesis.clone();
    let close_cyclotron_offset = cyclotron_offset.clone();
    let close_cyclotron_time = cyclotron_time.clone();
    let tracked_before_synthesis = before_synthesis.clone();
    let tracked_cyclotron_offset = cyclotron_offset.clone();
    let tracked_cyclotron_time = cyclotron_time.clone();
    let tracked_isotope_name = selected_isotope_name.clone();
    let saved_isotope_name = selected_isotope_name.clone();
    let rename_isotope_name = selected_isotope_name.clone();
    let close_isotope_name = selected_isotope_name.clone();

    use_effect(move || {
        let current = consumers.read().clone();
        let next_group_id = current
            .iter()
            .filter_map(|consumer| consumer.vial_group_id)
            .max()
            .unwrap_or(0)
            + 1;
        for (consumer_index, consumer) in current.iter().enumerate() {
            if consumer.is_mandatory
                || !consumer.split_into_vials
                || consumer.split_applied
                || consumer.name.trim().is_empty()
            {
                continue;
            }
            let Some(activities) = split_requested_activity_into_vials(
                &consumer.activity,
                &consumer.requested_time,
                &filling_start.read(),
                isotope_half_life_minutes,
                &volumetric_activity.read(),
                &maximum_vial_volume.read(),
            ) else {
                continue;
            };
            let source_name = consumer.name.trim().to_string();
            let original_activity = format_activity(&consumer.activity)
                .unwrap_or_else(|| consumer.activity.trim().to_string());
            let generated = activities
                .into_iter()
                .enumerate()
                .map(|(index, activity)| Consumer {
                    name: format!("{source_name} {}", index + 1),
                    activity: format_activity_value(activity),
                    requested_time: consumer.requested_time.clone(),
                    is_mandatory: false,
                    split_into_vials: true,
                    split_applied: true,
                    vial_group_id: Some(next_group_id),
                    vial_group_source_name: Some(source_name.clone()),
                    vial_group_original_activity: Some(original_activity.clone()),
                    actual_fill_time: String::new(),
                    actual_fill_activity_mbq: String::new(),
                })
                .collect::<Vec<_>>();
            let mut updated = current.clone();
            updated.splice(consumer_index..=consumer_index, generated);
            consumers.set(updated);
            break;
        }
    });

    use_effect(move || {
        let settings = CalculationSettings::new(
            &target_count.read(),
            &target_constant.read(),
            &target_current_1.read(),
            &target_current_2.read(),
            selected_isotope_id(),
            &tracked_isotope_name,
            isotope_half_life_minutes,
            &volumetric_activity.read(),
            &filling_start.read(),
            &new_drug_yield.read(),
            &maximum_vial_volume.read(),
            &semi_product_volume.read(),
            &synthesis_time.read(),
            &activity_transfer_time.read(),
            &tracked_before_synthesis,
            &tracked_cyclotron_offset,
            &tracked_cyclotron_time,
        );
        let current = calculation_snapshot(drug_id(), &drug.read(), &consumers.read(), &settings);
        let loaded = source_report_id().is_some();
        let dirty = loaded
            && saved_snapshot
                .read()
                .as_ref()
                .is_some_and(|saved| saved != &current);
        props.on_tab_state.call((props.tab_id, dirty, loaded));
    });
    use_effect(move || {
        if props.rename_requested == Some(props.tab_id) {
            rename_report_input.set(source_report_name().unwrap_or_default());
        }
    });

    let rows = consumers
        .read()
        .iter()
        .enumerate()
        .map(|(consumer_index, c)| {
            let requested_time = if c.is_mandatory {
                filling_start.read().clone()
            } else {
                c.requested_time.clone()
            };
            let requested_activity = if c.is_mandatory {
                activity_from_sampling_volume(&c.activity, &volumetric_activity.read())
                    .map(|activity| activity.to_string())
            } else {
                Some(c.activity.clone())
            };
            let requested_activity = requested_activity.unwrap_or_default();
            let filling_result = activity_at_reference_time(
                &requested_activity,
                &requested_time,
                &filling_start.read(),
                isotope_half_life_minutes,
            );
            let compensated_filling_activity = filling_result
                .and_then(|(activity, _)| {
                    compensate_radiochemical_yield(activity, &new_drug_yield.read())
                })
                .map(|activity| activity.to_string())
                .unwrap_or_default();
            let before_synthesis_result = activity_at_reference_time(
                &compensated_filling_activity,
                &filling_start.read(),
                &before_synthesis,
                isotope_half_life_minutes,
            );
            let cyclotron_result = activity_at_reference_time(
                &compensated_filling_activity,
                &filling_start.read(),
                &cyclotron_time,
                isotope_half_life_minutes,
            );
            let filling_volume = filling_result.and_then(|(activity, _)| {
                calculate_filling_volume(activity, &volumetric_activity.read())
            });
            let volume_badge_class = filling_volume
                .zip(parse_decimal(&maximum_vial_volume.read()))
                .and_then(|(volume, maximum)| (maximum > 0.0).then_some((volume / maximum) * 100.0))
                .map(|percentage| {
                    if percentage >= 100.0 {
                        "volume-badge danger"
                    } else if percentage > 90.0 {
                        "volume-badge warning"
                    } else {
                        "volume-badge safe"
                    }
                })
                .unwrap_or("volume-badge neutral");
            (
                c.name.clone(),
                cyclotron_result
                    .map(|(activity, _)| format_adaptive_value(activity))
                    .unwrap_or_else(|| "—".into()),
                before_synthesis_result
                    .map(|(activity, _)| format_activity_value(activity))
                    .unwrap_or_else(|| "—".into()),
                filling_result
                    .map(|(activity, _)| format_activity_value(activity))
                    .unwrap_or_else(|| "—".into()),
                filling_volume
                    .map(format_volume_value)
                    .unwrap_or_else(|| "—".into()),
                requested_time,
                format_activity(&requested_activity).unwrap_or_else(|| "—".into()),
                cyclotron_result.is_some()
                    && before_synthesis_result.is_some()
                    && filling_result.is_some()
                    && !compensated_filling_activity.is_empty()
                    && filling_volume.is_some()
                    && !c.name.trim().is_empty(),
                c.activity.clone(),
                c.is_mandatory,
                volume_badge_class,
                cyclotron_result.map(|(activity, _)| activity),
                filling_volume,
                consumer_index,
                c.vial_group_id,
                c.vial_group_original_activity.clone(),
            )
        })
        .collect::<Vec<_>>();
    let activity_totals = rows.iter().fold([0.0; 3], |mut totals, row| {
        for (index, value) in [&row.1, &row.2, &row.3].into_iter().enumerate() {
            if let Some(activity) = parse_decimal(value) {
                totals[index] += activity;
            }
        }
        totals
    });
    let mut activity_totals = activity_totals.map(format_activity_value);
    let requested_series_volume = rows.iter().filter_map(|row| row.12).sum::<f64>();
    let series_adjustment =
        calculate_series_volume_adjustment(requested_series_volume, &semi_product_volume.read());
    let total_series_volume_display = series_adjustment
        .map(|(series_volume, _, _)| format_volume_value(series_volume))
        .unwrap_or_else(|| format_volume_value(requested_series_volume));
    let adjustment_display = series_adjustment
        .map(|(_, adjustment, _)| format_volume_value(adjustment))
        .unwrap_or_else(|| "—".into());
    let has_product_excess = series_adjustment.is_some_and(|(_, _, has_excess)| has_excess);
    let exact_eob_total = rows.iter().filter_map(|row| row.11).sum::<f64>();
    activity_totals[0] = format_adaptive_value(exact_eob_total);
    let actual_fill_groups = {
        let directory = consumers.read();
        let mut grouped_indices = Vec::<Vec<usize>>::new();

        for (consumer_index, consumer) in directory.iter().enumerate() {
            if consumer.is_mandatory {
                continue;
            }
            if let Some(group_id) = consumer.vial_group_id {
                if let Some(group) = grouped_indices
                    .iter_mut()
                    .find(|group| directory[group[0]].vial_group_id == Some(group_id))
                {
                    group.push(consumer_index);
                    continue;
                }
            }
            grouped_indices.push(vec![consumer_index]);
        }

        grouped_indices
            .into_iter()
            .map(|consumer_indices| {
                let first = &directory[consumer_indices[0]];
                let requested_activity_gbq = first
                    .vial_group_original_activity
                    .clone()
                    .unwrap_or_else(|| first.activity.clone());
                let requested_time = first.requested_time.clone();
                let measurements = consumer_indices
                    .iter()
                    .map(|index| {
                        let consumer = &directory[*index];
                        (
                            consumer.actual_fill_activity_mbq.as_str(),
                            consumer.actual_fill_time.as_str(),
                        )
                    })
                    .collect::<Vec<_>>();
                let deviation = actual_fill_deviation(
                    &requested_activity_gbq,
                    &requested_time,
                    &filling_start.read(),
                    isotope_half_life_minutes,
                    &measurements,
                );
                let vials = consumer_indices
                    .iter()
                    .map(|index| {
                        let consumer = &directory[*index];
                        ActualFillVialView {
                            consumer_index: *index,
                            name: consumer.name.clone(),
                            requested_activity_gbq: consumer.activity.clone(),
                            actual_fill_time: consumer.actual_fill_time.clone(),
                            actual_fill_activity_mbq: consumer.actual_fill_activity_mbq.clone(),
                            deviation: actual_fill_deviation(
                                &consumer.activity,
                                &consumer.requested_time,
                                &filling_start.read(),
                                isotope_half_life_minutes,
                                &[(
                                    consumer.actual_fill_activity_mbq.as_str(),
                                    consumer.actual_fill_time.as_str(),
                                )],
                            ),
                        }
                    })
                    .collect();

                ActualFillGroupView {
                    vials,
                    name: first
                        .vial_group_source_name
                        .clone()
                        .unwrap_or_else(|| first.name.clone()),
                    requested_activity_gbq,
                    requested_time,
                    deviation,
                }
            })
            .collect::<Vec<_>>()
    };
    let total_target_current = match target_count().as_str() {
        "1" => parse_decimal(&target_current_1.read()),
        "2" => parse_decimal(&target_current_1.read())
            .zip(parse_decimal(&target_current_2.read()))
            .map(|(first, second)| first + second),
        _ => None,
    };
    let irradiation_time = if cyclotron_enabled {
        parse_decimal(&target_constant.read())
            .zip(total_target_current)
            .map_or(
                Err(IrradiationTimeError::InvalidInput),
                |(constant, current)| {
                    irradiation_time_minutes(
                        exact_eob_total,
                        constant,
                        current,
                        isotope_half_life_minutes,
                    )
                },
            )
    } else {
        Err(IrradiationTimeError::InvalidInput)
    };
    let irradiation_start = match &irradiation_time {
        Ok(minutes) => {
            time_before(&cyclotron_time, &minutes.ceil().to_string()).unwrap_or_else(|| "—".into())
        }
        Err(_) => "—".into(),
    };
    let (irradiation_value, irradiation_class, irradiation_badge_class) = match irradiation_time {
        Ok(minutes) if is_extreme_value(minutes) => (
            format!("{} мин", format_adaptive_value(minutes.ceil())),
            "unreachable",
            "irradiation-value-badge danger",
        ),
        Ok(minutes) => (
            format!("{} мин", format_adaptive_value(minutes.ceil())),
            "valid",
            "irradiation-value-badge",
        ),
        Err(IrradiationTimeError::UnreachableActivity) => (
            "∞ мин".into(),
            "unreachable",
            "irradiation-value-badge danger",
        ),
        Err(IrradiationTimeError::InvalidInput) => {
            ("—".into(), "invalid", "irradiation-value-badge")
        }
    };
    let target_current_1_fill = slider_fill(
        &target_current_1.read(),
        if target_count() == "1" { 80.0 } else { 65.0 },
    );
    let target_current_2_fill = slider_fill(&target_current_2.read(), 65.0);
    let current_tab_title = props
        .tabs
        .iter()
        .find(|tab| tab.id == props.tab_id)
        .map(|tab| tab.title.clone())
        .unwrap_or_default();
    let displayed_report_title = source_report_id().map(|_| current_tab_title.clone());
    let preferred_print_title = source_report_name()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| {
            (current_tab_title != "Новый расчёт" && !current_tab_title.trim().is_empty())
                .then_some(current_tab_title.clone())
        })
        .map(|title| sanitize_print_title(&title))
        .unwrap_or_default();
    let print_drug_name = sanitize_print_title(&drug.read());
    rsx! {
        style { {STYLE} }
        style { {CONSUMER_STYLE} }
        style { {TIME_STYLE} }
        style { {REQUEST_STYLE} }
        style { {RESPONSIVE_STYLE} }
        style { {RESULTS_LAYOUT_STYLE} }
        style { {CONSUMER_REFINEMENT_STYLE} }
        style { {INTEGRATED_TABLE_STYLE} }
        style { {MODAL_STYLE} }
        style { {TAB_AND_REPORT_STYLE} }
        style { {VOLUME_LIMIT_STYLE} }
        style { {CYCLOTRON_CONTROL_STYLE} }
        style { {VIEWPORT_LAYOUT_STYLE} }
        style { {IRRADIATION_COMPACT_STYLE} }
        style { {ISOTOPE_STYLE} }
        style { {INTERFACE_THEME_STYLE} }
        style { {INTERFACE_SURFACE_STYLE} }
        style { {FONT_SCALE_STYLE} }
        style { {PRINT_STYLE} }
        style { {DROPDOWN_THEME_STYLE} }
        style { {PRINT_PORTRAIT_STYLE} }
        style { {CONSUMER_BADGE_STYLE} }
        style { {PRINT_TABLE_COMPACT_STYLE} }
        style { {PRINT_CONSUMER_TEXT_STYLE} }
        style { {PRINT_METADATA_COMPACT_STYLE} }
        style { {TAB_CONTRAST_STYLE} }
        style { {VIAL_GROUP_STYLE} }
        style { {VIAL_GROUP_REFINEMENT_STYLE} }
        style { {PRINT_ADJUSTMENT_STYLE} }
        style { {PRINT_THEME_STYLE} }
        style { {REPORT_TITLE_STYLE} }
        style { {ACTUAL_FILL_STYLE} }
        style { {VERTICAL_PAGE_SCROLL_STYLE} }
        style { {ACTUAL_FILL_COMPACT_STYLE} }
        style { {ACTUAL_FILL_VIEW_TOGGLE_STYLE} }
        style { {ACTIVITY_CALCULATOR_STYLE} }
        style { {ACTIVITY_CALCULATOR_FIX_STYLE} }
        style { {ACTIVITY_CALCULATOR_ANCHOR_STYLE} }
        style { {CONSUMER_EDITOR_VISUAL_FIX_STYLE} }
        style { {CONSUMER_BADGE_FOCUS_STYLE} }
        style { {UNIFIED_CLOSE_BUTTON_STYLE} }
        style { {CONSUMER_DROPDOWN_OVERFLOW_STYLE} }
        style { {CONSUMER_INTERACTION_FIX_STYLE} }
        style { {CROSS_BUTTON_FINAL_STYLE} }
        style { {CROSS_AND_FOCUS_CORRECTION_STYLE} }
        style { {STABLE_CONSUMER_TABLE_INTERACTION_STYLE} }
        style { {TABLE_TAB_AND_EMPTY_STATE_FIX_STYLE} }
        style { {TAB_TOOLS_AND_ACTUAL_WIDTH_STYLE} }
        style { {COMPACT_TAB_CONTROLS_STYLE} }
        main { class: "shell", style: "{interface_theme_style}",
            header { class: "topbar",
                div { class: "title-and-tabs",
                    div { class: "application-heading",
                        h1 { "Калькулятор радиоактивности" }
                        if let Some(report_title) = displayed_report_title.as_ref() {
                            p {
                                class: "active-report-title",
                                title: "{report_title}",
                                "{report_title}"
                            }
                        }
                    }
                    span { class: "version-badge", "v{env!(\"CARGO_PKG_VERSION\")}" }
                    div { class: "calculation-tabs",
                        for tab in props.tabs.iter().cloned() {
                            div {
                                class: if props.active_tab == tab.id { "calculation-tab active" } else { "calculation-tab" },
                                button {
                                    class: "tab-title",
                                    onclick: move |_| props.on_activate_tab.call(tab.id),
                                    if tab.dirty { "{tab.title} *" } else { "{tab.title}" }
                                }
                                if tab.loaded {
                                    button {
                                        class: "tab-edit",
                                        title: "Переименовать и пересохранить отчёт",
                                        onclick: move |_| props.on_request_rename.call(tab.id),
                                        "✏"
                                    }
                                }
                                button {
                                    class: "tab-close",
                                    title: "Закрыть вкладку",
                                    onclick: move |_| props.on_request_close.call(tab.id),
                                    "×"
                                }
                            }
                        }
                        button {
                            class: "add-tab",
                            title: "Новый расчёт",
                            onclick: move |_| props.on_add_tab.call(()),
                            "+"
                        }
                    }
                }
                div { class: "topbar-actions",
                    button {
                        class: "secondary",
                        onclick: move |_| show_print_preview.set(true),
                        "Печать"
                    }
                    button { class: "secondary", onclick: move |_| {
                        report_page.set(0);
                        match (count_saved_calculations(), load_saved_calculation_page(10, 0)) {
                            (Ok(count), Ok(reports)) => {
                                saved_report_count.set(count);
                                saved_reports.set(reports);
                            }
                            (Err(error), _) | (_, Err(error)) => {
                                saved_report_count.set(0);
                                saved_reports.set(Vec::new());
                                notice.set(format!("Ошибка чтения истории: {error}"));
                            }
                        }
                        show_load_report.set(true);
                    }, "Загрузить отчёт" }
                    button { class: "secondary", onclick: move |_| {
                        report_name_input.set(source_report_name().unwrap_or_default());
                        show_save_report.set(true);
                    }, "Сохранить отчёт" }
                }
            }
            div { class: "workspace",
                aside { class: "sidebar",
                    section { class: "panel",
                        h2 { "Настройки расчета" }
                        label { "Тип препарата" }
                        SelectPicker {
                            value: drug_id().map(|id| id.to_string()).unwrap_or_default(),
                            options: drugs.read().iter().map(|item| PickerOption {
                                value: item.id.to_string(),
                                label: item.name.clone(),
                                emphasized: false,
                            }).chain(std::iter::once(PickerOption {
                                value: "__new__".into(),
                                label: "Добавить новый".into(),
                                emphasized: true,
                            })).collect::<Vec<_>>(),
                            onselect: move |value: String| {
                                if value == "__new__" {
                                    editing_drug_id.set(None);
                                    new_drug_name.set(String::new());
                                    new_drug_yield.set("95".into());
                                    maximum_vial_volume.set(String::new());
                                    semi_product_volume.set("22".into());
                                    selected_isotope_id.set(default_isotope_id);
                                    synthesis_time.set("0".into());
                                    activity_transfer_time.set("0".into());
                                    show_settings.set(true);
                                } else {
                                    if let Ok(selected_id) = value.parse::<i64>() {
                                        if let Some(selected) = drugs.read().iter().find(|item| item.id == selected_id).cloned() {
                                            drug_id.set(Some(selected.id));
                                            drug.set(selected.name);
                                            if let Ok(Some(profile)) = load_drug_profile(selected_id) {
                                                new_drug_yield.set(profile.radiochemical_yield);
                                                maximum_vial_volume.set(profile.maximum_vial_volume);
                                                semi_product_volume.set(profile.semi_product_volume);
                                                selected_isotope_id.set(profile.isotope_id.or(default_isotope_id));
                                                synthesis_time.set(profile.synthesis_time);
                                                activity_transfer_time.set(profile.activity_transfer_time);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        button { class: "settings-button", onclick: move |_| {
                            if let Some(selected_id) = drug_id() {
                                editing_drug_id.set(Some(selected_id));
                                new_drug_name.set(drug.read().clone());
                                if let Ok(Some(profile)) = load_drug_profile(selected_id) {
                                    new_drug_yield.set(profile.radiochemical_yield);
                                    maximum_vial_volume.set(profile.maximum_vial_volume);
                                    semi_product_volume.set(profile.semi_product_volume);
                                    selected_isotope_id.set(profile.isotope_id.or(default_isotope_id));
                                    synthesis_time.set(profile.synthesis_time);
                                    activity_transfer_time.set(profile.activity_transfer_time);
                                }
                                show_settings.set(true);
                            }
                        }, "⚙  Настройки препарата" }
                    }
                    section {
                        class: if cyclotron_enabled { "panel cyclotron-panel" } else { "panel cyclotron-panel isotope-muted" },
                        h2 { "Циклотрон" }
                        if !cyclotron_enabled {
                            p { class: "cyclotron-disabled-note",
                                "Расчёт циклотрона доступен только для F-18, C-11 и N-13"
                            }
                        }
                        label { "Количество мишеней" }
                        div { class: "target-toggle",
                            button {
                                r#type: "button",
                                class: if target_count() == "1" { "target-choice active" } else { "target-choice" },
                                onclick: move |_| {
                                    if target_count() == "2"
                                        && parse_decimal(&target_current_1.read()) == Some(65.0)
                                    {
                                        target_current_1.set("80".into());
                                    }
                                    target_count.set("1".into());
                                },
                                "1"
                            }
                            button {
                                r#type: "button",
                                class: if target_count() == "2" { "target-choice active" } else { "target-choice" },
                                onclick: move |_| {
                                    let first = clamp_current(&target_current_1.read(), 65.0);
                                    let second = clamp_current(&target_current_2.read(), 65.0);
                                    target_current_1.set(first);
                                    target_current_2.set(second);
                                    target_count.set("2".into());
                                },
                                "2"
                            }
                        }
                        Field {
                            label: "Постоянная мишени",
                            value: target_constant,
                            oninput: move |value| target_constant.set(value)
                        }
                        label { "Ток" }
                        div { class: "target-current-control",
                            div { class: "target-current-heading", span { "Мишень 1" } strong { "{target_current_1} µA" } }
                            input {
                                class: "current-slider",
                                r#type: "range",
                                min: "0",
                                max: if target_count() == "1" { "80" } else { "65" },
                                step: "1",
                                value: "{target_current_1}",
                                style: "{target_current_1_fill}",
                                oninput: move |event| {
                                    let maximum = if target_count() == "1" { 80.0 } else { 65.0 };
                                    target_current_1.set(clamp_current(&event.value(), maximum));
                                }
                            }
                            div { class: "input-with-unit",
                                input {
                                    r#type: "number",
                                    min: "0",
                                    max: if target_count() == "1" { "80" } else { "65" },
                                    step: "1",
                                    value: "{target_current_1}",
                                    oninput: move |event| {
                                        let maximum = if target_count() == "1" { 80.0 } else { 65.0 };
                                        target_current_1.set(clamp_current(&event.value(), maximum));
                                    }
                                }
                                span { class: "field-unit", "µA" }
                            }
                        }
                        div {
                            class: if target_count() == "2" { "target-current-control" } else { "target-current-control muted" },
                                div { class: "target-current-heading", span { "Мишень 2" } strong { "{target_current_2} µA" } }
                                input {
                                    class: "current-slider",
                                    r#type: "range",
                                    min: "0",
                                    max: "65",
                                    step: "1",
                                    value: "{target_current_2}",
                                    style: "{target_current_2_fill}",
                                    disabled: target_count() != "2",
                                    oninput: move |event| target_current_2.set(clamp_current(&event.value(), 65.0))
                                }
                                div { class: "input-with-unit",
                                    input {
                                        r#type: "number",
                                        min: "0",
                                        max: "65",
                                        step: "1",
                                        value: "{target_current_2}",
                                        disabled: target_count() != "2",
                                        oninput: move |event| target_current_2.set(clamp_current(&event.value(), 65.0))
                                    }
                                    span { class: "field-unit", "µA" }
                                }
                        }
                        label { "Отгрузка с мишеней циклотрона" }
                        div { class: "input-with-unit",
                            input { value: "{cyclotron_offset}", readonly: true }
                            span { class: "field-unit", "мин" }
                        }
                        div { class: "irradiation-summary {irradiation_class}",
                            div { class: "irradiation-metric irradiation-start",
                                span { "Старт облучения" }
                                strong { "{irradiation_start}" }
                            }
                            div { class: "irradiation-metric",
                                span { "Время облучения" }
                                strong {
                                    class: "{irradiation_badge_class}",
                                    title: if irradiation_value.contains('∞') {
                                        "Время облучения стремится к бесконечности.\nТребуемая активность достигла\nили превысила активность насыщения."
                                    } else {
                                        ""
                                    },
                                    "{irradiation_value}"
                                }
                            }
                        }
                    }
                    section { class: "panel",
                        h2 { "Параметры фасовки" }
                        UnitField { label: "Объемная активность", value: volumetric_activity, unit: "ГБк/мл", oninput: move |v| volumetric_activity.set(v) }
                        label { "Максимальный объем флакона" }
                        div { class: "input-with-unit",
                            input { value: "{maximum_vial_volume}", readonly: true }
                            span { class: "field-unit", "мл" }
                        }
                        label { "Объем полупродукта" }
                        div { class: "input-with-unit",
                            input { value: "{semi_product_volume}", readonly: true }
                            span { class: "field-unit", "мл" }
                        }
                        label { "Начало фасовки" }
                        TimeField { value: filling_start.read().clone(), oninput: move |v| filling_start.set(v) }
                        p { class: "hint",
                            "Период полураспада {selected_isotope_name}: {format_activity_value(isotope_half_life_minutes)} мин"
                        }
                    }
                    section { class: "panel consumers-panel", style: "height:420px; min-height:420px; max-height:420px; overflow:hidden;",
                        div { class: "panel-heading", h2 { "Потребители ({consumers.read().len()})" }, button { class: "small", onclick: move |_| consumers.write().push(Consumer::new(split_new_consumers())), "+ Добавить" } }
                        div { class: "consumer-scroll", style: "height:300px; min-height:300px; max-height:300px; overflow-y:auto; overflow-x:hidden;",
                            table { class: "consumer-editor",
                                thead { tr { th { "Центр" } th { "Активность / объем" } th { "Время" } th { "" } } }
                                tbody { for (i, c) in consumers.read().iter().enumerate() {
                                    tr {
                                        td {
                                            if c.is_mandatory {
                                                strong { class: "mandatory-consumer", "{c.name}" }
                                            } else {
                                                ConsumerPicker { value: c.name.clone(), options: centers.read().clone(), oninput: move |v| consumers.write()[i].name = v }
                                            }
                                        }
                                        td { div { class: "input-with-unit",
                                            input {
                                            value: "{c.activity}",
                                            oninput: move |e| consumers.write()[i].activity = e.value(),
                                            onblur: move |_| {
                                                let value = {
                                                    let current = consumers.read();
                                                    format_activity(&current[i].activity)
                                                };
                                                if let Some(value) = value {
                                                    consumers.write()[i].activity = value;
                                                }
                                            }
                                            }
                                            span { class: "field-unit", if c.is_mandatory { "мл" } else { "ГБк" } }
                                        } }
                                        td {
                                            if !c.is_mandatory {
                                                TimeField { value: c.requested_time.clone(), oninput: move |v| consumers.write()[i].requested_time = v }
                                            }
                                        }
                                        td { if !c.is_mandatory { button { class: "remove", onclick: move |_| { consumers.write().remove(i); }, "×" } } }
                                    }
                                } }
                            }
                        }
                    }
                    div { class: "isotope-settings-launch",
                        small {
                            "Выбранный изотоп: {selected_isotope_name} · T½ {format_activity_value(isotope_half_life_minutes)} мин"
                        }
                        button {
                            class: "settings-button",
                            onclick: move |_| {
                                match load_isotopes() {
                                    Ok(items) => {
                                        isotopes.set(items);
                                        show_isotope_settings.set(true);
                                    }
                                    Err(error) => notice.set(format!("Ошибка загрузки изотопов: {error}")),
                                }
                            },
                            "⚙  Настройки изотопа и периода полураспада"
                        }
                        button {
                            class: "settings-button",
                            onclick: move |_| {
                                interface_color_input.set(props.interface_color.clone());
                                interface_font_step_input.set(props.interface_font_step);
                                show_interface_settings.set(true);
                            },
                            "◉  Настройки интерфейса"
                        }
                    }
                }
                section { class: "results panel",
                    div { class: "results-heading", div { h2 { "Потребители и результаты" } p { "Препарат: {drug}" } } span { class: "live", "● Пересчет автоматически" } }
                    div { class: "integrated-table-heading",
                        h2 { "Расчет по потребителям ({consumers.read().len()})" }
                    }
                    div { class: "results-table-scroll",
                    table { class: "results-table integrated-results-table",
                        thead { tr {
                            th { "Потребитель" }
                            th { div { "Циклотрон" } small { "{cyclotron_time} · ГБк" } }
                            th { div { "До синтеза" } small { "{before_synthesis} · ГБк" } }
                            th { div { "Время фасовки" } small { "{filling_start} · ГБк" } }
                            th { div { "Объем наполнения флакона" } small { "мл" } }
                            th { class: "request-group request-group-start",
                                div { class: "consumer-header-action",
                                    span { "Потребитель" }
                                    label { class: "split-vials-toggle",
                                        input {
                                            r#type: "checkbox",
                                            checked: split_new_consumers(),
                                            onchange: move |event| split_new_consumers.set(event.checked())
                                        }
                                        span { "Разбивать на 2+ флакона" }
                                    }
                                    button {
                                        class: "small",
                                        title: "Добавить потребителя",
                                        onclick: move |_| consumers.write().push(Consumer::new(split_new_consumers())),
                                        "+ Добавить"
                                    }
                                }
                            }
                            th { class: "request-group", div { "Объем/активность" } small { "мл, ГБк" } }
                            th { class: "request-group request-group-end", "Время по заявке" }
                        } }
                        tbody { for (row_position, row) in rows.iter().enumerate() { tr {
                            class: if row.14.is_some() {
                                if row_position == 0 || rows[row_position - 1].14 != row.14 {
                                    "vial-group-row vial-group-first"
                                } else if row_position + 1 == rows.len() || rows[row_position + 1].14 != row.14 {
                                    "vial-group-row vial-group-last"
                                } else {
                                    "vial-group-row"
                                }
                            } else {
                                ""
                            },
                            td { strong { "{row.0}" } }
                            td {
                                if row.11.is_some_and(is_extreme_value) {
                                    span {
                                        class: "extreme-activity-badge",
                                        title: "Очень большая активность облучения",
                                        "{row.1}"
                                    }
                                } else {
                                    span { "{row.1}" }
                                }
                            }
                            td { "{row.2}" }
                            td { "{row.3}" }
                            td {
                                span {
                                    class: "{row.10}",
                                    title: if row.10.contains("danger") { "превышение максимального объема флакона" } else { "" },
                                    "{row.4}"
                                }
                            }
                            td { class: "request-group request-group-start",
                                if row.9 {
                                    strong { class: "mandatory-consumer", "{row.0}" }
                                } else {
                                    div { class: "consumer-name-cell",
                                        div {
                                            if row.14.is_some()
                                                && (row_position == 0 || rows[row_position - 1].14 != row.14)
                                            {
                                                span { class: "vial-group-label",
                                                    "Одна заявка · {rows.iter().filter(|candidate| candidate.14 == row.14).count()} {vial_noun(rows.iter().filter(|candidate| candidate.14 == row.14).count())}"
                                                }
                                            }
                                            ConsumerPicker { value: row.0.clone(), options: centers.read().clone(), oninput: {
                                                let consumer_index = row.13;
                                                move |value| consumers.write()[consumer_index].name = value
                                            } }
                                        }
                                        button { class: "remove", onclick: {
                                            let consumer_index = row.13;
                                            move |_| { consumers.write().remove(consumer_index); }
                                        }, "×" }
                                    }
                                }
                            }
                            td { class: "request-group",
                                if row.14.is_some()
                                    && (row_position == 0 || rows[row_position - 1].14 != row.14)
                                {
                                    if let Some(original_activity) = row.15.as_ref() {
                                        span { class: "vial-original-activity",
                                            "Исходно: {original_activity} ГБк"
                                        }
                                    }
                                }
                                div { class: "input-with-unit",
                                    input {
                                        value: "{row.8}",
                                        oninput: {
                                            let consumer_index = row.13;
                                            move |event| consumers.write()[consumer_index].activity = event.value()
                                        },
                                        onblur: {
                                            let consumer_index = row.13;
                                            move |_| {
                                            let value = {
                                                let current = consumers.read();
                                                format_activity(&current[consumer_index].activity)
                                            };
                                            if let Some(value) = value {
                                                consumers.write()[consumer_index].activity = value;
                                            }
                                        }}
                                    }
                                    span { class: "field-unit", if row.9 { "мл" } else { "ГБк" } }
                                }
                            }
                            td { class: "request-group request-group-end",
                                if !row.9 {
                                    TimeField { value: row.5.clone(), oninput: {
                                        let consumer_index = row.13;
                                        move |value| consumers.write()[consumer_index].requested_time = value
                                    } }
                                }
                            }
                        } } }
                        tfoot { tr { class: "total-row",
                            td { strong { "Итого" } }
                            td {
                                if is_extreme_value(exact_eob_total) {
                                    strong {
                                        class: "extreme-activity-badge",
                                        title: "Очень большая итоговая активность облучения",
                                        "{activity_totals[0]}"
                                    }
                                } else {
                                    strong { "{activity_totals[0]}" }
                                }
                            }
                            td { strong { "{activity_totals[1]}" } }
                            td { strong { "{activity_totals[2]}" } }
                            td {
                                title: "Общий объем серии препарата",
                                strong { "{total_series_volume_display}" }
                            }
                            td {
                                class: "empty-series-summary",
                                colspan: "3",
                                rowspan: "2",
                            }
                        } }
                        tr { class: if has_product_excess { "dilution-row excess-row" } else { "dilution-row" },
                            td {
                                strong {
                                    if has_product_excess { "Излишки препарата" } else { "Разбавление" }
                                }
                            }
                            td { class: "dilution-explanation", colspan: "3" }
                            td { strong { "{adjustment_display}" } }
                        }
                    }
                    }
                    div { class: "actual-fill-section",
                        div { class: "actual-fill-heading",
                            div {
                                h2 { "Контроль фактического налива" }
                                p { "Отклонение рассчитывается по всей заявке на время заявки" }
                            }
                            div { class: "actual-fill-heading-actions",
                                button {
                                    class: "secondary small actual-compact-toggle",
                                    onclick: move |_| {
                                        let next_value = !compact_actual_fill();
                                        compact_actual_fill.set(next_value);
                                    },
                                    if compact_actual_fill() {
                                        "Показать полный вариант таблицы"
                                    } else {
                                        "Показать компактный вариант таблицы"
                                    }
                                }
                                span { class: "actual-filling-time",
                                    span { "Время фасовки" }
                                    strong { "{filling_start}" }
                                }
                                button {
                                    class: if show_activity_calculator() {
                                        "calculator-toggle active"
                                    } else {
                                        "calculator-toggle"
                                    },
                                    title: if show_activity_calculator() {
                                        "Скрыть калькулятор"
                                    } else {
                                        "Показать калькулятор"
                                    },
                                    "aria-label": if show_activity_calculator() {
                                        "Скрыть калькулятор"
                                    } else {
                                        "Показать калькулятор"
                                    },
                                    onclick: move |_| {
                                        show_activity_calculator.set(!show_activity_calculator())
                                    },
                                    span { class: "calculator-toggle-icon", "123" }
                                }
                            }
                        }
                        div {
                            class: if show_activity_calculator() {
                                "actual-fill-content with-calculator"
                            } else {
                                "actual-fill-content"
                            },
                            div { class: "actual-fill-table-pane",
                        if actual_fill_groups.is_empty() {
                            div { class: "actual-fill-empty",
                                "Добавьте реального потребителя, чтобы выполнить контроль налива."
                            }
                        } else if compact_actual_fill() {
                            table { class: "actual-fill-table actual-fill-compact-view",
                                thead { tr {
                                    th { "Флакон" }
                                    th { "Фактическая активность, МБк" }
                                    th { "Время, ЧЧ:ММ" }
                                    th { "Отклонение, ГБк / %" }
                                } }
                                tbody {
                                    for group in actual_fill_groups.iter() {
                                        for vial in group.vials.iter() {
                                            tr {
                                                td { class: "actual-vial-name",
                                                    strong { "{vial.name}" }
                                                }
                                                td {
                                                    div { class: "input-with-unit actual-activity-input",
                                                        input {
                                                            value: "{vial.actual_fill_activity_mbq}",
                                                            oninput: {
                                                                let consumer_index = vial.consumer_index;
                                                                move |event| {
                                                                    consumers.write()[consumer_index].actual_fill_activity_mbq =
                                                                        event.value()
                                                                }
                                                            },
                                                            onblur: {
                                                                let consumer_index = vial.consumer_index;
                                                                move |_| {
                                                                    let formatted = {
                                                                        let current = consumers.read();
                                                                        format_activity(
                                                                            &current[consumer_index].actual_fill_activity_mbq
                                                                        )
                                                                    };
                                                                    if let Some(value) = formatted {
                                                                        consumers.write()[consumer_index]
                                                                            .actual_fill_activity_mbq = value;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        span { class: "field-unit", "МБк" }
                                                    }
                                                }
                                                td {
                                                    ManualTimeField {
                                                        value: vial.actual_fill_time.clone(),
                                                        oninput: {
                                                            let consumer_index = vial.consumer_index;
                                                            move |value| consumers.write()[consumer_index].actual_fill_time = value
                                                        }
                                                    }
                                                }
                                                td { class: "actual-result-cell",
                                                    CompactActualDeviationResult { deviation: vial.deviation }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            table { class: "actual-fill-table",
                                thead { tr {
                                    th { "Потребитель / флакон" }
                                    th { div { "Время измерения" } small { "ЧЧ:ММ" } }
                                    th { div { "Фактическая активность" } small { "МБк" } }
                                    th { div { "Фактически ко времени заявки" } small { "ГБк" } }
                                    th { div { "Активность по заявке" } small { "ГБк" } }
                                    th { "Результат" }
                                } }
                                tbody {
                                    for group in actual_fill_groups.iter() {
                                        for (vial_position, vial) in group.vials.iter().enumerate() {
                                            tr {
                                                class: if group.vials.len() > 1 {
                                                    if vial_position == 0 {
                                                        "actual-vial-group actual-vial-first"
                                                    } else if vial_position + 1 == group.vials.len() {
                                                        "actual-vial-group actual-vial-last"
                                                    } else {
                                                        "actual-vial-group"
                                                    }
                                                } else {
                                                    ""
                                                },
                                                td { class: "actual-vial-name",
                                                    strong { "{vial.name}" }
                                                }
                                                td {
                                                    ManualTimeField {
                                                        value: vial.actual_fill_time.clone(),
                                                        oninput: {
                                                            let consumer_index = vial.consumer_index;
                                                            move |value| consumers.write()[consumer_index].actual_fill_time = value
                                                        }
                                                    }
                                                }
                                                td {
                                                    div { class: "input-with-unit actual-activity-input",
                                                        input {
                                                            value: "{vial.actual_fill_activity_mbq}",
                                                            oninput: {
                                                                let consumer_index = vial.consumer_index;
                                                                move |event| {
                                                                    consumers.write()[consumer_index].actual_fill_activity_mbq =
                                                                        event.value()
                                                                }
                                                            },
                                                            onblur: {
                                                                let consumer_index = vial.consumer_index;
                                                                move |_| {
                                                                    let formatted = {
                                                                        let current = consumers.read();
                                                                        format_activity(
                                                                            &current[consumer_index].actual_fill_activity_mbq
                                                                        )
                                                                    };
                                                                    if let Some(value) = formatted {
                                                                        consumers.write()[consumer_index]
                                                                            .actual_fill_activity_mbq = value;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        span { class: "field-unit", "МБк" }
                                                    }
                                                }
                                                td {
                                                    ActualAtRequestBadge { deviation: vial.deviation }
                                                }
                                                td { class: "actual-request-cell",
                                                    strong {
                                                        "{format_activity(&vial.requested_activity_gbq).unwrap_or_else(|| \"—\".into())}"
                                                    }
                                                    small { "на {group.requested_time}" }
                                                }
                                                td { class: "actual-result-cell",
                                                    ActualDeviationResult { deviation: vial.deviation }
                                                }
                                            }
                                        }
                                        if group.vials.len() > 1 {
                                            tr { class: "actual-group-total",
                                                td { colspan: "3",
                                                    strong { "Итого по заявке «{group.name}»" }
                                                    span {
                                                        "{group.vials.len()} {vial_noun(group.vials.len())}"
                                                    }
                                                }
                                                td {
                                                    ActualAtRequestBadge { deviation: group.deviation }
                                                }
                                                td { class: "actual-request-cell",
                                                    strong {
                                                        "{format_activity(&group.requested_activity_gbq).unwrap_or_else(|| \"—\".into())}"
                                                    }
                                                    small { "на {group.requested_time}" }
                                                }
                                                td { class: "actual-result-cell",
                                                    ActualDeviationResult { deviation: group.deviation }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                            }
                            if show_activity_calculator() {
                                ActivityCalculator {
                                    isotope_name: selected_isotope_name.clone(),
                                    half_life_minutes: isotope_half_life_minutes
                                }
                            }
                        }
                    }
                }
            }
        }
        if props.rename_requested == Some(props.tab_id) {
            div { class: "modal-backdrop",
                section { class: "drug-modal rename-report-modal",
                    div { class: "panel-heading",
                        h2 { "Переименование отчёта" }
                        button { class: "close", onclick: move |_| props.on_cancel_rename.call(()), "×" }
                    }
                    Field {
                        label: "Название отчёта",
                        value: rename_report_input,
                        oninput: move |value| rename_report_input.set(value)
                    }
                    p { class: "hint", "Пустое поле вернёт автоматическое название из препарата и времени сохранения." }
                    button { class: "save-drug", onclick: move |_| {
                        if let Some(report_id) = source_report_id() {
                            let mut saved_consumers = consumers.read().clone();
                            for consumer in &mut saved_consumers {
                                if consumer.is_mandatory {
                                    consumer.requested_time = filling_start.read().clone();
                                }
                            }
                            let settings = CalculationSettings::new(
                                &target_count.read(),
                                &target_constant.read(),
                                &target_current_1.read(),
                                &target_current_2.read(),
                                selected_isotope_id(),
                                &rename_isotope_name,
                                isotope_half_life_minutes,
                                &volumetric_activity.read(),
                                &filling_start.read(),
                                &new_drug_yield.read(),
                                &maximum_vial_volume.read(),
                                &semi_product_volume.read(),
                                &synthesis_time.read(),
                                &activity_transfer_time.read(),
                                &rename_before_synthesis,
                                &rename_cyclotron_offset,
                                &rename_cyclotron_time,
                            );
                            let entered_name = rename_report_input.read().trim().to_string();
                            let report_name = (!entered_name.is_empty()).then_some(entered_name.as_str());
                            let result = drug_id()
                                .ok_or(rusqlite::Error::InvalidQuery)
                                .and_then(|id| {
                                    update_calculation(
                                        report_id,
                                        id,
                                        &drug.read(),
                                        &saved_consumers,
                                        &settings,
                                        report_name,
                                    )
                                });
                            match result {
                                Ok(_) => {
                                    source_report_name.set(report_name.map(str::to_owned));
                                    saved_snapshot.set(Some(calculation_snapshot(
                                        drug_id(),
                                        &drug.read(),
                                        &saved_consumers,
                                        &settings,
                                    )));
                                    if let Ok(title) = load_saved_calculation_title(report_id) {
                                        props.on_rename_tab.call((props.tab_id, title));
                                    }
                                    notice.set("Данные и название отчёта сохранены".into());
                                    props.on_cancel_rename.call(());
                                }
                                Err(error) => notice.set(format!("Ошибка сохранения: {error}")),
                            }
                        }
                    }, "Сохранить данные и название" }
                }
            }
        }
        if show_save_report() {
            div { class: "modal-backdrop",
                section { class: "drug-modal save-report-modal",
                    div { class: "panel-heading",
                        h2 { "Сохранение отчёта" }
                        button { class: "close", onclick: move |_| show_save_report.set(false), "×" }
                    }
                    Field {
                        label: "Название отчёта",
                        value: report_name_input,
                        oninput: move |value| report_name_input.set(value)
                    }
                    p { class: "hint", "Если оставить поле пустым, названием будут дата, время и препарат." }
                    button { class: "save-drug", onclick: move |_| {
                        let mut saved_consumers = consumers.read().clone();
                        for consumer in &mut saved_consumers {
                            if consumer.is_mandatory {
                                consumer.requested_time = filling_start.read().clone();
                            }
                        }
                        let settings = CalculationSettings::new(
                            &target_count.read(),
                            &target_constant.read(),
                            &target_current_1.read(),
                            &target_current_2.read(),
                            selected_isotope_id(),
                            &saved_isotope_name,
                            isotope_half_life_minutes,
                            &volumetric_activity.read(),
                            &filling_start.read(),
                            &new_drug_yield.read(),
                            &maximum_vial_volume.read(),
                            &semi_product_volume.read(),
                            &synthesis_time.read(),
                            &activity_transfer_time.read(),
                            &saved_before_synthesis,
                            &saved_cyclotron_offset,
                            &saved_cyclotron_time,
                        );
                        let entered_name = report_name_input.read().trim().to_string();
                        let report_name = (!entered_name.is_empty()).then_some(entered_name.as_str());
                        let result = drug_id().ok_or(rusqlite::Error::InvalidQuery).and_then(|id| {
                            if let Some(report_id) = source_report_id() {
                                update_calculation(
                                    report_id,
                                    id,
                                    &drug.read(),
                                    &saved_consumers,
                                    &settings,
                                    report_name,
                                )
                                .map(|_| report_id)
                            } else {
                                save_calculation(
                                    id,
                                    &drug.read(),
                                    &saved_consumers,
                                    &settings,
                                    report_name,
                                )
                            }
                        });
                        match result {
                            Ok(report_id) => {
                                source_report_id.set(Some(report_id));
                                source_report_name.set(report_name.map(str::to_owned));
                                saved_snapshot.set(Some(calculation_snapshot(
                                    drug_id(),
                                    &drug.read(),
                                    &saved_consumers,
                                    &settings,
                                )));
                                if let Ok(title) = load_saved_calculation_title(report_id) {
                                    props.on_rename_tab.call((props.tab_id, title));
                                }
                                if let Ok(saved_centers) = load_centers() {
                                    centers.set(saved_centers);
                                }
                                notice.set("Расчёт сохранён".into());
                                show_save_report.set(false);
                            }
                            Err(error) => notice.set(format!("Ошибка: {error}")),
                        }
                    }, "Сохранить" }
                }
            }
        }
        if show_print_preview() {
            div { class: "modal-backdrop print-preview-overlay",
                section { class: "print-document",
                    div { class: "print-actions",
                        h2 { "Предпросмотр документа" }
                        div {
                            button {
                                class: "secondary",
                                onclick: move |_| show_print_preview.set(false),
                                "Закрыть"
                            }
                            button {
                                onclick: move |_| {
                                    let preferred_title = serde_json::to_string(&preferred_print_title)
                                        .unwrap_or_else(|_| "\"\"".into());
                                    let drug_title = serde_json::to_string(&print_drug_name)
                                        .unwrap_or_else(|_| "\"Расчёт\"".into());
                                    let script = format!(
                                        r#"
                                        (() => {{
                                            const oldTitle = document.title;
                                            const preferred = {preferred_title};
                                            const drug = {drug_title} || "Расчёт";
                                            const now = new Date();
                                            const today = [
                                                now.getFullYear(),
                                                String(now.getMonth() + 1).padStart(2, "0"),
                                                String(now.getDate()).padStart(2, "0")
                                            ].join("-");
                                            document.title = preferred || `${{drug}} ${{today}}`;
                                            window.addEventListener("afterprint", () => {{
                                                document.title = oldTitle;
                                            }}, {{ once: true }});
                                            window.print();
                                        }})();
                                        "#
                                    );
                                    document::eval(&script);
                                },
                                "Печать"
                            }
                        }
                    }
                    header { class: "print-header",
                        h1 { "Расчёт активности радиофармацевтического препарата" }
                        div { class: "print-metadata",
                            div {
                                span { "Препарат" }
                                strong { "{drug}" }
                            }
                            div {
                                span { "Изотоп" }
                                strong { "{selected_isotope_name}" }
                            }
                            div {
                                span { "Объёмная активность" }
                                strong {
                                    if let Some(activity) = parse_decimal(&volumetric_activity.read()) {
                                        "{format_activity_value(activity * 1000.0)} МБк/мл"
                                    } else {
                                        "— МБк/мл"
                                    }
                                }
                            }
                            div {
                                span { "Количество мишеней" }
                                strong { "{target_count}" }
                            }
                            div {
                                span { "Радиохимический выход" }
                                strong {
                                    if let Some(value) = parse_decimal(&new_drug_yield.read()) {
                                        "{format_activity_value(value)} %"
                                    } else {
                                        "— %"
                                    }
                                }
                            }
                            div {
                                span { "Объём полупродукта" }
                                strong {
                                    if let Some(value) = parse_decimal(&semi_product_volume.read()) {
                                        "{format_volume_value(value)} мл"
                                    } else {
                                        "— мл"
                                    }
                                }
                            }
                        }
                    }
                    table { class: "print-table",
                        colgroup {
                            col { style: "width:20%" }
                            col { style: "width:11%" }
                            col { style: "width:11%" }
                            col { style: "width:11%" }
                            col { style: "width:17%" }
                            col { style: "width:20%" }
                            col { style: "width:10%" }
                        }
                        thead {
                            tr {
                                th { rowspan: "2", "Потребитель" }
                                th { "Циклотрон" }
                                th { "До синтеза" }
                                th { "Фасовка" }
                                th { class: "print-volume-column", rowspan: "2", "Объём наполнения флакона, мл" }
                                th { rowspan: "2", "Заявка: объём / активность" }
                                th { rowspan: "2", "Время заявки" }
                            }
                            tr {
                                th { "{cyclotron_time}, ГБк" }
                                th { "{before_synthesis}, ГБк" }
                                th { "{filling_start}, ГБк" }
                            }
                        }
                        tbody {
                            for row in rows.iter() {
                                tr {
                                    td {
                                        strong { class: "print-consumer-name", "{row.0}" }
                                    }
                                    td { "{row.1}" }
                                    td { "{row.2}" }
                                    td { "{row.3}" }
                                    td { class: "print-volume-column", strong { "{row.4}" } }
                                    td {
                                        if row.9 {
                                            "{row.8} мл"
                                        } else {
                                            "{row.6} ГБк"
                                        }
                                    }
                                    td {
                                        if row.9 { "—" } else { "{row.5}" }
                                    }
                                }
                            }
                        }
                        tfoot {
                            tr {
                                td { strong { "Итого" } }
                                td { strong { "{activity_totals[0]}" } }
                                td { strong { "{activity_totals[1]}" } }
                                td { strong { "{activity_totals[2]}" } }
                                td { class: "print-volume-column", strong { "{total_series_volume_display}" } }
                                td { colspan: "2", "Объём серии" }
                            }
                        }
                    }
                    section {
                        class: if has_product_excess { "print-adjustment excess" } else { "print-adjustment dilution" },
                        span {
                            if has_product_excess { "Излишки препарата" } else { "Разбавление" }
                        }
                        strong { "{adjustment_display} мл" }
                    }
                }
            }
        }
        if show_load_report() {
            div { class: "modal-backdrop",
                section { class: "report-modal",
                    div { class: "panel-heading",
                        h2 { "Сохранённые расчёты" }
                        button { class: "close", onclick: move |_| show_load_report.set(false), "×" }
                    }
                    if saved_reports.read().is_empty() {
                        p { class: "empty-reports", "Сохранённых расчётов пока нет." }
                    } else {
                        div { class: "report-list",
                            for report in saved_reports.read().iter() {
                                div { class: "report-row",
                                button { class: "report-open", onclick: {
                                    let report_id = report.id;
                                    let tab_title = report.report_title.clone();
                                    move |_| match load_saved_calculation(report_id) {
                                        Ok(saved) => {
                                            let snapshot = calculation_snapshot(
                                                saved.drug_id,
                                                &saved.drug_name,
                                                &saved.consumers,
                                                &saved.settings,
                                            );
                                            source_report_id.set(Some(report_id));
                                            source_report_name.set(saved.report_name.clone());
                                            saved_snapshot.set(Some(snapshot));
                                            drug_id.set(saved.drug_id);
                                            drug.set(saved.drug_name);
                                            target_count.set(saved.settings.target_count);
                                            target_constant.set(saved.settings.target_constant);
                                            target_current_1.set(saved.settings.target_current_1_microamps);
                                            target_current_2.set(saved.settings.target_current_2_microamps);
                                            selected_isotope_id.set(saved.settings.isotope_id.or(default_isotope_id));
                                            volumetric_activity.set(saved.settings.volumetric_activity_gbq_per_ml);
                                            filling_start.set(saved.settings.filling_start);
                                            new_drug_yield.set(saved.settings.radiochemical_yield);
                                            maximum_vial_volume.set(saved.settings.maximum_vial_volume_ml);
                                            semi_product_volume.set(saved.settings.semi_product_volume_ml);
                                            synthesis_time.set(saved.settings.synthesis_time_minutes);
                                            activity_transfer_time.set(saved.settings.activity_transfer_time_minutes);
                                            let mut loaded_consumers = saved.consumers;
                                            if !loaded_consumers.iter().any(Consumer::is_sampling) {
                                                loaded_consumers.insert(0, Consumer::sampling());
                                            }
                                            if !loaded_consumers.iter().any(Consumer::is_line_flush) {
                                                let position = loaded_consumers
                                                    .iter()
                                                    .position(|consumer| !consumer.is_mandatory)
                                                    .unwrap_or(loaded_consumers.len());
                                                loaded_consumers.insert(position, Consumer::line_flush());
                                            }
                                            consumers.set(loaded_consumers);
                                            props.on_rename_tab.call((props.tab_id, tab_title.clone()));
                                            notice.set("Сохранённый расчёт загружен".into());
                                            show_load_report.set(false);
                                        }
                                        Err(error) => notice.set(format!("Ошибка загрузки: {error}")),
                                    }
                                },
                                    span { class: "report-time", "{report.calculated_at}" }
                                    div { class: "report-name",
                                        strong { "{report.report_title}" }
                                        small { "{report.drug_name}" }
                                    }
                                    span { class: "report-consumers", "Потребителей: {report.consumer_count}" }
                                }
                                button {
                                    class: "report-delete",
                                    title: "Удалить отчёт",
                                    onclick: {
                                        let report_id = report.id;
                                        move |_| report_to_delete.set(Some(report_id))
                                    },
                                    "×"
                                }
                                }
                            }
                        }
                        div { class: "pagination",
                            button {
                                class: "secondary small",
                                disabled: report_page() == 0,
                                onclick: move |_| {
                                    let page = report_page().saturating_sub(1);
                                    report_page.set(page);
                                    saved_reports.set(load_saved_calculation_page(10, page * 10).unwrap_or_default());
                                },
                                "←"
                            }
                            span {
                                "Страница {report_page() + 1} из {saved_report_count().div_ceil(10).max(1)}"
                            }
                            button {
                                class: "secondary small",
                                disabled: (report_page() + 1) * 10 >= saved_report_count(),
                                onclick: move |_| {
                                    let page = report_page() + 1;
                                    report_page.set(page);
                                    saved_reports.set(load_saved_calculation_page(10, page * 10).unwrap_or_default());
                                },
                                "→"
                            }
                        }
                    }
                }
            }
        }
        if let Some(report_id) = report_to_delete() {
            div { class: "modal-backdrop confirm-backdrop",
                section { class: "confirm-modal",
                    h2 { "Удалить сохранённый расчёт?" }
                    p { "Восстановить удалённый расчёт будет невозможно." }
                    div { class: "confirm-actions",
                        button { class: "danger", onclick: move |_| {
                            match delete_saved_calculation(report_id) {
                                Ok(_) => {
                                    saved_report_count.set(count_saved_calculations().unwrap_or(0));
                                    let max_page = saved_report_count().saturating_sub(1) / 10;
                                    let page = report_page().min(max_page);
                                    report_page.set(page);
                                    saved_reports.set(load_saved_calculation_page(10, page * 10).unwrap_or_default());
                                    report_to_delete.set(None);
                                    if source_report_id() == Some(report_id) {
                                        source_report_id.set(None);
                                        source_report_name.set(None);
                                        saved_snapshot.set(None);
                                    }
                                }
                                Err(error) => notice.set(format!("Ошибка удаления: {error}")),
                            }
                        }, "Да" }
                        button { class: "secondary", onclick: move |_| report_to_delete.set(None), "Нет" }
                    }
                }
            }
        }
        if props.close_requested == Some(props.tab_id) {
            div { class: "modal-backdrop confirm-backdrop",
                section { class: "confirm-modal close-tab-modal",
                    h2 { "Закрыть вкладку?" }
                    p {
                        if source_report_id().is_some() {
                            "Сохранить изменения с перезаписью загруженного отчёта?"
                        } else {
                            "Сохранить этот расчёт как новый отчёт перед закрытием?"
                        }
                    }
                    div { class: "close-tab-actions",
                        button { onclick: move |_| {
                            let mut saved_consumers = consumers.read().clone();
                            for consumer in &mut saved_consumers {
                                if consumer.is_mandatory {
                                    consumer.requested_time = filling_start.read().clone();
                                }
                            }
                            let settings = CalculationSettings::new(
                                &target_count.read(),
                                &target_constant.read(),
                                &target_current_1.read(),
                                &target_current_2.read(),
                                selected_isotope_id(),
                                &close_isotope_name,
                                isotope_half_life_minutes,
                                &volumetric_activity.read(),
                                &filling_start.read(),
                                &new_drug_yield.read(),
                                &maximum_vial_volume.read(),
                                &semi_product_volume.read(),
                                &synthesis_time.read(),
                                &activity_transfer_time.read(),
                                &close_before_synthesis,
                                &close_cyclotron_offset,
                                &close_cyclotron_time,
                            );
                            let result = drug_id().ok_or(rusqlite::Error::InvalidQuery).and_then(|id| {
                                if let Some(report_id) = source_report_id() {
                                    let report_name = source_report_name.read();
                                    update_calculation(
                                        report_id,
                                        id,
                                        &drug.read(),
                                        &saved_consumers,
                                        &settings,
                                        report_name.as_deref(),
                                    )
                                } else {
                                    save_calculation(
                                        id,
                                        &drug.read(),
                                        &saved_consumers,
                                        &settings,
                                        None,
                                    )
                                    .map(|_| ())
                                }
                            });
                            match result {
                                Ok(_) => props.on_close_tab.call(props.tab_id),
                                Err(error) => notice.set(format!("Ошибка сохранения: {error}")),
                            }
                        },
                            if source_report_id().is_some() { "Перезаписать и закрыть" } else { "Сохранить и закрыть" }
                        }
                        button {
                            class: "danger",
                            onclick: move |_| props.on_close_tab.call(props.tab_id),
                            "Закрыть без сохранения"
                        }
                        button {
                            class: "secondary",
                            onclick: move |_| props.on_cancel_close.call(()),
                            "Отмена"
                        }
                    }
                }
            }
        }
        if show_settings() { div { class: "modal-backdrop", section { class: "drug-modal",
            div { class: "panel-heading", h2 { "Настройки препарата" }, button { class: "close", onclick: move |_| show_settings.set(false), "×" } }
            Field { label: "Название", value: new_drug_name, oninput: move |v| new_drug_name.set(v) }
            label { "Изотоп" }
            SelectPicker {
                value: selected_isotope_id().map(|id| id.to_string()).unwrap_or_default(),
                options: isotopes.read().iter().map(|isotope| PickerOption {
                    value: isotope.id.to_string(),
                    label: isotope.name.clone(),
                    emphasized: false,
                }).collect::<Vec<_>>(),
                onselect: move |value: String| {
                    if let Ok(id) = value.parse::<i64>() {
                        selected_isotope_id.set(Some(id));
                    }
                }
            }
            UnitField { label: "Радиохимический выход", value: new_drug_yield, unit: "%", oninput: move |v| new_drug_yield.set(v) }
            UnitField { label: "Максимальный объем флакона", value: maximum_vial_volume, unit: "мл", oninput: move |v| maximum_vial_volume.set(v) }
            UnitField { label: "Объем полупродукта", value: semi_product_volume, unit: "мл", oninput: move |v| semi_product_volume.set(v) }
            UnitField { label: "Время синтеза", value: synthesis_time, unit: "мин", oninput: move |v| synthesis_time.set(v) }
            UnitField { label: "Время передачи активности", value: activity_transfer_time, unit: "мин", oninput: move |v| activity_transfer_time.set(v) }
            p { class: "hint", "Профиль препарата сохраняется как отдельная настройка." }
            button { class: "save-drug", onclick: move |_| {
                let name = new_drug_name.read().trim().to_string();
                if !name.is_empty() {
                    let original_id = editing_drug_id();
                    let profile = DrugProfile {
                        isotope_id: selected_isotope_id(),
                        radiochemical_yield: new_drug_yield.read().clone(),
                        maximum_vial_volume: maximum_vial_volume.read().clone(),
                        semi_product_volume: semi_product_volume.read().clone(),
                        synthesis_time: synthesis_time.read().clone(),
                        activity_transfer_time: activity_transfer_time.read().clone(),
                    };
                    let result = if let Some(id) = original_id {
                        update_drug_profile(id, &name, &profile).map(|_| id)
                    } else {
                        save_drug_profile(&name, &profile)
                    };
                    match result {
                        Ok(saved_id) => {
                            let mut directory = drugs.write();
                            if let Some(item) = directory.iter_mut().find(|item| item.id == saved_id) {
                                item.name = name.clone();
                            } else {
                                directory.push(DrugListItem { id: saved_id, name: name.clone() });
                            }
                            drop(directory);
                            drug_id.set(Some(saved_id));
                            drug.set(name.clone());
                            editing_drug_id.set(Some(saved_id));
                            notice.set("Настройки препарата сохранены".into());
                            show_settings.set(false);
                        }
                        Err(error) => notice.set(format!("Ошибка: {error}")),
                    }
                }
            }, "Сохранить препарат" }
            if editing_drug_id().is_some() {
                button {
                    class: "delete-drug",
                    onclick: move |_| show_delete_confirm.set(true),
                    "Удалить препарат"
                }
            }
        } } }
        if show_isotope_settings() {
            div { class: "modal-backdrop",
                section { class: "isotope-modal",
                    div { class: "panel-heading",
                        div {
                            h2 { "Изотопы и периоды полураспада" }
                            p { class: "hint", "Все периоды задаются в минутах." }
                        }
                        button {
                            class: "close",
                            onclick: move |_| show_isotope_settings.set(false),
                            "×"
                        }
                    }
                    div { class: "isotope-list",
                        div { class: "isotope-row isotope-header",
                            span { "Изотоп" }
                            span { "Период полураспада" }
                        }
                        for (index, isotope) in isotopes.read().clone().iter().enumerate() {
                            div { class: "isotope-row",
                                input {
                                    value: "{isotope.name}",
                                    oninput: move |event| isotopes.write()[index].name = event.value()
                                }
                                div { class: "input-with-unit",
                                    input {
                                        r#type: "number",
                                        min: "0.000001",
                                        step: "any",
                                        value: "{isotope.half_life_minutes}",
                                        oninput: move |event| {
                                            isotopes.write()[index].half_life_minutes = event.value()
                                        }
                                    }
                                    span { class: "field-unit", "мин" }
                                }
                            }
                        }
                    }
                    div { class: "isotope-modal-actions",
                        button {
                            class: "secondary",
                            onclick: move |_| {
                                isotopes.write().push(Isotope {
                                    id: 0,
                                    code: String::new(),
                                    name: "Новый изотоп".into(),
                                    half_life_minutes: String::new(),
                                });
                            },
                            "+ Добавить изотоп"
                        }
                        button {
                            onclick: move |_| {
                                let edited = isotopes.read().clone();
                                let mut error_message = None;
                                for isotope in &edited {
                                    if let Err(error) = save_isotope(isotope) {
                                        error_message = Some(error.to_string());
                                        break;
                                    }
                                }
                                if let Some(error) = error_message {
                                    notice.set(format!("Ошибка сохранения изотопов: {error}"));
                                } else {
                                    match load_isotopes() {
                                        Ok(items) => {
                                            isotopes.set(items);
                                            show_isotope_settings.set(false);
                                            notice.set("Настройки изотопов сохранены".into());
                                        }
                                        Err(error) => {
                                            notice.set(format!("Ошибка обновления изотопов: {error}"));
                                        }
                                    }
                                }
                            },
                            "Сохранить"
                        }
                    }
                }
            }
        }
        if show_interface_settings() {
            div { class: "modal-backdrop",
                section { class: "interface-modal",
                    div { class: "panel-heading",
                        div {
                            h2 { "Настройки интерфейса" }
                            p { class: "hint", "Выберите любой акцентный цвет." }
                        }
                        button {
                            class: "close",
                            onclick: move |_| show_interface_settings.set(false),
                            "×"
                        }
                    }
                    div { class: "color-picker-row",
                        input {
                            class: "color-picker",
                            r#type: "color",
                            value: "{interface_color_input}",
                            oninput: move |event| interface_color_input.set(event.value())
                        }
                        input {
                            class: "color-hex",
                            value: "{interface_color_input}",
                            maxlength: "7",
                            oninput: move |event| interface_color_input.set(event.value())
                        }
                    }
                    div {
                        class: "theme-preview",
                        style: "background:{interface_preview_color};",
                        span { "Предпросмотр цвета" }
                    }
                    div { class: "font-size-setting",
                        div { class: "font-size-heading",
                            label { "Размер шрифта" }
                            strong { "+{interface_font_step_input() * 2} пт" }
                        }
                        input {
                            class: "font-step-slider",
                            r#type: "range",
                            min: "0",
                            max: "4",
                            step: "1",
                            value: "{interface_font_step_input}",
                            oninput: move |event| {
                                if let Ok(step) = event.value().parse::<u8>() {
                                    interface_font_step_input.set(step.min(4));
                                }
                            }
                        }
                        div { class: "font-step-labels",
                            span { "+0" }
                            span { "+2" }
                            span { "+4" }
                            span { "+6" }
                            span { "+8" }
                        }
                    }
                    div { class: "interface-modal-actions",
                        button {
                            class: "secondary",
                            onclick: move |_| interface_color_input.set("#3974d8".into()),
                            "Вернуть бело-синий"
                        }
                        button {
                            onclick: move |_| {
                                let color = interface_color_input.read().clone();
                                let font_step = interface_font_step_input();
                                match save_interface_color(&color)
                                    .and_then(|_| save_interface_font_step(font_step))
                                {
                                    Ok(()) => {
                                        props.on_interface_color_change.call(color);
                                        props.on_interface_font_step_change.call(font_step);
                                        show_interface_settings.set(false);
                                        notice.set("Настройки интерфейса сохранены".into());
                                    }
                                    Err(error) => {
                                        notice.set(format!("Некорректный цвет: {error}"));
                                    }
                                }
                            },
                            "Применить"
                        }
                    }
                }
            }
        }
        if show_delete_confirm() {
            div { class: "modal-backdrop confirm-backdrop",
                section { class: "confirm-modal",
                    h2 { "Вы точно хотите удалить препарат?" }
                    p { "Название, настройки и связанные сохраненные расчеты будут удалены из базы данных." }
                    div { class: "confirm-actions",
                        button {
                            class: "danger",
                            onclick: move |_| {
                                let id = editing_drug_id();
                                let name = new_drug_name.read().clone();
                                match id.ok_or(rusqlite::Error::InvalidQuery).and_then(delete_drug) {
                                    Ok(_) => {
                                        let next_drug = {
                                            let mut directory = drugs.write();
                                            directory.retain(|item| Some(item.id) != id);
                                            directory.first().cloned()
                                        };
                                        if let Some(next_drug) = next_drug {
                                            let profile = load_drug_profile(next_drug.id)
                                                .ok()
                                                .flatten()
                                                .unwrap_or_default();
                                            drug_id.set(Some(next_drug.id));
                                            drug.set(next_drug.name.clone());
                                            editing_drug_id.set(Some(next_drug.id));
                                            new_drug_name.set(next_drug.name);
                                            new_drug_yield.set(profile.radiochemical_yield);
                                            maximum_vial_volume.set(profile.maximum_vial_volume);
                                            semi_product_volume.set(profile.semi_product_volume);
                                            selected_isotope_id.set(profile.isotope_id.or(default_isotope_id));
                                            synthesis_time.set(profile.synthesis_time);
                                            activity_transfer_time.set(profile.activity_transfer_time);
                                        } else {
                                            drug_id.set(None);
                                            drug.set(String::new());
                                            editing_drug_id.set(None);
                                            new_drug_name.set(String::new());
                                            new_drug_yield.set("95".into());
                                            maximum_vial_volume.set(String::new());
                                            semi_product_volume.set("22".into());
                                            selected_isotope_id.set(default_isotope_id);
                                            synthesis_time.set("0".into());
                                            activity_transfer_time.set("0".into());
                                        }
                                        notice.set(format!("Препарат «{name}» удален"));
                                        show_delete_confirm.set(false);
                                        show_settings.set(false);
                                    }
                                    Err(error) => {
                                        notice.set(format!("Ошибка удаления: {error}"));
                                        show_delete_confirm.set(false);
                                    }
                                }
                            },
                            "Да"
                        }
                        button {
                            class: "secondary",
                            onclick: move |_| show_delete_confirm.set(false),
                            "Нет"
                        }
                    }
                }
            }
        }
        if !notice.read().is_empty() {
            div {
                key: "{notice()}",
                class: "info-toast",
                style: "{interface_theme_style}",
                onanimationend: move |_| notice.set(String::new()),
                span { "{notice}" }
                button {
                    class: "toast-close",
                    title: "Закрыть",
                    onclick: move |_| notice.set(String::new()),
                    "×"
                }
            }
        }
    }
}

const STYLE: &str = r#"*{box-sizing:border-box}body{margin:0;font-family:Segoe UI,sans-serif;font-size:16px;background:#f4f7fb;color:#1d2939}.shell{min-height:100vh}.topbar{height:72px;display:flex;align-items:center;justify-content:space-between;padding:0 18px;background:#f8f8f8;color:#202020;border-bottom:1px solid #dedede;user-select:none;position:relative}.drag-handle{position:absolute;inset:0 190px 0 0;z-index:0}.topbar>div:not(.topbar-actions){position:relative;z-index:1;pointer-events:none}.topbar-actions{display:flex;align-items:center;gap:8px;position:relative;z-index:2}.window-button{width:38px;height:32px;padding:0;border-radius:5px;background:transparent;color:#444;font-size:18px;font-weight:400}.window-button:hover{background:#e5e5e5}.close-window:hover{background:#c42b1c;color:#fff}h1{font-size:20px;margin:0}h2{font-size:19px;margin:0}.topbar p{margin:3px 0 0;color:#666;font-size:13px}.workspace{display:flex;gap:24px;padding:24px;align-items:flex-start}.sidebar{width:440px;flex:0 0 440px}.panel{background:#fff;border:1px solid #dce3ec;border-radius:12px;padding:20px;margin-bottom:18px;box-shadow:0 2px 8px #263b5b0d}.results{flex:1;min-width:700px}.results-heading,.panel-heading,.consumer-toolbar{display:flex;align-items:center;justify-content:space-between;gap:12px}.results-heading{margin-bottom:20px}.results-heading p,.hint{color:#68778f;font-size:14px;margin-top:6px}label{display:block;margin:14px 0 7px;color:#475467;font-size:14px;font-weight:600}input,select{width:100%;border:1px solid #cbd5e1;border-radius:7px;padding:11px 12px;background:#fff;font:inherit;color:#172b4d}input:focus,select:focus{outline:2px solid #8fb2ff;border-color:#4777d9}.consumer-editor,.results-table{width:100%;border-collapse:collapse;font-size:15px}.consumer-editor th,.results-table th{text-align:left;background:#f3f6fa;color:#52627a;font-size:12px;text-transform:uppercase}.consumer-editor th,.consumer-editor td,.results-table th,.results-table td{padding:12px;border-bottom:1px solid #e5eaf0}.consumer-editor td{vertical-align:top}.consumer-editor input,.consumer-editor select{min-width:120px}.consumer-toolbar{margin:18px 0 10px}.result-title{margin:28px 0 12px}.time-field small{display:block;color:#b42318;font-size:11px;margin-top:4px}.invalid{border-color:#dc2626!important;background:#fff5f5}.status{display:inline-block;padding:5px 9px;border-radius:20px;font-size:12px;font-weight:700}.status.ok{color:#16704a;background:#dcfce7}.status.warn{color:#9a5b00;background:#fff0c2}.tag,.live{color:#3974d8;background:#edf4ff;border-radius:20px;padding:5px 9px;font-size:11px;font-weight:700}.live{color:#16704a;background:#dcfce7}button{border:0;border-radius:7px;padding:11px 16px;background:#3974d8;color:#fff;font:inherit;font-weight:600;cursor:pointer}button:hover{background:#2f61b8}button:disabled{opacity:.45;cursor:not-allowed}.secondary{background:#e8f0ff;color:#244d91}.small{padding:8px 11px;font-size:13px}.remove,.close{background:transparent;color:#a33a3a;padding:2px 7px;font-size:20px}.settings-button{width:100%;margin-top:12px;background:#eef4ff;color:#244d91;text-align:left}.saved{color:#16704a;background:#dcfce7;padding:12px;border-radius:7px}.mock-note{margin-top:18px;padding:13px;background:#fff9e8;color:#8b6500;border-radius:7px;font-size:13px}.modal-backdrop{position:fixed;inset:0;background:#10213b66;display:flex;align-items:center;justify-content:center;z-index:5}.drug-modal{width:460px;background:#fff;border-radius:14px;padding:24px;box-shadow:0 20px 60px #10213b55}.save-drug{margin-top:18px;width:100%}.consumer-scroll{max-height:420px;overflow-y:auto}.consumers-panel .consumer-editor{font-size:12px}.consumers-panel .consumer-editor th{font-size:10px}.consumers-panel .consumer-editor th,.consumers-panel .consumer-editor td{padding:6px 4px}.consumers-panel .consumer-editor input,.consumers-panel .consumer-editor select{min-width:0;padding:6px 5px;font-size:12px}.results>.consumer-toolbar,.results>.consumer-editor{display:none}"#;
const TIME_STYLE: &str = ".time-field{position:relative}.time-menu{position:absolute;left:0;right:0;top:100%;z-index:20;max-height:220px;overflow-y:auto;background:#fff;border:1px solid #b8c4d4;border-radius:7px;box-shadow:0 8px 20px #25385833;padding:4px}.time-option{display:block;width:100%;padding:7px 9px;text-align:left;background:#fff;color:#172b4d;border-radius:4px;font-size:13px;font-weight:400}.time-option:hover{background:#e8f0ff;color:#244d91}.consumer-picker{position:relative}.consumer-menu{position:absolute;left:0;right:0;top:100%;z-index:25;max-height:220px;overflow-y:auto;background:#fff;border:1px solid #b8c4d4;border-radius:7px;box-shadow:0 8px 20px #25385833;padding:4px}.consumer-option,.consumer-create{display:block;width:100%;padding:7px 9px;text-align:left;background:#fff;color:#172b4d;border-radius:4px;font-size:13px;font-weight:400}.consumer-option:hover,.consumer-create:hover{background:#e8f0ff;color:#244d91}.consumer-create{color:#3974d8;font-weight:600}.consumer-empty{display:block;padding:8px;color:#68778f;font-size:12px}";
const CONSUMER_STYLE: &str = ".sidebar{display:flex;flex-direction:column}.sidebar>.consumers-panel{order:2;position:relative;z-index:10;height:420px;overflow:hidden}.sidebar>section.panel:has(.tag){order:3}.consumer-scroll{height:300px;min-height:300px;max-height:300px;overflow-y:auto;overflow-x:hidden}.consumer-editor tr:has(.consumer-picker:focus-within),.consumer-editor tr:has(.time-field:focus-within){position:relative;z-index:1000}.consumer-picker,.time-field{position:relative;z-index:30}.consumer-picker:focus-within,.time-field:focus-within{z-index:10000}.consumer-menu,.time-menu{z-index:100000;max-height:180px;overflow-y:auto}.consumers-panel tbody tr:nth-child(n+4):nth-last-child(-n+3) .consumer-menu,.consumers-panel tbody tr:nth-child(n+4):nth-last-child(-n+3) .time-menu{top:auto;bottom:100%}.consumer-picker input{min-width:0;width:100%;padding:6px 5px;font-size:12px}";
const REQUEST_STYLE: &str = ".results-table th,.results-table td{text-align:center}.results-table th small{display:block;margin-top:4px;font-size:14px;line-height:1.2;text-transform:none}.results-table .request-group{background:#f8fbff;border-top:2px solid #8fb2d9;border-bottom:2px solid #8fb2d9}.results-table .request-group-start{border-left:2px solid #8fb2d9}.results-table .request-group-end{border-right:2px solid #8fb2d9}.results-table th.request-group{background:#eaf2fb;color:#315b86}.results-table .total-row td{background:#eef4f9;border-top:2px solid #637b96;color:#20364d;font-weight:700}.results-table .total-row .request-group{background:#f8fbff}.mandatory-consumer{display:block;padding:7px 5px;color:#244d91;background:#eef4ff;border-radius:5px;white-space:nowrap}.input-with-unit{position:relative}.input-with-unit input{padding-right:64px}.input-with-unit .field-unit{position:absolute;right:9px;top:50%;transform:translateY(-50%);color:#3974d8;font-size:11px;font-weight:700;pointer-events:none;white-space:nowrap}.target-toggle{display:grid;grid-template-columns:1fr 1fr;gap:6px;padding:4px;background:#edf2f7;border-radius:9px}.target-choice{padding:9px;background:transparent;color:#475467}.target-choice:hover{background:#dce7f4}.target-choice.active{background:#3974d8;color:#fff;box-shadow:0 1px 4px #274b7433}.cyclotron-panel input[readonly]{background:#f4f7fb;color:#315b86;font-weight:700}";
const RESPONSIVE_STYLE: &str = ".sidebar>.consumers-panel{display:none}.results>.consumer-toolbar{display:flex;margin-top:4px}.results>.consumer-editor{display:table;margin-bottom:22px;min-width:700px}.results{overflow-x:auto}.results-table{min-width:900px}.results tbody tr:nth-child(n+4):nth-last-child(-n+3) .consumer-menu,.results tbody tr:nth-child(n+4):nth-last-child(-n+3) .time-menu{top:auto;bottom:100%}@media(max-width:1200px){.workspace{flex-direction:column;align-items:stretch}.sidebar{width:100%;flex-basis:auto}.results{width:100%;min-width:0}.sidebar>.panel{width:100%}}@media(max-width:700px){.workspace{padding:12px;gap:12px}.topbar{height:auto;min-height:72px;padding:12px;gap:12px;align-items:flex-start}.topbar-actions{margin-left:auto}.topbar h1{font-size:18px}.panel{padding:14px;border-radius:9px}.consumer-scroll{width:100%}}";
const RESULTS_LAYOUT_STYLE: &str = ".results{display:grid;grid-template-columns:minmax(760px,1fr) 400px;grid-template-rows:auto auto auto auto;column-gap:20px;align-items:start}.results>.results-heading{grid-column:1/-1;grid-row:1}.results>.result-title{grid-column:1;grid-row:2}.results>.results-table{grid-column:1;grid-row:3}.results>.mock-note{grid-column:1;grid-row:4}.results>.consumer-toolbar{grid-column:2;grid-row:2;margin:24px 0 12px}.results>.consumer-editor{grid-column:2;grid-row:3/5;width:100%;min-width:380px;margin:0;align-self:start}.results>.results-table,.results>.consumer-editor{table-layout:fixed}.results>.results-table thead tr,.results>.consumer-editor thead tr{height:68px}.results>.results-table tbody tr,.results>.consumer-editor tbody tr{height:58px}.results>.results-table tbody td,.results>.consumer-editor tbody td{height:58px;vertical-align:middle}.results>.results-table tbody tr:nth-child(even) td,.results>.consumer-editor tbody tr:nth-child(even) td{background-color:#f8fafc}.results>.results-table tbody td:first-child,.results>.consumer-editor tbody td:first-child{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.results>.consumer-editor th,.results>.consumer-editor td{padding:8px 6px}.results>.consumer-editor th:nth-child(1){width:38%}.results>.consumer-editor th:nth-child(2){width:27%}.results>.consumer-editor th:nth-child(3){width:27%}.results>.consumer-editor th:nth-child(4){width:8%}.results>.consumer-editor input{min-width:0;height:38px}.results>.consumer-editor .mandatory-consumer{height:38px;line-height:24px}@media(max-width:1500px){.workspace{flex-direction:column;align-items:stretch}.sidebar{width:100%;flex-basis:auto}.results{width:100%;min-width:0}}@media(max-width:1050px){.results{display:block}.results>.consumer-toolbar{margin-top:18px}.results>.consumer-editor{min-width:700px;margin-bottom:22px}}";
const CONSUMER_REFINEMENT_STYLE: &str = ".results{grid-template-columns:minmax(720px,1fr) 520px}.results>.consumer-editor{min-width:500px}.results>.consumer-editor th:nth-child(1){width:50%}.results>.consumer-editor th:nth-child(2){width:20%}.results>.consumer-editor th:nth-child(3){width:22%}.results>.consumer-editor th:nth-child(4){width:8%}.results>.consumer-editor td:has(.consumer-picker:focus-within){position:relative;z-index:1000000;overflow:visible}.results>.consumer-editor tr:has(.consumer-picker:focus-within){position:relative;z-index:1000000}.results>.consumer-editor .consumer-picker:focus-within,.results>.consumer-editor .consumer-menu{z-index:1000001}.results>.consumer-editor .consumer-menu{min-width:260px}.results>.results-table th:nth-child(6){width:76px}.results>.results-table th:nth-child(7),.results>.results-table th:nth-child(8){width:92px}@media(max-width:1050px){.results>.consumer-editor{min-width:700px}}";
const INTEGRATED_TABLE_STYLE: &str = ".results{display:block;overflow-x:auto}.integrated-table-heading{display:flex;align-items:center;justify-content:space-between;gap:16px;margin:24px 0 12px}.results>.integrated-results-table{display:table;width:100%;min-width:1240px;table-layout:fixed}.results>.integrated-results-table th:nth-child(1){width:150px}.results>.integrated-results-table th:nth-child(2),.results>.integrated-results-table th:nth-child(3),.results>.integrated-results-table th:nth-child(4){width:112px}.results>.integrated-results-table th:nth-child(5){width:138px}.results>.integrated-results-table th:nth-child(6){width:250px}.results>.integrated-results-table th:nth-child(7){width:132px}.results>.integrated-results-table th:nth-child(8){width:142px}.integrated-results-table tbody tr{height:62px}.integrated-results-table tbody td{height:62px;vertical-align:middle}.integrated-results-table tbody td:first-child strong{font-weight:700;color:#20364d}.integrated-results-table input{min-width:0;height:38px;padding-top:7px;padding-bottom:7px}.integrated-results-table .consumer-name-cell{display:grid;grid-template-columns:minmax(0,1fr) auto;align-items:center;gap:4px}.integrated-results-table .consumer-picker{min-width:0}.integrated-results-table .consumer-picker input{width:100%}.integrated-results-table td:has(.consumer-picker:focus-within),.integrated-results-table tr:has(.consumer-picker:focus-within){position:relative;z-index:1000000;overflow:visible}.integrated-results-table .consumer-menu{z-index:1000001;min-width:250px}.integrated-results-table tbody tr:nth-child(n+4):nth-last-child(-n+3) .consumer-menu,.integrated-results-table tbody tr:nth-child(n+4):nth-last-child(-n+3) .time-menu{top:auto;bottom:100%}.integrated-results-table .remove{font-size:18px;padding:4px 6px}.integrated-results-table .mandatory-consumer{height:38px;line-height:24px}";
const MODAL_STYLE: &str = ".modal-backdrop{position:fixed!important;inset:0!important;z-index:2147483646!important;isolation:isolate;background:rgba(15,23,42,.72)!important;pointer-events:auto}.modal-backdrop>.drug-modal,.modal-backdrop>.confirm-modal{position:relative;z-index:1;isolation:isolate}.modal-backdrop~*,body:has(.modal-backdrop) .shell{pointer-events:none}.modal-backdrop,.modal-backdrop *{pointer-events:auto}.delete-drug{width:100%;margin-top:10px;background:#fff1f0;color:#b42318;border:1px solid #f1aaa3}.delete-drug:hover,.danger:hover{background:#b42318;color:#fff}.confirm-backdrop{z-index:2147483647!important}.confirm-modal{width:min(440px,calc(100vw - 32px));background:#fff;border-radius:14px;padding:24px;box-shadow:0 24px 70px #0007;text-align:center}.confirm-modal p{margin:12px 0 22px;color:#68778f;line-height:1.5}.confirm-actions{display:grid;grid-template-columns:1fr 1fr;gap:10px}.danger{background:#d92d20;color:#fff}";
const TAB_AND_REPORT_STYLE: &str = ".title-and-tabs{display:flex;align-items:center;gap:14px;min-width:0;pointer-events:auto!important}.version-badge{margin-left:-7px;padding:2px 6px;border:1px solid #c9d4e2;border-radius:10px;background:#edf2f7;color:#68778f;font-size:10px;font-weight:700;line-height:1.2;white-space:nowrap}.calculation-tabs{display:flex;align-items:center;gap:5px;min-width:0;overflow-x:auto}.calculation-tab{display:flex;align-items:center;padding:0;background:#eef2f6;color:#52627a;border:1px solid #d5dde7;border-radius:7px;font-size:12px;white-space:nowrap;overflow:hidden;transition:background-color .15s,border-color .15s}.calculation-tab:hover{background:#dce7f4;border-color:#9eb2cc}.calculation-tab.active{background:#3974d8;color:#fff;border-color:#3974d8}.calculation-tab.active:hover{background:#2f61b8;border-color:#2f61b8}.tab-title,.tab-edit,.tab-close{padding:7px 9px;background:transparent!important;color:inherit;border-radius:0;font-size:12px}.tab-title:hover{background:transparent!important}.tab-edit{padding-left:5px;padding-right:5px;font-size:14px}.tab-edit:hover{background:#ffffff33!important}.tab-close{padding-left:5px;padding-right:8px;font-size:16px}.tab-close:hover{background:#d92d20!important;color:#fff}.add-tab{width:32px;height:32px;padding:0;border:1px solid #9eb2cc;background:#fff;color:#315b86;font-size:20px;line-height:28px}.report-modal{position:relative;z-index:1;width:min(700px,calc(100vw - 32px));max-height:80vh;background:#fff;border-radius:14px;padding:24px;box-shadow:0 24px 70px #0007}.report-list{display:flex;flex-direction:column;gap:7px;margin:18px 0;max-height:52vh;overflow-y:auto}.report-row{display:grid;grid-template-columns:minmax(0,1fr) 42px;align-items:stretch;width:100%;background:#f7f9fc;color:#20364d;border:1px solid #dce3ec;border-radius:7px;overflow:hidden}.report-row:hover{background:#eaf2ff}.report-open{display:grid;grid-template-columns:155px minmax(120px,1fr) 145px;align-items:center;gap:12px;padding:12px 14px;text-align:left;background:transparent;color:#20364d}.report-open:hover{background:transparent}.report-name{display:flex;min-width:0;flex-direction:column;gap:3px}.report-name strong,.report-name small{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.report-name small{color:#68778f;font-weight:400}.report-time,.report-consumers{color:#68778f;font-variant-numeric:tabular-nums}.report-delete{padding:0;background:transparent;color:#a33a3a;font-size:22px;border-radius:0}.report-delete:hover{background:#fee4e2;color:#b42318}.pagination{display:flex;align-items:center;justify-content:center;gap:14px}.empty-reports{padding:30px 0;text-align:center;color:#68778f}.close-tab-actions{display:grid;gap:9px}.info-toast{position:fixed;right:24px;bottom:24px;z-index:2147483647;display:flex;align-items:center;gap:18px;max-width:min(620px,calc(100vw - 48px));padding:18px 18px 18px 22px;background:#173b70;color:#fff;border:1px solid #6f9ad1;border-radius:12px;box-shadow:0 16px 45px #10213b66;font-size:21px;line-height:1.35;animation:toast-lifetime 10s ease forwards}.toast-close{flex:0 0 auto;padding:1px 7px;background:transparent;color:#fff;font-size:25px}.toast-close:hover{background:#ffffff22}@keyframes toast-lifetime{0%,88%{opacity:1;transform:translateY(0);visibility:visible}100%{opacity:0;transform:translateY(10px);visibility:hidden}}@media(max-width:900px){.topbar{flex-wrap:wrap;height:auto;padding-top:10px;padding-bottom:10px}.title-and-tabs{width:100%;flex-wrap:wrap}.topbar-actions{width:100%;justify-content:flex-end}.report-open{grid-template-columns:1fr}.info-toast{right:12px;bottom:12px;max-width:calc(100vw - 24px)}}";
const VOLUME_LIMIT_STYLE: &str = ".volume-badge{display:inline-flex;align-items:center;justify-content:center;min-width:52px;padding:5px 9px;border:1px solid;border-radius:999px;font-weight:800;line-height:1}.volume-badge.safe{background:#dcfae6;color:#067647;border-color:#47cd89}.volume-badge.warning{background:#fef0c7;color:#93370d;border-color:#fec84b}.volume-badge.danger{background:#fee4e2;color:#b42318;border-color:#f04438;cursor:help}.volume-badge.neutral{background:#f2f4f7;color:#475467;border-color:#d0d5dd}.consumer-header-action{display:flex;align-items:center;justify-content:center;gap:8px}.consumer-header-action .small{padding:5px 8px;font-size:11px;text-transform:none;white-space:nowrap}.integrated-results-table tfoot tr:first-child td{bottom:50px}.integrated-results-table tfoot .dilution-row td{bottom:0;background:#fff7e8;color:#8b5e00;border-top:1px solid #e8c780;font-weight:700}.integrated-results-table tfoot .excess-row td{background:#eef4ff;color:#315b86;border-top-color:#8fb2d9}.integrated-results-table tfoot .series-summary-side{bottom:0!important;background:#edf4ff;color:#315b86;border:2px solid #8fb2d9;text-align:center;vertical-align:middle}.series-summary-side span{display:inline-block;padding:6px 12px;border-radius:999px;background:#dceaff;font-weight:800}.dilution-explanation{text-align:left!important;font-size:12px;font-weight:500!important;color:#8b6b28!important}.excess-row .dilution-explanation{color:#315b86!important}";
const CYCLOTRON_CONTROL_STYLE: &str = ".target-current-control{display:grid;grid-template-columns:minmax(150px,1fr) 130px;grid-template-areas:'heading input' 'slider input';align-items:center;column-gap:12px;padding:12px;margin-top:9px;background:#f7f9fc;border:1px solid #e1e7ef;border-radius:9px}.target-current-heading{grid-area:heading;display:flex;align-items:center;justify-content:space-between;gap:10px;color:#475467;font-size:13px}.target-current-heading strong{color:#244d91;font-variant-numeric:tabular-nums}.current-slider{grid-area:slider;-webkit-appearance:none;appearance:none;width:100%;height:6px;padding:0;border:0;border-radius:999px;outline:0!important;background:linear-gradient(to right,#3974d8 0,#3974d8 var(--current-fill),#d7e0eb var(--current-fill),#d7e0eb 100%)}.current-slider::-webkit-slider-thumb{-webkit-appearance:none;appearance:none;width:16px;height:16px;border:2px solid #fff;border-radius:50%;background:#3974d8;box-shadow:0 1px 4px #244d9166;cursor:pointer}.target-current-control>.input-with-unit{grid-area:input}.target-current-control>.input-with-unit input{height:42px;padding-top:8px;padding-right:58px;padding-bottom:8px}.target-current-control>.input-with-unit .field-unit{right:30px}.irradiation-summary{margin-top:18px;border:2px solid #8fb2d9;border-radius:10px;background:#f4f8fd;color:#315b86;overflow:hidden}.irradiation-metric{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:12px 15px}.irradiation-metric+.irradiation-metric{border-top:1px solid #c8d9ec}.irradiation-metric span,.irradiation-metric strong{font-weight:800}.irradiation-metric span{font-size:14px}.irradiation-metric strong{font-size:19px;font-variant-numeric:tabular-nums;text-align:right}.irradiation-summary.unreachable{border-color:#f04438;background:#fff1f0;color:#b42318}.irradiation-summary.invalid{border-color:#d0d5dd;background:#f9fafb;color:#667085}";
const VIEWPORT_LAYOUT_STYLE: &str = "html,body,#main{width:100%;height:100%;overflow:hidden}.shell{width:100vw;height:100vh;min-height:0;overflow:hidden}.workspace{display:grid!important;grid-template-columns:minmax(360px,410px) minmax(0,1fr);width:100vw;max-width:none;height:calc(100vh - 72px);min-height:0;overflow:hidden;padding:14px;gap:14px;align-items:stretch}.sidebar{height:100%;min-width:0;min-height:0;width:auto!important;flex:none!important;overflow:visible}.sidebar>.panel{padding:12px 14px;margin-bottom:9px}.sidebar h2{font-size:17px}.sidebar label{margin:7px 0 4px;font-size:13px}.sidebar input,.sidebar select{padding:7px 9px;height:36px}.sidebar .settings-button{margin-top:7px;padding:8px 12px}.sidebar .target-toggle{padding:3px}.sidebar .target-choice{padding:6px}.sidebar .target-current-control{padding:7px 9px;margin-top:6px}.sidebar .target-current-control>.input-with-unit input{height:36px}.sidebar .irradiation-result{margin-top:10px;padding:10px 12px}.sidebar .irradiation-result strong{font-size:17px}.sidebar .hint{margin:7px 0 0}.results.panel{width:100%!important;max-width:none!important;height:100%;min-width:0!important;min-height:0;margin:0;padding:14px;overflow:hidden;display:flex!important;flex-direction:column}.results-heading{flex:0 0 auto;margin-bottom:8px}.integrated-table-heading{flex:0 0 auto;margin:7px 0 8px}.results-table-scroll{width:100%;max-width:100%;flex:1 1 auto;min-width:0;min-height:0;overflow:auto;border:1px solid #e5eaf0;border-radius:8px}.results>.results-table-scroll>.integrated-results-table{display:table;width:100%;margin:0;min-width:1240px}.integrated-results-table thead th{position:sticky;top:0;z-index:40}.integrated-results-table tfoot td{position:sticky;bottom:0;z-index:35}.integrated-results-table tbody tr,.integrated-results-table tbody td{height:50px}.integrated-results-table th,.integrated-results-table td{padding-top:6px!important;padding-bottom:6px!important}.integrated-results-table input{height:34px}.results>.mock-note{flex:0 0 auto;margin-top:7px;padding:8px}.sidebar section.panel:not(.consumers-panel) .time-menu{top:auto;bottom:100%;max-height:min(220px,38vh);overflow-y:auto}@media(max-width:1100px){.workspace{grid-template-columns:minmax(330px,360px) minmax(0,1fr);padding:8px;gap:8px}.results.panel{padding:10px}}@media(max-width:800px){.workspace{grid-template-columns:300px minmax(0,1fr)}}";
const IRRADIATION_COMPACT_STYLE: &str = ".sidebar .irradiation-summary{margin-top:10px}.sidebar .irradiation-metric{padding:8px 12px}.sidebar .irradiation-metric strong{font-size:17px}.sidebar input.current-slider{height:6px;padding:0;border:0}.target-current-control{transition:opacity .18s,filter .18s,background-color .18s}.target-current-control.muted{opacity:.48;filter:saturate(.25);background:#eef1f5}.target-current-control.muted .current-slider,.target-current-control.muted input{cursor:not-allowed}.irradiation-value-badge,.extreme-activity-badge{display:inline-flex;align-items:center;justify-content:center;padding:5px 9px;border-radius:999px;font-variant-numeric:tabular-nums}.irradiation-value-badge{background:#dceaff;color:#315b86}.irradiation-value-badge.danger,.extreme-activity-badge{background:#fee4e2;color:#b42318;border:1px solid #f04438;font-weight:800;cursor:help}";
const ISOTOPE_STYLE: &str = ".isotope-settings-launch{margin-top:auto;padding:3px 2px 0}.isotope-settings-launch small{display:block;padding:0 4px;color:#68778f;font-size:11px}.isotope-settings-launch .settings-button{margin-top:4px}.cyclotron-panel.isotope-muted{position:relative;opacity:.52;filter:grayscale(.65);background:#f3f5f7}.cyclotron-panel.isotope-muted button,.cyclotron-panel.isotope-muted input{pointer-events:none}.cyclotron-disabled-note{margin:5px 0 2px;padding:6px 8px;border-radius:6px;background:#e5e7eb;color:#475467;font-size:11px;font-weight:700}.isotope-modal{width:min(660px,calc(100vw - 32px));max-height:86vh;display:flex;flex-direction:column;background:#fff;border-radius:14px;padding:24px;box-shadow:0 20px 60px #10213b55}.isotope-list{margin-top:14px;overflow-y:auto;border:1px solid #dce3ec;border-radius:9px}.isotope-row{display:grid;grid-template-columns:minmax(180px,1fr) minmax(190px,.75fr);gap:12px;padding:8px 10px;border-bottom:1px solid #e5eaf0}.isotope-row:last-child{border-bottom:0}.isotope-header{position:sticky;top:0;z-index:2;background:#f3f6fa;color:#52627a;font-size:12px;font-weight:700;text-transform:uppercase}.isotope-row input{height:38px;padding:7px 10px}.isotope-row .input-with-unit input{padding-right:78px}.isotope-row .input-with-unit .field-unit{right:34px}.isotope-modal-actions{display:flex;justify-content:space-between;gap:12px;margin-top:16px}";
const INTERFACE_THEME_STYLE: &str = ".shell button:not(.secondary):not(.settings-button):not(.remove):not(.close):not(.window-button):not(.tab-edit):not(.tab-close):not(.report-delete):not(.toast-close):not(.time-option):not(.consumer-option):not(.consumer-create):not(.report-open):not(.target-choice){background:var(--interface-accent);color:var(--interface-on-accent)}.shell button:not(.secondary):not(.settings-button):not(.remove):not(.close):not(.window-button):not(.tab-edit):not(.tab-close):not(.report-delete):not(.toast-close):not(.time-option):not(.consumer-option):not(.consumer-create):not(.report-open):not(.target-choice):hover{background:var(--interface-dark);color:#fff}.shell .secondary,.shell .settings-button{background:var(--interface-light);color:var(--interface-dark)}.shell .secondary:hover,.shell .settings-button:hover{background:color-mix(in srgb,var(--interface-light) 82%,var(--interface-accent));color:var(--interface-dark)}.shell input:focus,.shell select:focus{outline-color:var(--interface-accent);border-color:var(--interface-accent)}.shell .target-choice.active,.shell .calculation-tab.active{background:var(--interface-accent);border-color:var(--interface-accent);color:var(--interface-on-accent)}.shell .current-slider{background:linear-gradient(to right,var(--interface-accent) 0,var(--interface-accent) var(--current-fill),#d7e0eb var(--current-fill),#d7e0eb 100%)}.shell .current-slider::-webkit-slider-thumb{background:var(--interface-accent)}.shell .field-unit,.shell .target-current-heading strong{color:var(--interface-dark)}.shell .tag,.shell .live,.shell .mandatory-consumer{color:var(--interface-dark);background:var(--interface-light)}.shell .irradiation-summary{border-color:var(--interface-accent);background:var(--interface-light);color:var(--interface-dark)}.shell .irradiation-value-badge,.shell .series-summary-side span{background:var(--interface-light);color:var(--interface-dark)}.interface-modal{width:min(460px,calc(100vw - 32px));background:#fff;border-radius:14px;padding:24px;box-shadow:0 20px 60px #10213b55}.color-picker-row{display:grid;grid-template-columns:76px 1fr;gap:12px;align-items:center;margin-top:18px}.color-picker{height:52px!important;padding:3px;cursor:pointer}.color-hex{height:52px}.theme-preview{display:flex;align-items:center;justify-content:center;height:72px;margin-top:14px;border-radius:10px;color:var(--interface-on-accent);font-weight:800}.interface-modal-actions{display:flex;justify-content:space-between;gap:12px;margin-top:18px}";
const INTERFACE_SURFACE_STYLE: &str = ".shell{background:color-mix(in srgb,var(--interface-light) 46%,#f4f7fb)}.shell .workspace{background:transparent}.shell .topbar{background:color-mix(in srgb,var(--interface-light) 58%,white);border-bottom-color:color-mix(in srgb,var(--interface-accent) 35%,#dce3ec)}.shell .panel,.shell .drug-modal,.shell .confirm-modal,.shell .report-modal,.shell .isotope-modal,.shell .interface-modal{background:color-mix(in srgb,var(--interface-light) 16%,white);border:1px solid color-mix(in srgb,var(--interface-accent) 30%,#dce3ec);box-shadow:0 12px 34px color-mix(in srgb,var(--interface-dark) 16%,transparent)}.shell .modal-backdrop{background:color-mix(in srgb,var(--interface-dark) 72%,transparent)!important}.shell input,.shell select{background:color-mix(in srgb,var(--interface-light) 8%,white);border-color:color-mix(in srgb,var(--interface-accent) 25%,#cbd5e1)}.shell .results-table-scroll,.shell .isotope-list{border-color:color-mix(in srgb,var(--interface-accent) 34%,#dce3ec)}.shell .consumer-editor th,.shell .results-table th,.shell .isotope-header{background:color-mix(in srgb,var(--interface-light) 72%,white);color:var(--interface-dark)}.shell .consumer-editor th,.shell .consumer-editor td,.shell .results-table th,.shell .results-table td,.shell .isotope-row{border-color:color-mix(in srgb,var(--interface-accent) 20%,#e5eaf0)}.shell .integrated-results-table tbody tr:nth-child(even) td,.shell .consumer-editor tbody tr:nth-child(even) td{background:color-mix(in srgb,var(--interface-light) 25%,white)}.shell .results-table .request-group{background:color-mix(in srgb,var(--interface-light) 38%,white);border-color:color-mix(in srgb,var(--interface-accent) 58%,#dce3ec)}.shell .results-table th.request-group{background:color-mix(in srgb,var(--interface-light) 78%,white);color:var(--interface-dark)}.shell .results-table .total-row td{background:color-mix(in srgb,var(--interface-light) 62%,white);border-top-color:var(--interface-dark);color:var(--interface-dark)}.shell .results-table .total-row .request-group{background:color-mix(in srgb,var(--interface-light) 42%,white)}.shell .target-toggle,.shell .target-current-control{background:color-mix(in srgb,var(--interface-light) 46%,white);border-color:color-mix(in srgb,var(--interface-accent) 24%,#dce3ec)}.shell .target-choice:not(.active):hover{background:color-mix(in srgb,var(--interface-light) 70%,white);color:var(--interface-dark)}.shell .time-menu,.shell .consumer-menu{background:color-mix(in srgb,var(--interface-light) 15%,white);border-color:color-mix(in srgb,var(--interface-accent) 45%,#b8c4d4);box-shadow:0 8px 22px color-mix(in srgb,var(--interface-dark) 20%,transparent)}.shell .time-option,.shell .consumer-option,.shell .consumer-create{background:transparent}.shell .time-option:hover,.shell .consumer-option:hover,.shell .consumer-create:hover{background:var(--interface-light);color:var(--interface-dark)}.shell .consumer-create{color:var(--interface-dark)}.shell .calculation-tab:not(.active),.shell .version-badge{background:color-mix(in srgb,var(--interface-light) 55%,white);border-color:color-mix(in srgb,var(--interface-accent) 28%,#d5dde7);color:var(--interface-dark)}.shell .calculation-tab:not(.active):hover{background:color-mix(in srgb,var(--interface-light) 78%,white);border-color:var(--interface-accent)}.shell .report-row{background:color-mix(in srgb,var(--interface-light) 25%,white);border-color:color-mix(in srgb,var(--interface-accent) 28%,#dce3ec)}.shell .report-row:hover{background:color-mix(in srgb,var(--interface-light) 68%,white)}.info-toast{background:var(--interface-dark);border-color:var(--interface-accent);box-shadow:0 16px 45px color-mix(in srgb,var(--interface-dark) 38%,transparent)}.shell .integrated-results-table tfoot .excess-row td,.shell .integrated-results-table tfoot .series-summary-side{background:color-mix(in srgb,var(--interface-light) 72%,white);color:var(--interface-dark);border-color:var(--interface-accent)}";
const FONT_SCALE_STYLE: &str = ".shell{font-size:calc(16px + var(--font-increase))}.shell h1{font-size:calc(20px + var(--font-increase))}.shell h2{font-size:calc(19px + var(--font-increase))}.shell .sidebar h2{font-size:calc(17px + var(--font-increase))}.shell .topbar p,.shell .time-option,.shell .consumer-option,.shell .consumer-create,.shell .small,.shell .target-current-heading{font-size:calc(13px + var(--font-increase))}.shell label,.shell .hint,.shell .results-table th small,.shell .irradiation-metric span{font-size:calc(14px + var(--font-increase))}.shell .consumer-editor,.shell .results-table{font-size:calc(15px + var(--font-increase))}.shell .consumer-editor th,.shell .results-table th,.shell .status,.shell .calculation-tab,.shell .tab-title,.shell .tab-edit,.shell .isotope-header,.shell .consumer-empty{font-size:calc(12px + var(--font-increase))}.shell .tag,.shell .live,.shell .field-unit,.shell .consumer-header-action .small,.shell .isotope-settings-launch small,.shell .cyclotron-disabled-note,.shell .time-field small{font-size:calc(11px + var(--font-increase))}.shell .consumers-panel .consumer-editor th,.shell .version-badge{font-size:calc(10px + var(--font-increase))}.shell .consumers-panel .consumer-editor,.shell .consumers-panel input,.shell .consumers-panel select{font-size:calc(12px + var(--font-increase))}.shell .irradiation-metric strong{font-size:calc(19px + var(--font-increase))}.shell .remove,.shell .close{font-size:calc(20px + var(--font-increase))}.shell .tab-close{font-size:calc(16px + var(--font-increase))}.shell .report-delete{font-size:calc(22px + var(--font-increase))}.info-toast{font-size:calc(21px + var(--font-increase))}.info-toast .toast-close{font-size:calc(25px + var(--font-increase))}.shell input:not([type=range]):not([type=color]):not([type=checkbox]),.shell select{height:calc(36px + var(--font-increase))!important;min-height:calc(36px + var(--font-increase));padding-top:7px;padding-bottom:7px;line-height:1.2}.shell .integrated-results-table tbody tr,.shell .integrated-results-table tbody td{height:calc(50px + var(--font-increase))!important;min-height:calc(50px + var(--font-increase))}.shell .integrated-results-table thead tr{height:calc(68px + var(--font-increase))}.shell .mandatory-consumer{display:flex!important;align-items:center;height:auto!important;min-height:calc(38px + var(--font-increase));line-height:1.2!important}.shell .sidebar{overflow-y:auto;padding-right:4px}.shell .drug-modal,.shell .confirm-modal,.shell .report-modal,.shell .isotope-modal,.shell .interface-modal{max-height:90vh;overflow:auto}.font-size-setting{margin-top:18px;padding:14px;border:1px solid color-mix(in srgb,var(--interface-accent) 30%,#dce3ec);border-radius:10px;background:color-mix(in srgb,var(--interface-light) 28%,white)}.font-size-heading{display:flex;align-items:center;justify-content:space-between}.font-size-heading label{margin:0}.font-size-heading strong{color:var(--interface-dark)}.font-step-slider{width:100%;height:6px!important;margin:18px 0 8px;padding:0!important;accent-color:var(--interface-accent);cursor:pointer}.font-step-labels{display:flex;justify-content:space-between;color:#68778f;font-size:calc(11px + var(--font-increase));font-variant-numeric:tabular-nums}";
const PRINT_STYLE: &str = ".print-preview-overlay{padding:24px;overflow:auto;align-items:flex-start!important}.print-document{width:min(1120px,96vw);aspect-ratio:297/210;max-height:calc(100vh - 48px);overflow:auto;background:#fff;color:#172b4d;border-radius:12px;padding:28px;box-shadow:0 24px 70px #0007}.print-actions{display:flex;align-items:center;justify-content:space-between;gap:18px;margin-bottom:24px;padding-bottom:16px;border-bottom:1px solid #d0d5dd}.print-actions>div{display:flex;gap:10px}.print-header h1{margin:0 0 20px;font-size:24px;text-align:center}.print-metadata{display:grid;grid-template-columns:1fr .7fr 1fr;gap:12px;margin-bottom:22px}.print-metadata>div{display:flex;flex-direction:column;gap:5px;padding:12px;border:1px solid #cbd5e1;border-radius:8px;background:#f8fafc}.print-metadata span{color:#667085;font-size:12px}.print-metadata strong{font-size:17px}.print-table{width:100%;border-collapse:collapse;table-layout:fixed;font-size:13px}.print-table th,.print-table td{padding:9px 7px;border:1px solid #98a2b3;text-align:center;vertical-align:middle}.print-table th{background:#eaf2fb;color:#20364d}.print-table tbody td:first-child{text-align:left}.print-table tfoot td{background:#e8eef5;font-weight:800}.print-table .print-volume-column{background:#dceaff;border-left:2px solid #315b86;border-right:2px solid #315b86;font-size:15px}.print-table tbody .print-volume-column strong,.print-table tfoot .print-volume-column strong{display:inline-block;padding:6px 10px;border:2px solid #315b86;border-radius:999px;background:#fff;color:#173b70;font-size:16px}.print-adjustment{display:flex;align-items:center;justify-content:space-between;margin-top:18px;padding:18px 22px;border:3px solid #315b86;border-radius:10px;background:#eaf2fb;color:#173b70}.print-adjustment span{font-size:20px;font-weight:800}.print-adjustment strong{font-size:26px}.print-adjustment.dilution{border-color:#8b5e00;background:#fff3d6;color:#714b00}@media print{@page{size:A4 landscape;margin:10mm}body{background:#fff!important}body *{visibility:hidden!important}.print-document,.print-document *{visibility:visible!important}.print-document{position:absolute!important;inset:0!important;width:100%!important;aspect-ratio:auto!important;max-height:none!important;overflow:visible!important;padding:0!important;border:0!important;border-radius:0!important;box-shadow:none!important;background:#fff!important}.print-actions{display:none!important}.print-header h1{font-size:18pt!important}.print-metadata strong{font-size:11pt!important}.print-table{font-size:8.5pt!important}.print-table th,.print-table td{padding:5pt 4pt!important}.print-table .print-volume-column{font-size:10pt!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}.print-table tbody .print-volume-column strong,.print-table tfoot .print-volume-column strong{font-size:11pt!important}.print-adjustment{-webkit-print-color-adjust:exact;print-color-adjust:exact;break-inside:avoid}.print-adjustment span{font-size:15pt!important}.print-adjustment strong{font-size:19pt!important}}";
const DROPDOWN_THEME_STYLE: &str = ".shell .time-menu,.shell .consumer-menu{padding:5px;background:color-mix(in srgb,var(--interface-light) 12%,white);border:1px solid color-mix(in srgb,var(--interface-accent) 48%,#cbd5e1);box-shadow:0 10px 28px color-mix(in srgb,var(--interface-dark) 24%,transparent)}.shell .time-option,.shell .consumer-option,.shell .consumer-create{width:100%;background:transparent!important;color:#172b4d!important;border:0;text-align:left;box-shadow:none}.shell .time-option:hover,.shell .consumer-option:hover,.shell .consumer-create:hover,.shell .time-option:focus,.shell .consumer-option:focus,.shell .consumer-create:focus{background:var(--interface-light)!important;color:var(--interface-dark)!important;outline:none}.shell .consumer-option.selected{background:color-mix(in srgb,var(--interface-light) 72%,white)!important;color:var(--interface-dark)!important;font-weight:700}.shell .consumer-create{margin-top:4px;border-top:1px solid color-mix(in srgb,var(--interface-accent) 28%,#dce3ec);border-radius:0 0 4px 4px;color:var(--interface-dark)!important;font-weight:700}.shell .select-picker{z-index:50}.shell .select-picker:focus-within{z-index:100001}.shell .select-picker-input{cursor:pointer;background:color-mix(in srgb,var(--interface-light) 8%,white)!important;color:#172b4d!important;font-weight:400!important}.shell .select-picker-input:hover{background:color-mix(in srgb,var(--interface-light) 18%,white)!important;border-color:var(--interface-accent)}.shell .select-picker-menu{top:calc(100% + 5px);bottom:auto;max-height:220px;z-index:100002}.shell .report-open{background:transparent!important;color:#20364d!important}.shell .report-open:hover{background:transparent!important;color:#20364d!important}.shell .target-choice{background:transparent;color:#475467}.shell .target-choice:hover{background:var(--interface-light);color:var(--interface-dark)}.shell .target-choice.active,.shell .target-choice.active:hover{background:var(--interface-accent);color:var(--interface-on-accent)}";
const PRINT_PORTRAIT_STYLE: &str = ".print-document{width:min(794px,96vw);aspect-ratio:210/297;padding:24px}.print-header h1{font-size:26px}.print-metadata{grid-template-columns:1fr 1fr;gap:8px;margin-bottom:16px}.print-metadata>div{padding:9px}.print-metadata span{font-size:14px}.print-metadata strong{font-size:19px}.print-table{font-size:15px}.print-table th,.print-table td{padding:5px 3px;line-height:1.12;overflow-wrap:anywhere}.print-table th:nth-child(1){width:20%}.print-table th:nth-child(2),.print-table th:nth-child(3),.print-table th:nth-child(4){width:11%}.print-table th:nth-child(5){width:17%}.print-table th:nth-child(6){width:20%}.print-table th:nth-child(7){width:10%}.print-table .print-volume-column{font-size:17px}.print-table tbody .print-volume-column strong,.print-table tfoot .print-volume-column strong{padding:4px 6px;font-size:18px}.print-adjustment{margin-top:12px;padding:13px 16px}.print-adjustment span{font-size:22px}.print-adjustment strong{font-size:28px}@media print{@page{size:A4 portrait;margin:8mm}.print-header h1{font-size:20pt!important}.print-metadata span{font-size:9pt!important}.print-metadata strong{font-size:13pt!important}.print-table{font-size:10.5pt!important}.print-table th,.print-table td{padding:2.5pt 1.5pt!important;line-height:1.08!important}.print-table .print-volume-column{font-size:12pt!important}.print-table tbody .print-volume-column strong,.print-table tfoot .print-volume-column strong{padding:3pt 4pt!important;font-size:13pt!important}.print-adjustment{margin-top:7pt!important;padding:8pt 10pt!important}.print-adjustment span{font-size:17pt!important}.print-adjustment strong{font-size:21pt!important}}";
const CONSUMER_BADGE_STYLE: &str = ".shell .consumer-picker .consumer-name-input{font-size:calc(15px + var(--font-increase))!important}.shell .consumer-picker .consumer-name-input.filled{padding:7px 5px;background:color-mix(in srgb,var(--interface-light) 65%,var(--interface-accent))!important;border:1px solid color-mix(in srgb,var(--interface-accent) 48%,#dce3ec)!important;border-radius:5px;color:var(--interface-dark)!important;font-weight:700;box-shadow:none}.shell .consumer-picker .consumer-name-input.filled:hover{background:color-mix(in srgb,var(--interface-light) 52%,var(--interface-accent))!important}.shell .consumer-picker .consumer-name-input.filled:focus{background:color-mix(in srgb,var(--interface-light) 42%,var(--interface-accent))!important;color:var(--interface-dark)!important}.print-consumer-badge{display:inline-block;width:100%;padding:5px 6px;border-radius:6px;font-size:16px;font-weight:800;line-height:1.15;text-align:left}.print-consumer-badge.technical{background:#eaf2fb;color:#244d91;border:1px solid #8fb2d9}.print-consumer-badge.custom{background:#315b86;color:#fff;border:1px solid #173b70}@media print{.print-consumer-badge{padding:3pt 4pt!important;font-size:12pt!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}.print-consumer-badge.custom{background:#315b86!important;color:#fff!important}.print-consumer-badge.technical{background:#eaf2fb!important;color:#244d91!important}}";
const PRINT_TABLE_COMPACT_STYLE: &str = ".print-table{font-size:12px}.print-table tbody td{padding:4px 2px;white-space:nowrap}.print-table tbody td:first-child{overflow:visible}.print-consumer-badge{padding:4px 5px;font-size:12px;white-space:nowrap}.print-table tbody .print-volume-column strong,.print-table tfoot .print-volume-column strong{display:inline;padding:0;border:0;border-radius:0;background:transparent;color:inherit;font-size:inherit}.print-table .print-volume-column{background:#dceaff}@media print{.print-table{font-size:8.5pt!important}.print-table tbody td{padding:2pt 1pt!important;white-space:nowrap!important}.print-consumer-badge{padding:2pt 3pt!important;font-size:8.5pt!important;white-space:nowrap!important}.print-table tbody .print-volume-column strong,.print-table tfoot .print-volume-column strong{display:inline!important;padding:0!important;border:0!important;border-radius:0!important;background:transparent!important;color:inherit!important;font-size:inherit!important}.print-table .print-volume-column{background:#dceaff!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}}";
const PRINT_CONSUMER_TEXT_STYLE: &str = ".print-consumer-name{font-size:inherit;font-weight:800;color:#172b4d;white-space:nowrap}@media print{.print-consumer-name{font-size:inherit!important;color:#000!important;background:transparent!important;border:0!important}}";
const PRINT_METADATA_COMPACT_STYLE: &str = ".print-header h1{margin-bottom:12px}.print-metadata{grid-template-columns:repeat(3,minmax(0,1fr));gap:5px;margin-bottom:10px}.print-metadata>div{gap:2px;padding:6px 8px;border-radius:6px}.print-metadata span{font-size:11px;line-height:1.1}.print-metadata strong{font-size:14px;line-height:1.15;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}@media print{.print-header h1{margin-bottom:7pt!important}.print-metadata{grid-template-columns:repeat(3,minmax(0,1fr))!important;gap:3pt!important;margin-bottom:6pt!important}.print-metadata>div{gap:1pt!important;padding:3.5pt 5pt!important;border-radius:4pt!important}.print-metadata span{font-size:7.5pt!important;line-height:1.05!important}.print-metadata strong{font-size:10pt!important;line-height:1.1!important}}";
const TAB_CONTRAST_STYLE: &str = ".shell .calculation-tab:not(.active){background:color-mix(in srgb,var(--interface-light) 36%,white);color:#172b4d!important;border-color:color-mix(in srgb,var(--interface-accent) 38%,#cbd5e1)}.shell .calculation-tab:not(.active) .tab-title,.shell .calculation-tab:not(.active) .tab-edit,.shell .calculation-tab:not(.active) .tab-close{color:#172b4d!important;font-weight:700}.shell .calculation-tab:not(.active):hover{background:color-mix(in srgb,var(--interface-light) 62%,white);border-color:var(--interface-accent)}.shell .calculation-tab.active,.shell .calculation-tab.active .tab-title,.shell .calculation-tab.active .tab-edit,.shell .calculation-tab.active .tab-close{color:var(--interface-on-accent)!important}.shell .calculation-tab .tab-close:hover{color:#fff!important}";
const VIAL_GROUP_STYLE: &str = ".split-vials-toggle{display:inline-flex;align-items:center;gap:5px;margin:0;padding:4px 7px;border:1px solid color-mix(in srgb,var(--interface-accent) 32%,#dce3ec);border-radius:999px;background:color-mix(in srgb,var(--interface-light) 48%,white);color:var(--interface-dark);font-size:calc(10px + var(--font-increase));font-weight:700;text-transform:none;white-space:nowrap;cursor:pointer}.split-vials-toggle input{width:15px;height:15px;margin:0;accent-color:var(--interface-accent);cursor:pointer}.vial-group-row td{background:color-mix(in srgb,var(--interface-light) 24%,white)!important}.vial-group-row td:first-child{border-left:4px solid var(--interface-accent)}.vial-group-first td{border-top:2px solid color-mix(in srgb,var(--interface-accent) 62%,#dce3ec)}.vial-group-last td{border-bottom:2px solid color-mix(in srgb,var(--interface-accent) 62%,#dce3ec)}.vial-group-label{display:inline-block;margin:0 0 4px;padding:3px 7px;border-radius:999px;background:var(--interface-accent);color:var(--interface-on-accent);font-size:calc(10px + var(--font-increase));font-weight:800;white-space:nowrap}.vial-group-row .consumer-name-cell>div{min-width:0}";
const VIAL_GROUP_REFINEMENT_STYLE: &str = ".vial-group-first{position:relative}.vial-group-label{position:absolute;z-index:12;left:50%;top:0;transform:translate(-50%,-25%);margin:0;padding:3px 9px;font-weight:500!important;line-height:1.1;box-shadow:0 1px 3px color-mix(in srgb,var(--interface-dark) 20%,transparent)}.vial-original-activity{display:table;margin:0 auto 4px;padding:3px 7px;border:1px solid color-mix(in srgb,var(--interface-accent) 48%,#dce3ec);border-radius:999px;background:color-mix(in srgb,var(--interface-light) 58%,white);color:var(--interface-dark);font-size:calc(10px + var(--font-increase));font-weight:600;line-height:1.1;white-space:nowrap}";
const PRINT_ADJUSTMENT_STYLE: &str = ".print-adjustment.dilution{border-color:var(--interface-accent);background:var(--interface-light);color:var(--interface-dark)}@media print{.print-adjustment.dilution{border-color:var(--interface-accent)!important;background:var(--interface-light)!important;color:var(--interface-dark)!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}}";
const PRINT_THEME_STYLE: &str = ".print-document{border:1px solid color-mix(in srgb,var(--interface-accent) 38%,#dce3ec)}.print-metadata>div{background:color-mix(in srgb,var(--interface-light) 22%,white);border-color:color-mix(in srgb,var(--interface-accent) 34%,#cbd5e1)}.print-metadata strong{color:var(--interface-dark)}.print-table th{background:color-mix(in srgb,var(--interface-light) 74%,white);color:var(--interface-dark)}.print-table th,.print-table td{border-color:color-mix(in srgb,var(--interface-accent) 42%,#98a2b3)}.print-table tfoot td{background:color-mix(in srgb,var(--interface-light) 54%,white);color:var(--interface-dark)}.print-table .print-volume-column{background:color-mix(in srgb,var(--interface-light) 82%,white);border-left-color:var(--interface-accent);border-right-color:var(--interface-accent);color:var(--interface-dark)}.print-adjustment,.print-adjustment.dilution,.print-adjustment.excess{border-color:var(--interface-accent);background:color-mix(in srgb,var(--interface-light) 76%,white);color:var(--interface-dark)}@media print{.print-metadata>div,.print-table th,.print-table tfoot td,.print-table .print-volume-column,.print-adjustment,.print-adjustment.dilution,.print-adjustment.excess{-webkit-print-color-adjust:exact;print-color-adjust:exact}.print-table th{background:color-mix(in srgb,var(--interface-light) 74%,white)!important;color:var(--interface-dark)!important}.print-table tfoot td{background:color-mix(in srgb,var(--interface-light) 54%,white)!important;color:var(--interface-dark)!important}.print-table .print-volume-column{background:color-mix(in srgb,var(--interface-light) 82%,white)!important;border-color:var(--interface-accent)!important;color:var(--interface-dark)!important}.print-adjustment,.print-adjustment.dilution,.print-adjustment.excess{background:color-mix(in srgb,var(--interface-light) 76%,white)!important;border-color:var(--interface-accent)!important;color:var(--interface-dark)!important}}";
const REPORT_TITLE_STYLE: &str = ".application-heading{display:flex;min-width:190px;max-width:310px;flex-direction:column;justify-content:center}.application-heading h1,.application-heading p{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.application-heading .active-report-title{margin:3px 0 0;color:var(--interface-dark);font-weight:700}";
const ACTUAL_FILL_STYLE: &str = ".actual-fill-section{min-width:980px;padding:18px 16px 22px;border-top:3px solid color-mix(in srgb,var(--interface-accent) 48%,#dce3ec);background:color-mix(in srgb,var(--interface-light) 16%,white)}.actual-fill-heading{display:flex;align-items:center;justify-content:space-between;gap:18px;margin-bottom:12px}.actual-fill-heading h2{color:var(--interface-dark)}.actual-fill-heading p{margin:3px 0 0;color:#68778f;font-size:calc(12px + var(--font-increase))}.actual-filling-time{display:flex;align-items:center;gap:9px;padding:7px 11px;border:1px solid color-mix(in srgb,var(--interface-accent) 42%,#cbd5e1);border-radius:8px;background:var(--interface-light);color:var(--interface-dark)}.actual-filling-time span{font-size:calc(11px + var(--font-increase));font-weight:600}.actual-filling-time strong{font-size:calc(16px + var(--font-increase));font-variant-numeric:tabular-nums}.actual-fill-table{width:100%;border-collapse:separate;border-spacing:0;table-layout:fixed;font-size:calc(13px + var(--font-increase));background:color-mix(in srgb,var(--interface-light) 8%,white);border:1px solid color-mix(in srgb,var(--interface-accent) 30%,#dce3ec);border-radius:9px;overflow:hidden}.actual-fill-table th,.actual-fill-table td{padding:8px;border-right:1px solid color-mix(in srgb,var(--interface-accent) 20%,#e5eaf0);border-bottom:1px solid color-mix(in srgb,var(--interface-accent) 20%,#e5eaf0);text-align:center;vertical-align:middle}.actual-fill-table th:last-child,.actual-fill-table td:last-child{border-right:0}.actual-fill-table tbody tr:last-child td{border-bottom:0}.actual-fill-table th{background:color-mix(in srgb,var(--interface-light) 72%,white);color:var(--interface-dark);font-size:calc(11px + var(--font-increase));text-transform:uppercase}.actual-fill-table th small{display:block;margin-top:3px;font-size:calc(10px + var(--font-increase));text-transform:none}.actual-fill-table th:nth-child(1){width:18%}.actual-fill-table th:nth-child(2){width:12%}.actual-fill-table th:nth-child(3){width:14%}.actual-fill-table th:nth-child(4){width:12%}.actual-fill-table th:nth-child(5){width:14%}.actual-fill-table th:nth-child(6){width:12%}.actual-fill-table th:nth-child(7){width:18%}.actual-fill-table input{height:calc(34px + var(--font-increase))!important;padding-top:5px!important;padding-bottom:5px!important}.actual-vial-name{text-align:left!important}.actual-vial-name strong,.actual-vial-name small{display:block}.actual-vial-name small{margin-top:3px;color:#68778f}.actual-vial-group td{background:color-mix(in srgb,var(--interface-light) 25%,white)}.actual-vial-first td{border-top:2px solid color-mix(in srgb,var(--interface-accent) 55%,#dce3ec)}.actual-vial-last td{border-bottom:2px solid color-mix(in srgb,var(--interface-accent) 55%,#dce3ec)}.actual-request-cell strong,.actual-request-cell small,.actual-request-cell span{display:block}.actual-request-cell strong{font-size:calc(15px + var(--font-increase));color:var(--interface-dark)}.actual-request-cell small{margin-top:3px;color:#68778f}.actual-request-cell span{margin-top:5px;color:var(--interface-dark);font-size:calc(10px + var(--font-increase));font-weight:700}.actual-result-badge{display:flex;flex-direction:column;align-items:center;gap:3px;padding:7px 9px;border-radius:8px;border:1px solid}.actual-result-badge.excess{background:#dcfce7;border-color:#86d7a8;color:#166534}.actual-result-badge.deficit{background:#fee2e2;border-color:#f3a6a6;color:#991b1b}.actual-result-badge span{font-variant-numeric:tabular-nums}.actual-comparison{display:block;margin-top:5px;color:#68778f;line-height:1.25}.actual-result-pending{display:inline-block;padding:6px 9px;border-radius:7px;background:#eef2f6;color:#68778f;font-weight:700}.actual-fill-pending-value{color:#98a2b3}.actual-fill-empty{padding:22px;border:1px dashed color-mix(in srgb,var(--interface-accent) 38%,#cbd5e1);border-radius:9px;text-align:center;color:#68778f}.actual-activity-input .field-unit{right:8px}.shell .actual-fill-table .time-menu{top:auto;bottom:100%;max-height:180px}";
const VERTICAL_PAGE_SCROLL_STYLE: &str = "html,body,#main{height:auto!important;min-height:100%;overflow-x:hidden!important;overflow-y:auto!important}.shell{height:auto!important;min-height:100vh!important;overflow:visible!important}.workspace{height:auto!important;min-height:calc(100vh - 72px)!important;overflow:visible!important;align-items:start!important}.sidebar{height:auto!important;align-self:start}.results.panel{height:auto!important;min-height:calc(100vh - 100px)!important;overflow:visible!important}.results-table-scroll{flex:none!important;min-height:0!important;max-height:none!important;overflow-x:auto!important;overflow-y:visible!important;border:0!important;border-radius:0!important}.integrated-results-table{border:1px solid color-mix(in srgb,var(--interface-accent) 28%,#dce3ec);border-radius:8px}.integrated-results-table tfoot td{position:static!important}.actual-fill-section{min-width:1240px!important;margin-top:20px;padding:0 0 22px!important;border:0!important;background:transparent!important}.actual-fill-heading{padding:0 4px}.actual-group-total td{background:color-mix(in srgb,var(--interface-light) 68%,white)!important;border-top:2px solid var(--interface-accent)!important}.actual-group-total td:first-child{text-align:right!important}.actual-group-total td:first-child strong,.actual-group-total td:first-child span{display:inline-block}.actual-group-total td:first-child span{margin-left:10px;color:#68778f}.manual-time-field input{font-variant-numeric:tabular-nums}";
const ACTUAL_FILL_COMPACT_STYLE: &str = ".actual-fill-table th{width:16.6667%!important}.actual-fill-table th,.actual-fill-table td{padding:5px 6px!important}.actual-fill-table input{height:30px!important;min-height:30px!important;padding-top:3px!important;padding-bottom:3px!important}.actual-fill-table .actual-vial-name{text-align:center!important}.actual-fill-table .actual-vial-name strong{font-size:calc(15px + var(--font-increase));line-height:1.2}.actual-fill-table .actual-result-badge{gap:1px;padding:4px 6px}.actual-fill-table .actual-comparison{margin-top:3px;font-size:calc(10px + var(--font-increase))}.actual-fill-table .actual-result-pending{padding:4px 7px}.actual-fill-table .actual-request-cell strong{font-size:calc(13px + var(--font-increase))}.actual-fill-table .actual-request-cell small{margin-top:1px}.actual-fill-table .time-field small{margin-top:2px}.integrated-results-table .empty-series-summary{background:color-mix(in srgb,var(--interface-light) 62%,white)!important;border-top-color:var(--interface-dark)!important}";
const ACTUAL_FILL_VIEW_TOGGLE_STYLE: &str = ".actual-fill-heading-actions{display:flex;align-items:center;justify-content:flex-end;gap:10px;flex-wrap:wrap}.actual-compact-toggle{white-space:nowrap}.actual-fill-compact-view th{width:25%!important}.actual-fill-compact-view th,.actual-fill-compact-view td{padding:2px 5px!important}.actual-fill-compact-view input{height:calc(26px + var(--font-increase))!important;min-height:calc(26px + var(--font-increase))!important;padding-top:1px!important;padding-bottom:1px!important}.actual-fill-compact-view .actual-vial-name{text-align:center!important}.actual-fill-compact-view .actual-result-badge{max-width:260px;margin:0 auto;padding:2px 6px;gap:0;line-height:1.15}.actual-fill-compact-view .actual-result-pending{padding:2px 6px}.actual-fill-compact-view .manual-time-field{max-width:180px;margin:0 auto}.actual-fill-compact-view .actual-activity-input{max-width:210px;margin:0 auto}.actual-at-request-badge{display:inline-block;min-width:64px;padding:4px 8px;border:1px solid;border-radius:999px;font-weight:700;font-variant-numeric:tabular-nums}.actual-at-request-badge.excess{background:#dcfce7;border-color:#86d7a8;color:#166534}.actual-at-request-badge.deficit{background:#fee2e2;border-color:#f3a6a6;color:#991b1b}@media(max-width:1100px){.actual-fill-heading{align-items:flex-start}.actual-fill-heading-actions{align-items:flex-end;flex-direction:column}}";
const ACTIVITY_CALCULATOR_STYLE: &str = ".actual-fill-content{display:grid;grid-template-columns:minmax(0,3fr) minmax(270px,1fr);align-items:start;gap:14px}.actual-fill-table-pane{min-width:0;overflow-x:auto}.actual-fill-table-pane .actual-fill-table{min-width:860px}.actual-fill-table-pane .actual-fill-compact-view{min-width:650px}.activity-calculator{padding:12px;border:1px solid color-mix(in srgb,var(--interface-accent) 34%,#cbd5e1);border-radius:10px;background:color-mix(in srgb,var(--interface-light) 28%,white);color:var(--interface-dark);box-shadow:0 2px 8px color-mix(in srgb,var(--interface-dark) 8%,transparent)}.calculator-mode-switch{display:grid;grid-template-columns:1fr 1fr;gap:4px;margin-bottom:10px;padding:3px;border-radius:8px;background:color-mix(in srgb,var(--interface-light) 75%,white)}.calculator-mode-switch button{min-height:30px;padding:4px 7px;border:0;border-radius:6px;background:transparent;color:var(--interface-dark);font-size:calc(11px + var(--font-increase));font-weight:700}.calculator-mode-switch button.active{background:var(--interface-accent);color:var(--interface-on-accent);box-shadow:0 1px 4px color-mix(in srgb,var(--interface-dark) 18%,transparent)}.standard-calculator output{display:flex;align-items:center;justify-content:flex-end;min-height:44px;margin-bottom:8px;padding:6px 10px;overflow:hidden;border:1px solid color-mix(in srgb,var(--interface-accent) 28%,#cbd5e1);border-radius:8px;background:white;color:var(--interface-dark);font-size:calc(20px + var(--font-increase));font-weight:700;font-variant-numeric:tabular-nums}.calculator-keypad{display:grid;grid-template-columns:repeat(4,1fr);gap:5px}.calculator-keypad button{min-width:0;min-height:34px;padding:5px;border:1px solid color-mix(in srgb,var(--interface-accent) 25%,#cbd5e1);border-radius:7px;background:white;color:var(--interface-dark);font-size:calc(14px + var(--font-increase));font-weight:700}.calculator-keypad button:hover{background:var(--interface-light)}.calculator-keypad button.operator{background:color-mix(in srgb,var(--interface-accent) 18%,white);color:var(--interface-dark)}.calculator-keypad button.calculator-zero{grid-column:span 2}.decay-calculator{display:grid;gap:10px}.calculator-isotope{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:6px 8px;border-radius:7px;background:var(--interface-light)}.calculator-isotope span{font-size:calc(10px + var(--font-increase));font-weight:600}.decay-calculator label>span{display:block;margin-bottom:4px;font-size:calc(11px + var(--font-increase));font-weight:700}.calculator-activity-field{display:grid;grid-template-columns:minmax(0,1fr) auto}.calculator-activity-field input{min-width:0;border-radius:7px 0 0 7px}.calculator-activity-field button{min-width:58px;padding:4px 7px;border:1px solid var(--interface-accent);border-radius:0 7px 7px 0;background:var(--interface-accent);color:var(--interface-on-accent);font-weight:700}.calculator-time-fields{display:grid;grid-template-columns:1fr 1fr;gap:7px}.activity-calculator input{height:calc(34px + var(--font-increase));font-size:calc(13px + var(--font-increase))}.activity-calculator .time-field small{display:none}.activity-calculator .time-menu{display:none}.decay-calculator-result{display:flex;min-height:68px;flex-direction:column;align-items:center;justify-content:center;padding:7px;border:1px solid color-mix(in srgb,var(--interface-accent) 40%,#cbd5e1);border-radius:8px;background:color-mix(in srgb,var(--interface-light) 58%,white);text-align:center}.decay-calculator-result>span{font-size:calc(10px + var(--font-increase));font-weight:700}.decay-calculator-result strong{margin-top:2px;font-size:calc(17px + var(--font-increase));font-variant-numeric:tabular-nums}.decay-calculator-result small{margin-top:2px;color:#68778f;font-weight:600}@media(max-width:1050px){.actual-fill-content{grid-template-columns:1fr}.activity-calculator{width:min(100%,420px)}}";
const ACTIVITY_CALCULATOR_FIX_STYLE: &str = ".actual-fill-content{grid-template-columns:minmax(0,1fr)}.actual-fill-content.with-calculator{grid-template-columns:minmax(0,3fr) minmax(270px,1fr)}.activity-calculator{contain:layout;width:100%;min-width:0;height:370px;min-height:370px;max-height:370px;overflow-x:hidden;overflow-y:auto;scrollbar-gutter:stable;align-self:start;box-sizing:border-box}.activity-calculator>*{width:100%;box-sizing:border-box}.calculator-toggle{display:grid;width:42px;height:42px;min-width:42px;padding:0;place-items:center;border:1px solid color-mix(in srgb,var(--interface-accent) 42%,#cbd5e1);border-radius:8px;background:var(--interface-light);color:var(--interface-dark)}.calculator-toggle:hover,.calculator-toggle.active{border-color:var(--interface-accent);background:var(--interface-accent);color:var(--interface-on-accent)}.calculator-toggle-icon{display:grid;width:25px;height:21px;place-items:center;border:2px solid currentColor;border-radius:4px;font-size:9px;font-weight:900;line-height:1}.calculator-mode-switch button{font-size:calc(13px + var(--font-increase))}.calculator-keypad button{font-size:calc(17px + var(--font-increase))}.standard-calculator .calculator-display{display:block;width:100%;height:44px!important;min-height:44px;margin-bottom:8px;padding:6px 10px!important;box-sizing:border-box;overflow:hidden;border:1px solid color-mix(in srgb,var(--interface-accent) 28%,#cbd5e1);border-radius:8px;background:white;color:var(--interface-dark);font-size:calc(20px + var(--font-increase));font-weight:700;text-align:right;font-variant-numeric:tabular-nums}.calculator-field-heading{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:4px}.calculator-field-heading>span{margin:0!important}.calculator-unit-switch{display:flex;gap:2px}.calculator-unit-switch button{min-height:23px;padding:2px 6px;border:1px solid color-mix(in srgb,var(--interface-accent) 34%,#cbd5e1);border-radius:5px;background:white;color:var(--interface-dark);font-size:calc(10px + var(--font-increase));font-weight:700}.calculator-unit-switch button.active{border-color:var(--interface-accent);background:var(--interface-accent);color:var(--interface-on-accent)}.calculator-activity-field{position:relative;display:block}.calculator-activity-field input{width:100%;min-width:0;padding-right:52px!important;border-radius:7px!important}.calculator-activity-field .field-unit{right:10px;pointer-events:none}@media(max-width:1050px){.actual-fill-content.with-calculator{grid-template-columns:1fr}.activity-calculator{width:min(100%,420px);justify-self:end}}";
const ACTIVITY_CALCULATOR_ANCHOR_STYLE: &str = ".actual-fill-content.with-calculator{position:relative;display:block;min-height:370px}.actual-fill-content.with-calculator .actual-fill-table-pane{width:calc(75% - 7px)}.actual-fill-content.with-calculator .activity-calculator{position:absolute;top:0;right:0;width:calc(25% - 7px);margin:0;overflow-y:scroll;scrollbar-width:none}.actual-fill-content.with-calculator .activity-calculator::-webkit-scrollbar{width:0;height:0}.calculator-live-result{display:flex;min-height:47px;align-items:flex-end;justify-content:space-between;gap:8px;margin:-3px 0 8px;padding:4px 8px;border-bottom:1px solid color-mix(in srgb,var(--interface-accent) 32%,#cbd5e1)}.calculator-live-result>span{padding-bottom:3px;color:#68778f;font-size:calc(10px + var(--font-increase));font-weight:700}.standard-calculator .calculator-live-result output{display:block;min-width:0;min-height:0;margin:0;padding:0;overflow:hidden;border:0;border-radius:0;background:transparent;color:var(--interface-dark);font-size:calc(21px + var(--font-increase));font-weight:800;text-align:right;font-variant-numeric:tabular-nums}@media(max-width:1050px){.actual-fill-content.with-calculator{display:grid;min-height:0}.actual-fill-content.with-calculator .actual-fill-table-pane{width:100%}.actual-fill-content.with-calculator .activity-calculator{position:static;width:min(100%,420px);margin:0;justify-self:end}}";
const CONSUMER_EDITOR_VISUAL_FIX_STYLE: &str = ".consumer-editor td:last-child{text-align:center}.consumer-editor td:last-child .remove,.integrated-results-table .consumer-name-cell .remove{display:grid;width:32px;height:32px;min-width:32px;min-height:32px;margin:auto;padding:0!important;place-items:center;border:1px solid color-mix(in srgb,#b42318 32%,#dce3ec);border-radius:8px;background:color-mix(in srgb,#fee2e2 42%,white);color:#a33a3a;font-size:calc(18px + var(--font-increase));line-height:1}.consumer-editor td:last-child .remove:hover,.integrated-results-table .consumer-name-cell .remove:hover{border-color:#b42318;background:#fee2e2;color:#991b1b}.vial-group-first td.request-group{position:relative}.vial-group-first .vial-group-label{z-index:100;top:-1px;transform:translate(-50%,-62%);border:1px solid color-mix(in srgb,var(--interface-accent) 58%,#dce3ec);background:color-mix(in srgb,var(--interface-accent) 24%,white);color:var(--interface-dark);box-shadow:0 1px 3px color-mix(in srgb,var(--interface-dark) 14%,transparent)}.vial-group-first .vial-original-activity{position:absolute;z-index:100;top:-1px;left:50%;display:inline-block;margin:0;padding:3px 9px;transform:translate(-50%,-62%);border-color:color-mix(in srgb,var(--interface-accent) 58%,#dce3ec);background:color-mix(in srgb,var(--interface-accent) 24%,white);color:var(--interface-dark);font-weight:500;line-height:1.1;box-shadow:0 1px 3px color-mix(in srgb,var(--interface-dark) 14%,transparent)}";
const CONSUMER_BADGE_FOCUS_STYLE: &str = ".vial-group-first .vial-group-label,.vial-group-first .vial-original-activity{transition:opacity .12s ease}.vial-group-first:has(.consumer-picker:focus-within) .vial-group-label,.vial-group-first:has(.consumer-picker:focus-within) .vial-original-activity{opacity:0;pointer-events:none}";
const UNIFIED_CLOSE_BUTTON_STYLE: &str = ".shell .close,.shell .remove,.shell .report-delete,.shell .toast-close,.shell .tab-close{display:grid;place-items:center;aspect-ratio:1;padding:0!important;border-radius:8px;line-height:1;text-align:center;font-family:Arial,sans-serif}.shell .close,.shell .remove,.shell .report-delete,.shell .toast-close{width:32px;height:32px;min-width:32px;min-height:32px}.shell .tab-close{width:26px;height:26px;min-width:26px;min-height:26px;border-radius:7px}.shell .close{border:1px solid color-mix(in srgb,#b42318 28%,#dce3ec);background:color-mix(in srgb,#fee2e2 35%,transparent);color:#a33a3a}.shell .close:hover{border-color:#b42318;background:#fee2e2;color:#991b1b}.shell .toast-close{border:1px solid color-mix(in srgb,white 42%,transparent);background:color-mix(in srgb,white 10%,transparent);color:white}.shell .toast-close:hover{background:color-mix(in srgb,white 22%,transparent);color:white}.shell .report-delete{border:1px solid color-mix(in srgb,#b42318 32%,#dce3ec);background:color-mix(in srgb,#fee2e2 42%,white);color:#a33a3a}.shell .tab-close{border:1px solid transparent}.shell .tab-close:hover{border-color:#dc2626;background:#dc2626!important;color:white!important}";
const CONSUMER_DROPDOWN_OVERFLOW_STYLE: &str = ".results:has(.integrated-results-table .consumer-picker:focus-within),.results-table-scroll:has(.consumer-picker:focus-within){overflow:visible!important}.results-table-scroll:has(.consumer-picker:focus-within){border-color:transparent!important}.integrated-results-table:has(.consumer-picker:focus-within){position:relative;z-index:1000000}.integrated-results-table .consumer-menu{contain:layout paint}";
const CONSUMER_INTERACTION_FIX_STYLE: &str = ".results:has(.integrated-results-table .time-field:focus-within),.results-table-scroll:has(.time-field:focus-within){overflow:visible!important}.results-table-scroll:has(.time-field:focus-within){border-color:transparent!important}.integrated-results-table:has(.time-field:focus-within){position:relative;z-index:1000000}.integrated-results-table .time-menu{contain:layout paint;z-index:1000001}.integrated-results-table tbody td{border-bottom-width:2px!important}.vial-group-first:focus-within .vial-group-label,.vial-group-first:focus-within .vial-original-activity,tr:focus-within+.vial-group-first .vial-group-label,tr:focus-within+.vial-group-first .vial-original-activity{opacity:0;pointer-events:none}.shell .close,.shell .remove,.shell .report-delete,.shell .toast-close,.shell .tab-close{font-size:0!important}.shell .close::before,.shell .remove::before,.shell .report-delete::before,.shell .toast-close::before,.shell .tab-close::before{content:'×';display:block;font-family:Arial,sans-serif;font-size:20px;font-weight:400;line-height:1;transform:translateY(-1px)}.shell .tab-close::before{font-size:18px}.shell .close,.shell .remove,.shell .report-delete,.shell .toast-close{width:32px!important;height:32px!important;min-width:32px!important;min-height:32px!important;border-radius:8px!important}.shell .tab-close{width:28px!important;height:28px!important;min-width:28px!important;min-height:28px!important;margin:2px;border-radius:8px!important}";
const CROSS_BUTTON_FINAL_STYLE: &str = ".close,.remove,.report-delete,.toast-close,.tab-close{display:flex!important;width:32px!important;height:32px!important;min-width:32px!important;min-height:32px!important;align-items:center!important;justify-content:center!important;aspect-ratio:1;padding:0!important;border:1px solid color-mix(in srgb,#b42318 32%,#dce3ec)!important;border-radius:8px!important;background:color-mix(in srgb,#fee2e2 42%,white)!important;color:#a33a3a!important;font-family:Arial,sans-serif!important;font-size:18px!important;font-weight:400!important;line-height:1!important;text-align:center!important;transform:none!important}.close::before,.remove::before,.report-delete::before,.toast-close::before,.tab-close::before{content:none!important;display:none!important}.close:hover,.remove:hover,.report-delete:hover,.toast-close:hover,.tab-close:hover{border-color:#b42318!important;background:#fee2e2!important;color:#991b1b!important}.tab-close{margin:2px!important}.calculation-tab.active .tab-close{border-color:color-mix(in srgb,white 46%,transparent)!important;background:color-mix(in srgb,white 14%,transparent)!important;color:var(--interface-on-accent)!important}.calculation-tab.active .tab-close:hover{border-color:#dc2626!important;background:#dc2626!important;color:white!important}.info-toast .toast-close{border-color:color-mix(in srgb,white 42%,transparent)!important;background:color-mix(in srgb,white 10%,transparent)!important;color:white!important}.info-toast .toast-close:hover{background:color-mix(in srgb,white 22%,transparent)!important;color:white!important}";
const CROSS_AND_FOCUS_CORRECTION_STYLE: &str = ".shell .integrated-results-table .remove,.shell .calculation-tab .tab-close{display:flex!important;align-items:center!important;justify-content:center!important;font-family:Arial,sans-serif!important;font-size:18px!important;font-weight:400!important;line-height:1!important;color:#a33a3a!important}.shell .integrated-results-table .remove::before,.shell .calculation-tab .tab-close::before{content:none!important;display:none!important}.shell .calculation-tab.active .tab-close,.shell .calculation-tab:not(.active) .tab-close{border-color:color-mix(in srgb,#b42318 32%,#dce3ec)!important;background:color-mix(in srgb,#fee2e2 76%,white)!important;color:#a33a3a!important}.shell .calculation-tab .tab-close:hover{border-color:#dc2626!important;background:#dc2626!important;color:white!important}.shell .results-table-scroll:has(.consumer-picker:focus-within),.shell .results-table-scroll:has(.time-field:focus-within){border:0!important;border-radius:0!important}";
const STABLE_CONSUMER_TABLE_INTERACTION_STYLE: &str = ".shell .results:has(.integrated-results-table .consumer-picker:focus-within),.shell .results:has(.integrated-results-table .time-field:focus-within){overflow-x:auto!important;overflow-y:visible!important}.shell .results-table-scroll:has(.consumer-picker:focus-within),.shell .results-table-scroll:has(.time-field:focus-within){overflow-x:auto!important;overflow-y:visible!important;border:0!important;border-radius:0!important}.integrated-results-table{border-collapse:separate!important;border-spacing:0!important}.integrated-results-table .consumer-picker:focus-within,.integrated-results-table .time-field:focus-within{anchor-name:--active-consumer-field}.integrated-results-table .consumer-menu,.integrated-results-table .time-menu{position:fixed!important;position-anchor:--active-consumer-field;top:anchor(bottom)!important;right:auto!important;bottom:auto!important;left:anchor(left)!important;width:anchor-size(width);min-width:180px;margin-top:5px;position-try-fallbacks:flip-block;z-index:2147483000!important}.integrated-results-table .consumer-menu{min-width:250px}";
const TABLE_TAB_AND_EMPTY_STATE_FIX_STYLE: &str = ".integrated-results-table{border-collapse:collapse!important;border-spacing:0!important}.integrated-results-table tbody td{border-bottom-width:1px!important}.integrated-results-table .request-group{border-top-width:2px!important;border-bottom-width:2px!important}.integrated-results-table td:has(.consumer-picker:focus-within),.integrated-results-table tr:has(.consumer-picker:focus-within),.integrated-results-table td:has(.time-field:focus-within),.integrated-results-table tr:has(.time-field:focus-within){position:static!important;z-index:auto!important}.calculation-tab,.calculation-tab.active,.calculation-tab:not(.active){font-weight:700!important;line-height:1.2!important}.calculation-tab .tab-title{min-height:32px;display:flex;align-items:center;font-weight:700!important;line-height:1.2!important;letter-spacing:0!important}.calculation-tab .tab-edit,.calculation-tab .tab-close{flex:0 0 32px;width:32px!important;min-width:32px!important;height:32px!important;min-height:32px!important;margin:0!important}.actual-fill-content:not(.with-calculator),.actual-fill-content:not(.with-calculator) .actual-fill-table-pane{display:block;width:100%!important}.actual-fill-empty{display:flex;width:100%;min-height:82px;align-items:center;justify-content:center;border:2px dashed color-mix(in srgb,var(--interface-accent) 38%,#cbd5e1);background:color-mix(in srgb,var(--interface-light) 16%,white)}";
const TAB_TOOLS_AND_ACTUAL_WIDTH_STYLE: &str = ".shell .calculation-tab .tab-close{font-size:14px!important}.shell .calculation-tab .tab-edit{display:flex!important;flex:0 0 32px;width:32px!important;height:32px!important;min-width:32px!important;min-height:32px!important;align-items:center!important;justify-content:center!important;margin:0!important;padding:0!important;border:1px solid color-mix(in srgb,var(--interface-accent) 30%,#dce3ec)!important;border-radius:8px!important;background:color-mix(in srgb,var(--interface-light) 66%,white)!important;color:var(--interface-dark)!important;font-family:'Segoe UI Symbol','Segoe UI',sans-serif!important;font-size:15px!important;font-weight:500!important;line-height:1!important}.shell .calculation-tab .tab-edit:hover{border-color:var(--interface-accent)!important;background:var(--interface-light)!important;color:var(--interface-dark)!important}.actual-fill-section,.actual-fill-empty{width:100%!important;max-width:none!important;box-sizing:border-box}.actual-fill-content:not(.with-calculator),.actual-fill-content:not(.with-calculator) .actual-fill-table-pane{display:block!important;width:100%!important;max-width:none!important;box-sizing:border-box}.actual-fill-content.with-calculator .actual-fill-table-pane{width:calc(75% - 7px)!important}.actual-fill-content.with-calculator .activity-calculator{width:calc(25% - 7px)!important}@media(max-width:1050px){.actual-fill-content.with-calculator .actual-fill-table-pane{width:100%!important}.actual-fill-content.with-calculator .activity-calculator{width:min(100%,420px)!important}}";
const COMPACT_TAB_CONTROLS_STYLE: &str = ".shell .calculation-tab .tab-edit,.shell .calculation-tab .tab-close{display:flex!important;flex:0 0 22px!important;width:22px!important;height:22px!important;min-width:22px!important;min-height:22px!important;align-items:center!important;justify-content:center!important;margin:0 2px 0 0!important;padding:0!important;border-radius:6px!important;line-height:1!important}.shell .calculation-tab .tab-close{border:1px solid color-mix(in srgb,#b42318 36%,#dce3ec)!important;background:color-mix(in srgb,#fee2e2 66%,white)!important;color:#a33a3a!important;font-size:12px!important}.shell .calculation-tab .tab-edit{border:1px solid var(--interface-dark)!important;background:var(--interface-dark)!important;color:white!important;font-family:'Segoe UI Symbol','Segoe UI',sans-serif!important;font-size:13px!important;font-weight:500!important}.shell .calculation-tab .tab-edit:hover{border-color:var(--interface-accent)!important;background:var(--interface-accent)!important;color:var(--interface-on-accent)!important}.shell .calculation-tab .tab-close:hover{border-color:#dc2626!important;background:#dc2626!important;color:white!important}.shell .calculation-tab.active .tab-edit{border-color:color-mix(in srgb,white 50%,var(--interface-dark))!important;background:color-mix(in srgb,var(--interface-dark) 78%,black)!important;color:white!important}.shell .calculation-tab.active .tab-close{border-color:color-mix(in srgb,#fee2e2 58%,white)!important;background:color-mix(in srgb,#fee2e2 82%,white)!important;color:#991b1b!important}";
