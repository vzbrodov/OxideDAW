use crate::Result;
use oxide_core::{
    edit::{EditCommand, ProjectEditor},
    Project,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub fn pack(source: &Path, directory: &Path) -> Result<PathBuf> {
    let project = Project::load(source)?;
    let mut media = Vec::new();
    let mut edits = Vec::new();
    for (index, track) in project.tracks.iter().enumerate() {
        if track.sample.is_none() {
            continue;
        }
        let path = project.sample_path(source, index)?;
        let description = format!("Track {} ({}): {}", track.id, track.name, path.display());
        let (input, size) =
            open_sample(&path).map_err(|error| format!("{description}: {error}"))?;
        let mut name = PathBuf::from(format!("track-{}", track.id));
        if let Some(extension) = path.extension().filter(|extension| extension.len() <= 16) {
            name.set_extension(extension);
        }
        let relative = Path::new("media").join(name);
        edits.push(EditCommand::SetSample {
            track_id: track.id,
            sample: Some(relative.clone()),
        });
        media.push((relative, input.take(size), size, description));
    }
    let mut editor = ProjectEditor::new(project)?;
    editor.apply_batch(&edits)?;

    fs::create_dir(directory).map_err(|error| {
        format!(
            "Cannot create package directory {}: {error}",
            directory.display()
        )
    })?;
    let destination = directory.join("project.oxide.json");
    let media_directory = directory.join("media");
    let mut created_media_directory = false;
    let mut copied = Vec::new();
    let result = (|| -> Result<()> {
        if !media.is_empty() {
            fs::create_dir(&media_directory)?;
            created_media_directory = true;
        }
        for (relative, mut input, size, description) in media {
            let target = directory.join(relative);
            (|| -> io::Result<()> {
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)?;
                copied.push(target);
                if io::copy(&mut input, &mut output)? != size {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "sample shrank during copy",
                    ));
                }
                output.sync_all()
            })()
            .map_err(|error| format!("Cannot copy {description}: {error}"))?;
        }
        #[cfg(unix)]
        if created_media_directory {
            File::open(&media_directory)?.sync_all()?;
        }
        editor.project().save_new(&destination)
    })();

    if result.is_err() && !destination.is_file() {
        for path in copied {
            let _ = fs::remove_file(path);
        }
        if created_media_directory {
            let _ = fs::remove_dir(media_directory);
        }
        let _ = fs::remove_dir(directory);
    }
    result.map_err(|error| format!("Cannot pack {}: {error}", directory.display()))?;
    Ok(destination)
}

fn open_sample(path: &Path) -> io::Result<(File, u64)> {
    fn regular_file_size(metadata: fs::Metadata) -> io::Result<u64> {
        if metadata.is_file() {
            Ok(metadata.len())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a regular sample file",
            ))
        }
    }
    regular_file_size(fs::metadata(path)?)?;
    let input = File::open(path)?;
    let size = regular_file_size(input.metadata()?)?;
    Ok((input, size))
}
