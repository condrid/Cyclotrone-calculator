use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq)]
pub(crate) struct DrugListItem {
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(Clone, PartialEq)]
pub(crate) struct Isotope {
    pub(crate) id: i64,
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) half_life_minutes: String,
}

#[derive(Clone, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct Consumer {
    pub(crate) name: String,
    pub(crate) activity: String,
    pub(crate) requested_time: String,
    pub(crate) is_mandatory: bool,
    pub(crate) split_into_vials: bool,
    pub(crate) split_applied: bool,
    pub(crate) vial_group_id: Option<u64>,
    pub(crate) vial_group_source_name: Option<String>,
    pub(crate) vial_group_original_activity: Option<String>,
}

impl Consumer {
    pub(crate) fn new(split_into_vials: bool) -> Self {
        Self {
            split_into_vials,
            ..Self::default()
        }
    }

    pub(crate) fn sampling() -> Self {
        Self {
            name: "Отбор проб".into(),
            activity: String::new(),
            requested_time: String::new(),
            is_mandatory: true,
            split_into_vials: false,
            split_applied: false,
            vial_group_id: None,
            vial_group_source_name: None,
            vial_group_original_activity: None,
        }
    }

    pub(crate) fn line_flush() -> Self {
        Self {
            name: "Промывка линий".into(),
            activity: "3,00".into(),
            requested_time: String::new(),
            is_mandatory: true,
            split_into_vials: false,
            split_applied: false,
            vial_group_id: None,
            vial_group_source_name: None,
            vial_group_original_activity: None,
        }
    }

    pub(crate) fn is_sampling(&self) -> bool {
        self.is_mandatory && self.name == "Отбор проб"
    }

    pub(crate) fn is_line_flush(&self) -> bool {
        self.is_mandatory && self.name == "Промывка линий"
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct DrugProfile {
    pub(crate) isotope_id: Option<i64>,
    pub(crate) radiochemical_yield: String,
    pub(crate) maximum_vial_volume: String,
    pub(crate) semi_product_volume: String,
    #[serde(rename = "synthesis_time_minutes")]
    pub(crate) synthesis_time: String,
    #[serde(rename = "activity_transfer_time_minutes")]
    pub(crate) activity_transfer_time: String,
}

impl Default for DrugProfile {
    fn default() -> Self {
        Self {
            isotope_id: None,
            radiochemical_yield: "95".into(),
            maximum_vial_volume: String::new(),
            semi_product_volume: "22".into(),
            synthesis_time: "0".into(),
            activity_transfer_time: "0".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_consumer_has_a_fixed_identity() {
        let consumer = Consumer::sampling();

        assert_eq!(consumer.name, "Отбор проб");
        assert!(consumer.is_mandatory);
    }

    #[test]
    fn line_flush_has_default_editable_volume() {
        let consumer = Consumer::line_flush();

        assert_eq!(consumer.name, "Промывка линий");
        assert_eq!(consumer.activity, "3,00");
        assert!(consumer.is_line_flush());
    }

    #[test]
    fn empty_saved_profile_uses_defaults() {
        let profile: DrugProfile = serde_json::from_str("{}").expect("valid default profile");

        assert_eq!(profile.radiochemical_yield, "95");
        assert_eq!(profile.maximum_vial_volume, "");
        assert_eq!(profile.semi_product_volume, "22");
        assert_eq!(profile.synthesis_time, "0");
        assert_eq!(profile.activity_transfer_time, "0");
    }

    #[test]
    fn legacy_consumer_payload_does_not_enable_vial_splitting() {
        let consumer: Consumer = serde_json::from_str(
            r#"{"name":"Подольск","activity":"50","requested_time":"07:30","is_mandatory":false}"#,
        )
        .expect("legacy consumer");

        assert!(!consumer.split_into_vials);
        assert!(!consumer.split_applied);
        assert_eq!(consumer.vial_group_id, None);
    }

    #[test]
    fn new_consumer_captures_split_mode() {
        let consumer = Consumer::new(true);

        assert!(consumer.split_into_vials);
        assert!(!consumer.split_applied);
    }
}
