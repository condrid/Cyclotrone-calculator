pub(crate) const F18_HALF_LIFE_MINUTES: f64 = 109.77;
pub(crate) const SCIENTIFIC_FORMAT_THRESHOLD: f64 = 10_000.0;
pub(crate) const VIAL_FILL_LIMIT_RATIO: f64 = 14.5 / 15.0;

#[derive(Debug, PartialEq)]
pub(crate) enum IrradiationTimeError {
    InvalidInput,
    UnreachableActivity,
}

pub(crate) fn irradiation_time_minutes(
    eob_activity: f64,
    target_constant: f64,
    total_current_microamps: f64,
    half_life_minutes: f64,
) -> Result<f64, IrradiationTimeError> {
    if eob_activity.is_nan()
        || !target_constant.is_finite()
        || !total_current_microamps.is_finite()
        || eob_activity < 0.0
        || target_constant <= 0.0
        || total_current_microamps <= 0.0
        || !half_life_minutes.is_finite()
        || half_life_minutes <= 0.0
    {
        return Err(IrradiationTimeError::InvalidInput);
    }
    if eob_activity.is_infinite() {
        return Err(IrradiationTimeError::UnreachableActivity);
    }
    if eob_activity == 0.0 {
        return Ok(0.0);
    }

    let saturation_activity = target_constant * total_current_microamps;
    if eob_activity >= saturation_activity {
        return Err(IrradiationTimeError::UnreachableActivity);
    }

    let decay_constant = std::f64::consts::LN_2 / half_life_minutes;
    Ok(-(1.0 - eob_activity / saturation_activity).ln() / decay_constant)
}

pub(crate) fn cyclotron_unloading_minutes(target_count: &str) -> Option<i32> {
    match target_count {
        "1" => Some(5),
        "2" => Some(11),
        _ => None,
    }
}

pub(crate) fn is_valid_time(value: &str) -> bool {
    let mut parts = value.split(':');
    let hours = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minutes = parts.next().and_then(|part| part.parse::<u32>().ok());

    hours.is_some_and(|value| value < 24)
        && minutes.is_some_and(|value| value < 60)
        && parts.next().is_none()
        && value.len() == 5
}

pub(crate) fn format_time_input(value: &str) -> String {
    let digits = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .take(4)
        .collect::<String>();

    if digits.len() <= 2 {
        digits
    } else {
        format!("{}:{}", &digits[..2], &digits[2..])
    }
}

fn minutes_from_midnight(value: &str) -> Option<i32> {
    if !is_valid_time(value) {
        return None;
    }
    let (hours, minutes) = value.split_once(':')?;
    Some(hours.parse::<i32>().ok()? * 60 + minutes.parse::<i32>().ok()?)
}

pub(crate) fn time_before_synthesis(
    filling_start: &str,
    synthesis_time: &str,
    activity_transfer_time: &str,
) -> Option<String> {
    let total_duration =
        parse_non_negative(synthesis_time)? + parse_non_negative(activity_transfer_time)?;
    time_before(filling_start, &total_duration.to_string())
}

pub(crate) fn time_before(reference_time: &str, offset_minutes: &str) -> Option<String> {
    let reference_minutes = minutes_from_midnight(reference_time)?;
    let offset = parse_non_negative(offset_minutes)?;
    let result = (reference_minutes - offset.round() as i32).rem_euclid(24 * 60);
    Some(format!("{:02}:{:02}", result / 60, result % 60))
}

pub(crate) fn parse_decimal(value: &str) -> Option<f64> {
    value.trim().replace(',', ".").parse::<f64>().ok()
}

