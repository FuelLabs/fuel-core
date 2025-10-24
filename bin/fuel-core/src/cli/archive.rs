mod backup;
mod restore;

use crate::cli::archive::{
    backup::backup,
    restore::restore,
};
use clap::{
    Parser,
    Subcommand,
};
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

pub async fn exec(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Backup(args) => backup(&args.from, &args.to),
        Command::Restore(args) => restore(&args.from, &args.to),
    }
}

pub fn add_to_archive(src_dir: &Path, dest_file: &Path) -> anyhow::Result<()> {
    let file = File::create(dest_file)?;
    let writer = BufWriter::new(file);
    let mut tar = Builder::new(writer);

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

    let all_files = collect_files(src_dir, src_dir)?;

    // process the files in parallel w/ rayon
    let files = all_files
        .par_iter()
        .filter_map(|(relative_path, path)| {
            let metadata = path.metadata().ok()?;
            let mut header = Header::new_gnu();
            header.set_path(relative_path).ok()?;
            header.set_size(metadata.len());
            header.set_mode(metadata.permissions().mode());
            header.set_mtime(metadata.modified().ok()?.elapsed().ok()?.as_secs());
            header.set_cksum();

            Some((header, path.clone()))
        })
        .collect::<Vec<_>>();

    // Process files sequentially, reading and writing each one
    for (header, path) in files {
        let file = File::open(path)?;
        tar.append(&header, file)?;
    }

    tar.finish()?;
    Ok(())
}

pub fn extract_from_archive(src_file: &Path, dest_dir: &Path) -> anyhow::Result<()> {
    let src_file = Path::new(src_file);
    let dest_dir = Path::new(dest_dir);

    let file = File::open(src_file)?;
    let decoder = BufReader::new(file);
    let mut archive = Archive::new(decoder);

    archive.unpack(dest_dir)?;
    Ok(())
}
