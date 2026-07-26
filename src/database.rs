use rusqlite::{params, types::Type, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::calculations::F18_HALF_LIFE_MINUTES;
use crate::models::{Consumer, DrugListItem, DrugProfile, Isotope};
use crate::storage::database_path;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct CalculationSettings {
    pub(crate) target_count: String,
    pub(crate) target_constant: String,
    pub(crate) target_current_1_microamps: String,
    pub(crate) target_current_2_microamps: String,
    pub(crate) isotope_id: Option<i64>,
    pub(crate) isotope_name: String,
    pub(crate) volumetric_activity_gbq_per_ml: String,
    pub(crate) filling_start: String,
    pub(crate) half_life_minutes: f64,
    pub(crate) radiochemical_yield: String,
    pub(crate) maximum_vial_volume_ml: String,
    pub(crate) semi_product_volume_ml: String,
    pub(crate) synthesis_time_minutes: String,
    pub(crate) activity_transfer_time_minutes: String,
    pub(crate) before_synthesis: String,
    pub(crate) cyclotron_offset_minutes: String,
    pub(crate) cyclotron_time: String,
}

impl Default for CalculationSettings {
    fn default() -> Self {
        Self::new(
            "2",
            "8",
            "65",
            "65",
            None,
            "F-18",
            F18_HALF_LIFE_MINUTES,
            "6",
            "04:30",
            "95",
            "",
            "22",
            "0",
            "0",
            "—",
            "11",
            "—",
        )
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct SavedCalculationSummary {
    pub(crate) id: i64,
    pub(crate) calculated_at: String,
    pub(crate) drug_name: String,
    pub(crate) report_title: String,
    pub(crate) consumer_count: usize,
}

pub(crate) struct SavedCalculation {
    pub(crate) drug_id: Option<i64>,
    pub(crate) drug_name: String,
    pub(crate) report_name: Option<String>,
    pub(crate) consumers: Vec<Consumer>,
    pub(crate) settings: CalculationSettings,
}

impl CalculationSettings {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        target_count: &str,
        target_constant: &str,
        target_current_1_microamps: &str,
        target_current_2_microamps: &str,
        isotope_id: Option<i64>,
        isotope_name: &str,
        half_life_minutes: f64,
        volumetric_activity_gbq_per_ml: &str,
        filling_start: &str,
        radiochemical_yield: &str,
        maximum_vial_volume_ml: &str,
        semi_product_volume_ml: &str,
        synthesis_time_minutes: &str,
        activity_transfer_time_minutes: &str,
        before_synthesis: &str,
        cyclotron_offset_minutes: &str,
        cyclotron_time: &str,
    ) -> Self {
        Self {
            target_count: target_count.into(),
            target_constant: target_constant.into(),
            target_current_1_microamps: target_current_1_microamps.into(),
            target_current_2_microamps: target_current_2_microamps.into(),
            isotope_id,
            isotope_name: isotope_name.into(),
            volumetric_activity_gbq_per_ml: volumetric_activity_gbq_per_ml.into(),
            filling_start: filling_start.into(),
            half_life_minutes,
            radiochemical_yield: radiochemical_yield.into(),
            maximum_vial_volume_ml: maximum_vial_volume_ml.into(),
            semi_product_volume_ml: semi_product_volume_ml.into(),
            synthesis_time_minutes: synthesis_time_minutes.into(),
            activity_transfer_time_minutes: activity_transfer_time_minutes.into(),
            before_synthesis: before_synthesis.into(),
            cyclotron_offset_minutes: cyclotron_offset_minutes.into(),
            cyclotron_time: cyclotron_time.into(),
        }
    }
}

pub(crate) fn initialize_database() -> rusqlite::Result<()> {
    let path = database_path()?;
    let is_new_database = !path.exists();
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    seed_isotopes(&connection)?;
    if is_new_database {
        for (code, name) in [("fdg", "F-18 FDG"), ("tc99m", "Tc-99m")] {
            connection.execute(
                "INSERT INTO drug_types (code, name, settings_json) VALUES (?1, ?2, ?3)",
                params![code, name, "{}"],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn load_interface_color() -> rusqlite::Result<String> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    Ok(connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'interface_color'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "#3974d8".into()))
}

pub(crate) fn save_interface_color(color: &str) -> rusqlite::Result<()> {
    let color = color.trim();
    if color.len() != 7
        || !color.starts_with('#')
        || !color[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    connection.execute(
        "INSERT INTO app_settings (key, value) VALUES ('interface_color', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [color],
    )?;
    Ok(())
}

pub(crate) fn load_interface_font_step() -> rusqlite::Result<u8> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let value = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'interface_font_step'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "0".into());
    Ok(value.parse::<u8>().unwrap_or(0).min(4))
}

pub(crate) fn save_interface_font_step(step: u8) -> rusqlite::Result<()> {
    if step > 4 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    connection.execute(
        "INSERT INTO app_settings (key, value) VALUES ('interface_font_step', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [step.to_string()],
    )?;
    Ok(())
}

pub(crate) fn load_isotopes() -> rusqlite::Result<Vec<Isotope>> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    seed_isotopes(&connection)?;
    let mut statement =
        connection.prepare("SELECT id, code, name, half_life_minutes FROM isotopes ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok(Isotope {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
            // HTML input[type=number] accepts a dot as the decimal separator in
            // its value attribute. A comma makes decimal values appear empty.
            half_life_minutes: row.get::<_, f64>(3)?.to_string(),
        })
    })?;
    rows.collect()
}

