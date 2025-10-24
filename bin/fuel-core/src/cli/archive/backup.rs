use crate::cli::archive::add_to_archive;
use fuel_core::combined_database::CombinedDatabase;
use tempfile::TempDir;

pub fn backup(db_dir: &str, backup_path: &str) -> anyhow::Result<()> {
    let tmp_backup_dir = TempDir::new()?;
    let db_dir = std::path::Path::new(db_dir);
    let backup_file = std::path::Path::new(backup_path);

    let path = tmp_backup_dir.path();
    CombinedDatabase::backup(db_dir, path)?;
    add_to_archive(path, backup_file)?;

    Ok(())
}
