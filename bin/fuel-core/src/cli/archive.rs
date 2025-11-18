use clap::{
    Parser,
    Subcommand,
};
use fuel_core::combined_database::CombinedDatabase;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Back up the database into a tar archive
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

    /// Destination directory where the tar files will be created.
    #[arg(long, help = "The destination directory for the backup tar files")]
    pub to: String,

    /// Name of the final tar file (default: db.tar)
    #[arg(
        long,
        default_value = DEFAULT_DB_TAR_FILE,
        help = "The name of the tar file to create"
    )]
    pub file_name: String,
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
        Command::Backup(args) => backup(&args.from, &args.to, &args.file_name),
        Command::Restore(args) => restore(&args.to, &args.from),
    }
}

#[cfg(not(feature = "archive"))]
pub fn backup(db_dir: &str, backup_path: &str) -> anyhow::Result<()> {
    CombinedDatabase::backup(db_dir.as_ref(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(feature = "archive")]
pub fn backup(
    db_dir: &str,
    backup_path: &str,
    tar_file_name: &str,
) -> anyhow::Result<()> {
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
                    .filter_map(|(rel, abs)| {
                        let metadata = abs.metadata().ok()?;
                        let mut header = Header::new_gnu();
                        let path = Path::new(&dir_name).join(rel);
                        header.set_path(path).ok()?;
                        header.set_size(metadata.len());
                        header.set_mode(metadata.permissions().mode());
                        header.set_mtime(
                            metadata.modified().ok()?.elapsed().ok()?.as_secs(),
                        );
                        header.set_cksum();
                        Some((header, abs))
                    })
                    .collect::<Vec<_>>();

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