fn parse_non_negative(value: &str) -> Option<f64> {
    let value = parse_decimal(value)?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

pub(crate) fn format_activity(value: &str) -> Option<String> {
    parse_decimal(value).map(format_activity_value)
}

pub(crate) fn format_activity_value(value: f64) -> String {
    format!("{value:.2}").replace('.', ",")
}

pub(crate) fn format_volume_value(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

pub(crate) fn format_adaptive_value(value: f64) -> String {
    if value == f64::INFINITY {
        return "∞".into();
    }
    if value == f64::NEG_INFINITY {
        return "−∞".into();
    }
    if value.abs() >= SCIENTIFIC_FORMAT_THRESHOLD {
        return format!("{value:.2e}").replace('.', ",");
    }
    format_activity_value(value)
}

pub(crate) fn is_extreme_value(value: f64) -> bool {
    !value.is_finite() || value.abs() >= SCIENTIFIC_FORMAT_THRESHOLD
}

pub(crate) fn compensate_radiochemical_yield(
    activity: f64,
    radiochemical_yield_percent: &str,
) -> Option<f64> {
    let yield_percent = parse_decimal(radiochemical_yield_percent)?;
    if !yield_percent.is_finite() || yield_percent <= 0.0 || yield_percent > 100.0 {
        return None;
    }
    Some(activity / (yield_percent / 100.0))
}

pub(crate) fn calculate_filling_volume(activity: f64, volumetric_activity: &str) -> Option<f64> {
    let activity_per_ml = parse_decimal(volumetric_activity)?;
    if !activity_per_ml.is_finite() || activity_per_ml <= 0.0 {
        return None;
    }
    Some(activity / activity_per_ml)
}

pub(crate) fn split_requested_activity_into_vials(
    requested_activity: &str,
    requested_time: &str,
    filling_start: &str,
    half_life_minutes: f64,
    volumetric_activity: &str,
    maximum_vial_volume: &str,
) -> Option<Vec<f64>> {
    let requested_activity = parse_non_negative(requested_activity)?;
    let volumetric_activity = parse_decimal(volumetric_activity)?;
    let maximum_vial_volume = parse_decimal(maximum_vial_volume)?;
    if requested_activity <= 0.0
        || !volumetric_activity.is_finite()
        || volumetric_activity <= 0.0
        || !maximum_vial_volume.is_finite()
        || maximum_vial_volume <= 0.0
    {
        return None;
    }

    let filling_activity = activity_at_reference_time(
        &requested_activity.to_string(),
        requested_time,
        filling_start,
        half_life_minutes,
    )?
    .0;
    let filling_volume = filling_activity / volumetric_activity;
    let allowed_volume = maximum_vial_volume * VIAL_FILL_LIMIT_RATIO;
    if filling_volume <= allowed_volume {
        return None;
    }

    let decay_factor = filling_activity / requested_activity;
    let safe_requested_activity =
        ((allowed_volume * volumetric_activity / decay_factor) * 10.0).floor() / 10.0;
    if !safe_requested_activity.is_finite() || safe_requested_activity <= 0.0 {
        return None;
    }

    let vial_count = (requested_activity / safe_requested_activity).ceil() as usize;
    if !(2..=10_000).contains(&vial_count) {
        return None;
    }
    let mut remaining = requested_activity;
    let mut activities = Vec::with_capacity(vial_count);
    for index in 0..vial_count {
        if index + 1 == vial_count {
            activities.push((remaining * 10.0).ceil() / 10.0);
        } else {
            activities.push(safe_requested_activity);
            remaining = (remaining - safe_requested_activity).max(0.0);
        }
    }
    activities.sort_by(f64::total_cmp);
    Some(activities)
}

pub(crate) fn calculate_series_volume_adjustment(
    requested_volume: f64,
    semi_product_volume: &str,
) -> Option<(f64, f64, bool)> {
    let semi_product_volume = parse_decimal(semi_product_volume)?;
    if !requested_volume.is_finite()
        || requested_volume < 0.0
        || !semi_product_volume.is_finite()
        || semi_product_volume < 0.0
    {
        return None;
    }

    let has_excess = requested_volume < semi_product_volume;
    Some((
        requested_volume.max(semi_product_volume),
        (requested_volume - semi_product_volume).abs(),
        has_excess,
    ))
}

pub(crate) fn activity_from_sampling_volume(
    volume_ml: &str,
    volumetric_activity: &str,
) -> Option<f64> {
    let volume = parse_non_negative(volume_ml)?;
    let activity_per_ml = parse_decimal(volumetric_activity)?;
    if !activity_per_ml.is_finite() || activity_per_ml <= 0.0 {
        return None;
    }
    Some(volume * activity_per_ml)
}

pub(crate) fn activity_at_reference_time(
    activity: &str,
    requested_time: &str,
    reference_time: &str,
    half_life_minutes: f64,
) -> Option<(f64, i32)> {
    if !half_life_minutes.is_finite() || half_life_minutes <= 0.0 {
        return None;
    }
    let requested_activity = parse_non_negative(activity)?;
    let elapsed_minutes = (minutes_from_midnight(requested_time)?
        - minutes_from_midnight(reference_time)?)
    .rem_euclid(24 * 60);
    let activity_at_reference =
        requested_activity * 2_f64.powf(elapsed_minutes as f64 / half_life_minutes);
    Some((activity_at_reference, elapsed_minutes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recalculates_requested_activity_back_to_reference_time() {
        let (activity, elapsed) =
            activity_at_reference_time("100", "07:49", "06:00", F18_HALF_LIFE_MINUTES)
                .expect("valid input");
        assert_eq!(elapsed, 109);
        assert!((activity - 199.03).abs() < 0.01);
    }

    #[test]
    fn calculates_process_times_across_midnight() {
        assert_eq!(
            time_before_synthesis("00:15", "20", "10"),
            Some("23:45".into())
        );
        assert_eq!(time_before("00:15", "30"), Some("23:45".into()));
    }

    #[test]
    fn calculates_activity_adjustments_and_volume() {
        assert_eq!(compensate_radiochemical_yield(185.0, "92,5"), Some(200.0));
        assert_eq!(calculate_filling_volume(185.0, "92,5"), Some(2.0));
        assert_eq!(activity_from_sampling_volume("1,5", "70"), Some(105.0));
        assert_eq!(format_volume_value(2.04), "2,0");
        assert_eq!(format_volume_value(2.06), "2,1");
        assert_eq!(format_adaptive_value(12_345.0), "1,23e4");
        assert_eq!(format_adaptive_value(f64::INFINITY), "∞");
    }

    #[test]
    fn rejects_invalid_numeric_constants() {
        assert_eq!(compensate_radiochemical_yield(200.0, "101"), None);
        assert_eq!(compensate_radiochemical_yield(200.0, "0"), None);
        assert_eq!(calculate_filling_volume(100.0, "0"), None);
        assert_eq!(activity_from_sampling_volume("-1", "70"), None);
    }

    #[test]
    fn treats_unused_semi_product_as_excess() {
        assert_eq!(
            calculate_series_volume_adjustment(18.0, "22"),
            Some((22.0, 4.0, true))
        );
        assert_eq!(
            calculate_series_volume_adjustment(30.0, "22"),
            Some((30.0, 8.0, false))
        );
    }

    #[test]
    fn splits_large_request_into_safe_editable_vials() {
        let activities =
            split_requested_activity_into_vials("200", "06:30", "06:30", 109.77, "6", "15")
                .expect("split required");

        assert_eq!(activities, vec![26.0, 87.0, 87.0]);
        assert!(activities.iter().sum::<f64>() >= 200.0);
        assert!(activities.iter().all(|activity| *activity <= 87.0));

        let rounded_up =
            split_requested_activity_into_vials("200,01", "06:30", "06:30", 109.77, "6", "15")
                .expect("split with fractional remainder");
        assert_eq!(rounded_up, vec![26.1, 87.0, 87.0]);
    }

    #[test]
    fn does_not_split_request_below_vial_threshold() {
        assert_eq!(
            split_requested_activity_into_vials("80", "06:30", "06:30", 109.77, "6", "15"),
            None
        );
        assert_eq!(
            split_requested_activity_into_vials("87", "06:30", "06:30", 109.77, "6", "15"),
            None
        );
    }

    #[test]
    fn chooses_unloading_time_by_target_count() {
        assert_eq!(cyclotron_unloading_minutes("1"), Some(5));
        assert_eq!(cyclotron_unloading_minutes("2"), Some(11));
        assert_eq!(cyclotron_unloading_minutes("3"), None);
    }

    #[test]
    fn decay_is_applied_before_yield_compensation() {
        let activity_at_filling =
            activity_at_reference_time("8", "07:30", "06:30", F18_HALF_LIFE_MINUTES)
                .expect("valid activity")
                .0;
        let required_activity =
            compensate_radiochemical_yield(activity_at_filling, "70").expect("valid yield");

        assert!((activity_at_filling - 11.68).abs() < 0.01);
        assert!((required_activity - 16.69).abs() < 0.01);
    }

    #[test]
    fn calculates_irradiation_time_from_eob_activity() {
        let time = irradiation_time_minutes(164.0, 8.0, 65.0, F18_HALF_LIFE_MINUTES)
            .expect("reachable activity");
        assert!((time - 60.00).abs() < 0.01);

        let faster = irradiation_time_minutes(164.0, 8.0, 130.0, F18_HALF_LIFE_MINUTES)
            .expect("two targets");
        assert!(faster < time);
        assert_eq!(
            irradiation_time_minutes(520.0, 8.0, 65.0, F18_HALF_LIFE_MINUTES),
            Err(IrradiationTimeError::UnreachableActivity)
        );
        assert_eq!(
            irradiation_time_minutes(f64::INFINITY, 8.0, 65.0, F18_HALF_LIFE_MINUTES),
            Err(IrradiationTimeError::UnreachableActivity)
        );
    }

    #[test]
    fn uses_selected_isotope_half_life_for_decay() {
        let (activity, elapsed) =
            activity_at_reference_time("100", "08:00", "07:39", 21.0).expect("valid input");
        assert_eq!(elapsed, 21);
        assert!((activity - 200.0).abs() < 0.001);
    }
}
