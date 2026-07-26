use rusqlite::Error;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const DATA_DIRECTORY_NAME: &str = "data";
const DATABASE_FILE_NAME: &str = "calculator.sqlite3";

static DATABASE_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Returns the portable database path located next to the running executable.
///
/// The resolved path is cached so every database connection uses exactly the
/// same file even if the process working directory is changed later.
pub(crate) fn database_path() -> rusqlite::Result<PathBuf> {
    if let Some(path) = DATABASE_PATH.get() {
        return Ok(path.clone());
    }

    let executable = env::current_exe().map_err(io_error)?;
    let executable_directory = executable
        .parent()
        .ok_or_else(|| Error::InvalidPath(executable.clone()))?;
    let working_directory = env::current_dir().ok();
    let path = prepare_portable_database(executable_directory, working_directory.as_deref())?;

    // Another thread may have initialized the path first. In that case its
    // value is authoritative and must be used by all subsequent connections.
    let _ = DATABASE_PATH.set(path);
    Ok(DATABASE_PATH
        .get()
        .expect("database path was initialized")
        .clone())
}

fn prepare_portable_database(
    executable_directory: &Path,
    working_directory: Option<&Path>,
) -> rusqlite::Result<PathBuf> {
    let data_directory = executable_directory.join(DATA_DIRECTORY_NAME);
    fs::create_dir_all(&data_directory).map_err(io_error)?;

    let database = data_directory.join(DATABASE_FILE_NAME);
    if database.exists() {
        return Ok(database);
    }

    // Older versions used a path relative to the process working directory.
    // Prefer a database beside the executable, then check the launch directory.
    let beside_executable = executable_directory.join(DATABASE_FILE_NAME);
    let in_working_directory = working_directory.map(|path| path.join(DATABASE_FILE_NAME));

    for legacy_database in std::iter::once(beside_executable)
        .chain(in_working_directory)
        .filter(|path| path != &database && path.is_file())
    {
        copy_database_atomically(&legacy_database, &database)?;
        break;
    }

    Ok(database)
}

fn copy_database_atomically(source: &Path, destination: &Path) -> rusqlite::Result<()> {
    let temporary = destination.with_extension(format!("sqlite3.migrating-{}", std::process::id()));

    fs::copy(source, &temporary).map_err(io_error)?;
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        return Err(io_error(error));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> Error {
    Error::ToSqlConversionFailure(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        env::temp_dir().join(format!(
            "cyclotrone-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn migrates_legacy_database_without_removing_it() {
        let root = test_directory("migration");
        let executable_directory = root.join("app");
        fs::create_dir_all(&executable_directory).expect("test directory");
        let legacy = executable_directory.join(DATABASE_FILE_NAME);
        fs::write(&legacy, b"legacy database").expect("legacy database");

        let database =
            prepare_portable_database(&executable_directory, None).expect("portable path");

        assert_eq!(
            database,
            executable_directory
                .join(DATA_DIRECTORY_NAME)
                .join(DATABASE_FILE_NAME)
        );
        assert_eq!(
            fs::read(&database).expect("new database"),
            b"legacy database"
        );
        assert!(
            legacy.exists(),
            "the legacy database must remain as a backup"
        );
        fs::remove_dir_all(root).expect("test cleanup");
    }

    #[test]
    fn existing_portable_database_is_never_overwritten() {
        let root = test_directory("existing");
        let executable_directory = root.join("app");
        let data_directory = executable_directory.join(DATA_DIRECTORY_NAME);
        fs::create_dir_all(&data_directory).expect("test directory");
        fs::write(
            executable_directory.join(DATABASE_FILE_NAME),
            b"legacy database",
        )
        .expect("legacy database");
        let database = data_directory.join(DATABASE_FILE_NAME);
        fs::write(&database, b"current database").expect("current database");

        prepare_portable_database(&executable_directory, None).expect("portable path");

        assert_eq!(
            fs::read(&database).expect("new database"),
            b"current database"
        );
        fs::remove_dir_all(root).expect("test cleanup");
    }

    #[test]
    fn checks_the_legacy_working_directory() {
        let root = test_directory("working-directory");
        let executable_directory = root.join("app");
        let working_directory = root.join("launch");
        fs::create_dir_all(&executable_directory).expect("executable directory");
        fs::create_dir_all(&working_directory).expect("working directory");
        let legacy = working_directory.join(DATABASE_FILE_NAME);
        fs::write(&legacy, b"working directory database").expect("legacy database");

        let database = prepare_portable_database(&executable_directory, Some(&working_directory))
            .expect("portable path");

        assert_eq!(
            fs::read(database).expect("new database"),
            b"working directory database"
        );
        assert!(
            legacy.exists(),
            "the legacy database must remain as a backup"
        );
        fs::remove_dir_all(root).expect("test cleanup");
    }
}