pub(crate) fn save_isotope(isotope: &Isotope) -> rusqlite::Result<i64> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let half_life = isotope
        .half_life_minutes
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    if !half_life.is_finite() || half_life <= 0.0 || isotope.name.trim().is_empty() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if isotope.id > 0 {
        connection.execute(
            "UPDATE isotopes SET name = ?2, half_life_minutes = ?3 WHERE id = ?1",
            params![isotope.id, isotope.name.trim(), half_life],
        )?;
        Ok(isotope.id)
    } else {
        let code = format!("custom:{}", isotope.name.trim().to_lowercase());
        connection.execute(
            "INSERT INTO isotopes (code, name, half_life_minutes) VALUES (?1, ?2, ?3)",
            params![code, isotope.name.trim(), half_life],
        )?;
        Ok(connection.last_insert_rowid())
    }
}

pub(crate) fn save_calculation(
    drug_id: i64,
    drug_name: &str,
    consumers: &[Consumer],
    settings: &CalculationSettings,
    report_name: Option<&str>,
) -> rusqlite::Result<i64> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    save_consumer_centers(&connection, consumers)?;

    let constants_json = to_json(settings)?;
    let inputs_json = to_json(consumers)?;
    connection.execute(
        "INSERT INTO calculations
            (drug_id, drug_type, constants_json, inputs_json, report_name)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![drug_id, drug_name, constants_json, inputs_json, report_name],
    )?;
    Ok(connection.last_insert_rowid())
}

pub(crate) fn update_calculation(
    calculation_id: i64,
    drug_id: i64,
    drug_name: &str,
    consumers: &[Consumer],
    settings: &CalculationSettings,
    report_name: Option<&str>,
) -> rusqlite::Result<()> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    save_consumer_centers(&connection, consumers)?;
    let updated = connection.execute(
        "UPDATE calculations
         SET drug_id = ?2, drug_type = ?3, constants_json = ?4, inputs_json = ?5,
             report_name = ?6, calculated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
        params![
            calculation_id,
            drug_id,
            drug_name,
            to_json(settings)?,
            to_json(consumers)?,
            report_name
        ],
    )?;
    if updated == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

pub(crate) fn delete_saved_calculation(id: i64) -> rusqlite::Result<()> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    connection.execute("DELETE FROM calculations WHERE id = ?1", [id])?;
    Ok(())
}

