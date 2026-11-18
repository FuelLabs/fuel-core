use clap::{
    Parser,
    Subcommand,
};
use fuel_core::combined_database::CombinedDatabase;
use fuel_core_types::tai64::Tai64;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Backup the database into multiple tar files for each database, then concatenate into a final tar
    Backup(BackupArgs),
    /// Restore the database from a tar archive
    Restore(RestoreArgs),
}

pub const DEFAULT_DB_TAR_FILE: &str = "db.tar";
#[derive(Debug, Parser)]
pub struct BackupArgs {
    /// Database directory to back up
    #[arg(long, help = "The source directory of the database to backup")]
    pub from: String,

    /// Destination directory where intermediate tar files will be created.
    ///
    /// Note: Backup produces multiple tar chunks here, for each database,
    /// then concatenates them into the final tar file (see --file-name).
    #[arg(long, help = "The destination directory for the backup tar files")]
    pub to: String,

    /// Name of the final tar file (default: db.tar)
    #[arg(
        long,
        default_value = DEFAULT_DB_TAR_FILE,
        help = "The name of the tar file to create"
    )]
    pub file_name: String,

    /// Offline prune history older than this duration before backup (e.g. `30d`).
    /// Deletes offchain indexes/statuses and aged onchain blocks/txs (scope B);
    /// keeps live Coins/Contracts/UTXO. Never use `--db-prune` for retention
    /// (that flag destroys the DB). Full Archive must leave this unset.
    #[arg(long = "history-retention", env)]
    pub history_retention: Option<humantime::Duration>,
}

#[derive(Debug, Parser)]
pub struct RestoreArgs {
    /// Path to the backup tar file to restore from
    #[arg(long, help = "The source tar file to restore data from")]
    pub from: String,

    /// Destination directory where the database will be restored
    #[arg(long, help = "The target directory to restore the database into")]
    pub to: String,
}

pub fn exec(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Backup(args) => backup(
            &args.from,
            &args.to,
            &args.file_name,
            args.history_retention.map(Into::into),
        ),
        Command::Restore(args) => restore(&args.to, &args.from),
    }
}

fn maybe_prune_before_backup(
    db_dir: &str,
    history_retention: Option<std::time::Duration>,
) -> anyhow::Result<()> {
    if let Some(retention) = history_retention {
        tracing::info!(
            retention = ?retention,
            "Pruning history (scope B) before archive backup"
        );
        let report =
            CombinedDatabase::prune_history_at(db_dir.as_ref(), retention, Tai64::now())?;
        tracing::info!(?report, "History prune complete");
    }
    Ok(())
}

#[cfg(not(feature = "archive"))]
pub fn backup(
    db_dir: &str,
    backup_path: &str,
    _: &str,
    history_retention: Option<std::time::Duration>,
) -> anyhow::Result<()> {
    maybe_prune_before_backup(db_dir, history_retention)?;
    CombinedDatabase::backup(db_dir.as_ref(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(feature = "archive")]
pub fn backup(
    db_dir: &str,
    backup_path: &str,
    tar_file_name: &str,
    history_retention: Option<std::time::Duration>,
) -> anyhow::Result<()> {
    maybe_prune_before_backup(db_dir, history_retention)?;
    let tmp_dir = tempfile::TempDir::new()?;
    CombinedDatabase::backup(db_dir.as_ref(), tmp_dir.path())?;
    archiver::add_to_archive(tmp_dir.path(), backup_path.as_ref(), tar_file_name)?;
    Ok(())
}

#[cfg(not(feature = "archive"))]
pub fn restore(restore_to: &str, backup_path: &str) -> anyhow::Result<()> {
    CombinedDatabase::restore(restore_to.as_ref(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(feature = "archive")]
pub fn restore(restore_to: &str, backup_path: &str) -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    archiver::extract_from_archive(backup_path.as_ref(), tmp_dir.path())?;
    CombinedDatabase::restore(restore_to.as_ref(), tmp_dir.path())?;
    Ok(())
}

#[cfg(feature = "archive")]
mod archiver {
    use anyhow::anyhow;
    use rayon::prelude::*;
    use std::{
        fs::{
            self,
            File,
        },
        io::{
            BufReader,
            BufWriter,
            Write,
        },
        os::unix::fs::PermissionsExt,
        path::{
            Path,
            PathBuf,
        },
    };
    use tar::{
        Archive,
        Builder,
        Header,
    };

    pub fn add_to_archive(
        src_dir: &Path,
        dest_path: &Path,
        db_file_name: &str,
    ) -> anyhow::Result<()> {
        let top_level_dirs = fs::read_dir(src_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_dir() { Some(path) } else { None }
            })
            .collect::<Vec<_>>();

        let tar_paths = top_level_dirs
            .par_iter()
            .map(|dir| {
                let dir_name = dir
                    .file_name()
                    .ok_or(anyhow!("Failed to get the directory name."))?
                    .to_str()
                    .ok_or(anyhow!("Failed to get the directory name."))?;
                let tar_path = dest_path.join(format!("{}.tar", dir_name));
                let writer = BufWriter::new(File::create(&tar_path)?);
                let mut tar = Builder::new(writer);

                let files = collect_files(dir, dir)?
                    .into_iter()
                    .map(|(rel, abs)| {
                        let metadata = abs.metadata()?;
                        let mut header = Header::new_gnu();
                        let path = Path::new(&dir_name).join(rel);
                        header.set_path(path)?;
                        header.set_size(metadata.len());
                        header.set_mode(metadata.permissions().mode());
                        header.set_mtime(
                            metadata
                                .modified()?
                                .duration_since(std::time::UNIX_EPOCH)?
                                .as_secs(),
                        );
                        header.set_cksum();
                        Ok((header, abs))
                    })
                    .collect::<Result<Vec<_>, anyhow::Error>>()?;

                for (header, path) in files {
                    tar.append(&header, File::open(path)?)?;
                }

                tar.finish()?;
                Ok::<_, anyhow::Error>(tar_path)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut final_writer =
            BufWriter::new(File::create(dest_path.join(db_file_name))?);
        for tar_path in tar_paths {
            let mut reader = BufReader::new(File::open(tar_path)?);
            std::io::copy(&mut reader, &mut final_writer)?;
            final_writer.flush()?;
        }

        Ok(())
    }

    pub fn extract_from_archive(src_file: &Path, dest_dir: &Path) -> anyhow::Result<()> {
        let reader = BufReader::new(File::open(src_file)?);
        let mut archive = Archive::new(reader);
        archive.set_ignore_zeros(true); // Because we've concatenated all the individual tars to db.tar
        archive.unpack(dest_dir)?;

        Ok(())
    }

    fn collect_files(
        path: &Path,
        base_path: &Path,
    ) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(base_path)?.to_path_buf();

            if path.is_file() {
                files.push((relative_path, path));
            } else if path.is_dir() {
                files.extend(collect_files(&path, base_path)?);
            }
        }
        Ok(files)
    }
}
