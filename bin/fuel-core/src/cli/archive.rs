use clap::{
    Parser,
    Subcommand,
};
use fuel_core::combined_database::CombinedDatabase;

#[derive(Debug, Subcommand)]
pub enum Command {
    Backup(BackupArgs),
    Restore(RestoreArgs),
}

#[derive(Debug, Parser)]
pub struct BackupArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,
}

#[derive(Debug, Parser)]
pub struct RestoreArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,
}

pub fn exec(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Backup(args) => backup(&args.from, &args.to),
        Command::Restore(args) => restore(&args.from, &args.to),
    }
}

#[cfg(not(feature = "archive"))]
pub fn backup(db_dir: &str, backup_path: &str) -> anyhow::Result<()> {
    CombinedDatabase::backup(db_dir.as_ref(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(feature = "archive")]
pub fn backup(db_dir: &str, backup_path: &str) -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    CombinedDatabase::backup(db_dir.as_ref(), tmp_dir.path())?;
    archiver::add_to_archive(tmp_dir.path(), backup_path.as_ref())?;
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
    use rayon::prelude::*;
    use std::{
        fs::{
            self,
            File,
        },
        io::{
            BufReader,
            BufWriter,
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

    pub fn add_to_archive(src_dir: &Path, dest_file: &Path) -> anyhow::Result<()> {
        let writer = BufWriter::new(File::create(dest_file)?);
        let mut tar = Builder::new(writer);

        let files = collect_files(src_dir)?
            .par_iter()
            .filter_map(|(rel, abs)| {
                let metadata = abs.metadata().ok()?;
                let mut header = Header::new_gnu();
                header.set_path(rel).ok()?;
                header.set_size(metadata.len());
                header.set_mode(metadata.permissions().mode());
                header.set_mtime(metadata.modified().ok()?.elapsed().ok()?.as_secs());
                header.set_cksum();
                Some((header, abs.clone()))
            })
            .collect::<Vec<_>>();

        for (header, path) in files {
            tar.append(&header, File::open(path)?)?;
        }

        tar.finish()?;
        Ok(())
    }

    pub fn extract_from_archive(src_file: &Path, dest_dir: &Path) -> anyhow::Result<()> {
        let reader = BufReader::new(File::open(src_file)?);
        Archive::new(reader).unpack(dest_dir)?;
        Ok(())
    }

    fn collect_files(base: &Path) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(base)?.to_path_buf();

            if path.is_file() {
                files.push((rel, path));
            } else if path.is_dir() {
                files.extend(collect_files(&path)?);
            }
        }
        Ok(files)
    }
}