pub(crate) fn save_drug_profile(name: &str, profile: &DrugProfile) -> rusqlite::Result<i64> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let code = format!("custom:{}", name.trim().to_lowercase());
    let settings_json = to_json(profile)?;
    connection.execute(
        "INSERT INTO drug_types (code, name, settings_json) VALUES (?1, ?2, ?3)",
        params![code, name, settings_json],
    )?;
    Ok(connection.last_insert_rowid())
}

pub(crate) fn update_drug_profile(
    id: i64,
    new_name: &str,
    profile: &DrugProfile,
) -> rusqlite::Result<()> {
    let mut connection = open_connection()?;
    initialize_schema(&connection)?;
    let settings_json = to_json(profile)?;
    let transaction = connection.transaction()?;
    let updated = transaction.execute(
        "UPDATE drug_types SET name = ?2, settings_json = ?3 WHERE id = ?1",
        params![id, new_name, settings_json],
    )?;
    if updated == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    transaction.execute(
        "UPDATE calculations SET drug_type = ?2 WHERE drug_id = ?1",
        params![id, new_name],
    )?;
    transaction.commit()
}

pub(crate) fn delete_drug(id: i64) -> rusqlite::Result<()> {
    let mut connection = open_connection()?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM calculations WHERE drug_id = ?1", [id])?;
    transaction.execute("DELETE FROM drug_types WHERE id = ?1", [id])?;
    transaction.commit()
}

pub(crate) fn load_drug_profile(id: i64) -> rusqlite::Result<Option<DrugProfile>> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let settings = connection
        .query_row(
            "SELECT settings_json FROM drug_types WHERE id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    settings
        .map(|json| {
            serde_json::from_str(&json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
            })
        })
        .transpose()
}

pub(crate) fn load_drugs() -> rusqlite::Result<Vec<DrugListItem>> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let mut statement = connection.prepare("SELECT id, name FROM drug_types ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok(DrugListItem {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    rows.collect()
}

pub(crate) fn load_centers() -> rusqlite::Result<Vec<String>> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let mut statement = connection.prepare(
        "SELECT name FROM centers
         WHERE is_active = 1
           AND name NOT IN ('Отбор проб', 'Промывка линий')
         ORDER BY name",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut centers = Vec::new();
    for row in rows {
        let name = row?;
        let normalized = name.trim().to_lowercase();
        if !centers
            .iter()
            .any(|existing: &String| existing.trim().to_lowercase() == normalized)
        {
            centers.push(name);
        }
    }
    centers.sort_by_key(|name| name.to_lowercase());
    Ok(centers)
}

fn save_consumer_centers(connection: &Connection, consumers: &[Consumer]) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("SELECT name FROM centers")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut known_names = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    for consumer in consumers {
        if consumer.is_mandatory {
            continue;
        }
        let center_name = consumer
            .vial_group_source_name
            .as_deref()
            .unwrap_or(&consumer.name)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if center_name.is_empty() {
            continue;
        }
        let normalized = center_name.to_lowercase();
        if known_names
            .iter()
            .any(|name| name.trim().to_lowercase() == normalized)
        {
            continue;
        }
        connection.execute("INSERT INTO centers (name) VALUES (?1)", [&center_name])?;
        known_names.push(center_name);
    }
    Ok(())
}

pub(crate) fn count_saved_calculations() -> rusqlite::Result<usize> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    connection.query_row("SELECT COUNT(*) FROM calculations", [], |row| row.get(0))
}

pub(crate) fn load_saved_calculation_page(
    limit: usize,
    offset: usize,
) -> rusqlite::Result<Vec<SavedCalculationSummary>> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let mut statement = connection.prepare(
        "SELECT id, calculated_at, drug_type, inputs_json,
                COALESCE(NULLIF(report_name, ''), drug_type || ' ' || calculated_at)
         FROM calculations
         ORDER BY id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = statement.query_map(params![limit as i64, offset as i64], |row| {
        let consumers_json: String = row.get(3)?;
        // A malformed legacy payload must not hide the entire history page.
        let consumers = serde_json::from_str::<Vec<Consumer>>(&consumers_json).unwrap_or_default();
        let mut grouped_consumers = std::collections::HashSet::new();
        let consumer_count = consumers
            .iter()
            .filter(|consumer| !consumer.is_mandatory && !consumer.name.trim().is_empty())
            .filter(|consumer| {
                consumer
                    .vial_group_id
                    .map(|group_id| grouped_consumers.insert(group_id))
                    .unwrap_or(true)
            })
            .count();
        Ok(SavedCalculationSummary {
            id: row.get(0)?,
            calculated_at: row.get(1)?,
            drug_name: row.get(2)?,
            report_title: row.get(4)?,
            consumer_count,
        })
    })?;
    rows.collect()
}

pub(crate) fn load_saved_calculation(id: i64) -> rusqlite::Result<SavedCalculation> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    let (drug_id, drug_name, constants_json, inputs_json, report_name) = connection.query_row(
        "SELECT drug_id, drug_type, constants_json, inputs_json, report_name
         FROM calculations WHERE id = ?1",
        [id],
        |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    Ok(SavedCalculation {
        drug_id,
        drug_name,
        report_name,
        settings: from_json(&constants_json)?,
        consumers: from_json(&inputs_json)?,
    })
}

pub(crate) fn load_saved_calculation_title(id: i64) -> rusqlite::Result<String> {
    let connection = open_connection()?;
    initialize_schema(&connection)?;
    connection.query_row(
        "SELECT COALESCE(NULLIF(report_name, ''), drug_type || ' ' || calculated_at)
         FROM calculations WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
}

fn open_connection() -> rusqlite::Result<Connection> {
    Connection::open(database_path()?)
}

fn initialize_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS centers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            is_active INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS drug_types (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL,
            name TEXT NOT NULL,
            settings_json TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE IF NOT EXISTS calculations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            calculated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            drug_id INTEGER,
            drug_type TEXT NOT NULL,
            constants_json TEXT NOT NULL,
            inputs_json TEXT NOT NULL,
            report_name TEXT
        );
        CREATE TABLE IF NOT EXISTS isotopes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            half_life_minutes REAL NOT NULL
        );
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;

    migrate_drug_identity(connection)
}

fn seed_isotopes(connection: &Connection) -> rusqlite::Result<()> {
    for (code, name, half_life) in [
        ("f18", "F-18", 109.77),
        ("c11", "C-11", 20.4),
        ("n13", "N-13", 9.965),
        ("o15", "O-15", 2.037),
        ("ga68", "Ga-68", 67.71),
        ("cu64", "Cu-64", 762.0),
        ("zr89", "Zr-89", 4704.0),
        ("tc99m", "Tc-99m", 360.0),
        ("i123", "I-123", 792.0),
        ("i131", "I-131", 11_520.0),
        ("y90", "Y-90", 3840.0),
        ("lu177", "Lu-177", 9576.0),
        ("ra223", "Ra-223", 16_459.2),
        ("ac225", "Ac-225", 14_385.6),
    ] {
        connection.execute(
            "INSERT OR IGNORE INTO isotopes (code, name, half_life_minutes)
             VALUES (?1, ?2, ?3)",
            params![code, name, half_life],
        )?;
    }
    Ok(())
}

fn migrate_drug_identity(connection: &Connection) -> rusqlite::Result<()> {
    let table_sql: String = connection.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'drug_types'",
        [],
        |row| row.get(0),
    )?;
    if table_sql
        .to_ascii_uppercase()
        .contains("NAME TEXT NOT NULL UNIQUE")
        || table_sql
            .to_ascii_uppercase()
            .contains("CODE TEXT NOT NULL UNIQUE")
    {
        connection.execute_batch(
            "PRAGMA foreign_keys = OFF;
             BEGIN IMMEDIATE;
             DROP TABLE IF EXISTS drug_types_new;
             CREATE TABLE drug_types_new (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 code TEXT NOT NULL,
                 name TEXT NOT NULL,
                 settings_json TEXT NOT NULL DEFAULT '{}'
             );
             INSERT INTO drug_types_new (id, code, name, settings_json)
                 SELECT id, code, name, settings_json FROM drug_types;
             DROP TABLE drug_types;
             ALTER TABLE drug_types_new RENAME TO drug_types;
             COMMIT;
             PRAGMA foreign_keys = ON;",
        )?;
    }

    let mut statement = connection.prepare("PRAGMA table_info(calculations)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    if !columns.iter().any(|column| column == "drug_id") {
        connection.execute("ALTER TABLE calculations ADD COLUMN drug_id INTEGER", [])?;
    }
    if !columns.iter().any(|column| column == "report_name") {
        connection.execute("ALTER TABLE calculations ADD COLUMN report_name TEXT", [])?;
    }
    connection.execute(
        "UPDATE calculations
         SET drug_id = (
             SELECT MIN(drug_types.id)
             FROM drug_types
             WHERE drug_types.name = calculations.drug_type
         )
         WHERE drug_id IS NULL",
        [],
    )?;
    Ok(())
}

fn to_json<T: Serialize + ?Sized>(value: &T) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn from_json<T: serde::de::DeserializeOwned>(json: &str) -> rusqlite::Result<T> {
    serde_json::from_str(json)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_drugs_to_id_based_identity() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE drug_types (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    code TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL UNIQUE,
                    settings_json TEXT NOT NULL DEFAULT '{}'
                );
                CREATE TABLE calculations (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    calculated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    drug_type TEXT NOT NULL,
                    constants_json TEXT NOT NULL,
                    inputs_json TEXT NOT NULL
                );
                INSERT INTO drug_types (code, name) VALUES ('old', 'Препарат');
                INSERT INTO calculations (drug_type, constants_json, inputs_json)
                    VALUES ('Препарат', '{}', '[]');",
            )
            .expect("legacy schema");

        initialize_schema(&connection).expect("schema migration");
        connection
            .execute(
                "INSERT INTO drug_types (code, name) VALUES ('new', 'Препарат')",
                [],
            )
            .expect("duplicate display names are allowed");

        let linked_id: Option<i64> = connection
            .query_row("SELECT drug_id FROM calculations LIMIT 1", [], |row| {
                row.get(0)
            })
            .expect("migrated calculation");
        assert_eq!(linked_id, Some(1));
    }

    #[test]
    fn seeds_common_medical_isotopes_with_f18_and_c11() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        initialize_schema(&connection).expect("schema");
        seed_isotopes(&connection).expect("isotope seed");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM isotopes", [], |row| row.get(0))
            .expect("isotope count");
        let f18_half_life: f64 = connection
            .query_row(
                "SELECT half_life_minutes FROM isotopes WHERE code = 'f18'",
                [],
                |row| row.get(0),
            )
            .expect("F-18");
        let c11_half_life: f64 = connection
            .query_row(
                "SELECT half_life_minutes FROM isotopes WHERE code = 'c11'",
                [],
                |row| row.get(0),
            )
            .expect("C-11");

        assert_eq!(count, 14);
        assert!((f18_half_life - 109.77).abs() < f64::EPSILON);
        assert!((c11_half_life - 20.4).abs() < f64::EPSILON);
    }

    #[test]
    fn consumer_directory_is_case_insensitive_for_cyrillic_names() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        initialize_schema(&connection).expect("schema");
        let mut first = Consumer::new(false);
        first.name = "ПОДОльск".into();
        let mut second = Consumer::new(false);
        second.name = "ПодОльск".into();

        save_consumer_centers(&connection, &[first]).expect("first consumer");
        save_consumer_centers(&connection, &[second]).expect("same consumer with another case");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM centers", [], |row| row.get(0))
            .expect("consumer count");
        assert_eq!(count, 1);
    }
}
