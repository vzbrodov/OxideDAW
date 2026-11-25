//! Recovery snapshots use the native project format and a private state directory.
use crate::session::Session;
use oxide_core::{project::ProjectResult, Project};
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SNAPSHOT: &str = "recovery.oxide.json";
const INTERVAL: Duration = Duration::from_secs(10);

pub(crate) struct Recovery {
    slot: Slot,
    pub candidate: Option<(Slot, ProjectResult<Session>)>,
    last: Project,
    next: Instant,
}

impl Recovery {
    pub fn start(session: &Session) -> ProjectResult<Self> {
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .ok_or("No state directory available")?;
        Self::in_directory(&state.join("oxidedaw/recovery"), session)
    }

    fn in_directory(root: &Path, session: &Session) -> ProjectResult<Self> {
        private_directory(root, true)?;
        let mut candidates = fs::read_dir(root)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .filter(|dir| dir.join(SNAPSHOT).is_file())
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|dir| {
            dir.join(SNAPSHOT)
                .metadata()
                .and_then(|info| info.modified())
                .ok()
        });
        let candidate = candidates
            .into_iter()
            .rev()
            .find_map(|dir| Slot::lock(dir).ok())
            .map(|slot| {
                let session = Session::recover(&slot.path());
                (slot, session)
            });

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = root.join(format!("{}-{timestamp}", std::process::id()));
        private_directory(&dir, false)?;
        Ok(Self {
            slot: Slot::lock(dir)?,
            candidate,
            last: session.editor.project().clone(),
            next: Instant::now() + INTERVAL,
        })
    }

    pub fn poll(&mut self, session: &Session, ctx: &eframe::egui::Context) -> ProjectResult<()> {
        let now = Instant::now();
        if now >= self.next {
            self.next = now + INTERVAL;
            ctx.request_repaint_after(INTERVAL);
            return self.snapshot(session);
        }
        ctx.request_repaint_after(self.next - now);
        Ok(())
    }

    fn snapshot(&mut self, session: &Session) -> ProjectResult<()> {
        let project = session.editor.project();
        if &self.last != project {
            if session.dirty() {
                session.document_for(None).save(&self.slot.path())?;
                self.last = project.clone();
            } else {
                self.clear(session)?;
            }
        }
        Ok(())
    }

    pub fn clear(&mut self, session: &Session) -> ProjectResult<()> {
        self.slot.clear()?;
        self.last = session.editor.project().clone();
        Ok(())
    }

    pub fn accept(&mut self, candidate: Slot, session: &Session) -> ProjectResult<()> {
        let unused = std::mem::replace(&mut self.slot, candidate);
        self.last = session.editor.project().clone();
        unused.remove()?;
        Ok(())
    }

    pub fn finish(&self) -> ProjectResult<()> {
        self.slot.remove()?;
        Ok(())
    }
}

fn private_directory(path: &Path, recursive: bool) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

pub(crate) struct Slot {
    dir: PathBuf,
    // The directory lock is released by the OS after a crash.
    _lock: File,
}

impl Slot {
    fn lock(dir: PathBuf) -> ProjectResult<Self> {
        let lock = File::open(&dir)?;
        lock.try_lock()?;
        Ok(Self { dir, _lock: lock })
    }

    fn path(&self) -> PathBuf {
        self.dir.join(SNAPSHOT)
    }

    fn clear(&self) -> io::Result<()> {
        match fs::remove_file(self.path()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    fn remove(&self) -> io::Result<()> {
        self.clear()?;
        match fs::remove_dir(&self.dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::edit::EditCommand;

    #[test]
    fn restart_recovers_edits_without_replacing_the_original() -> ProjectResult<()> {
        let root = std::env::temp_dir().join(format!("oxide-recovery-{}", std::process::id()));
        fs::create_dir(&root)?;
        let original = root.join("original.oxide.json");
        // Copy only the document to exercise recovery with its media offline.
        fs::write(
            &original,
            include_bytes!("../../../tests/fixtures/legacy-beat/v2.oxide.json"),
        )?;
        let project = Project::load(&original)?;
        let mut session = Session::open(Some(&original))?;
        let state = root.join("state");
        let mut writer = Recovery::in_directory(&state, &session)?;
        let snapshot = writer.slot.path();

        writer.snapshot(&session)?;
        assert!(!snapshot.exists());
        session
            .editor
            .apply_batch(&[EditCommand::SetBpm { bpm: 131.0 }])?;
        writer.snapshot(&session)?;
        assert!(snapshot.exists());

        let concurrent = Recovery::in_directory(&state, &session)?;
        assert!(concurrent.candidate.is_none());
        concurrent.finish()?;
        drop(writer);

        let fresh = Session::open(None)?;
        let mut restarted = Recovery::in_directory(&state, &fresh)?;
        let (candidate, recovered) = restarted.candidate.take().unwrap();
        let mut recovered = recovered?;
        assert!(recovered.dirty());
        assert_eq!(recovered.editor.project().bpm, 131.0);
        assert_eq!(
            recovered.sample_path(0),
            Some(root.canonicalize()?.join("sample.wav"))
        );
        assert!(recovered.save(&original).is_err());
        assert_eq!(Project::load(&original)?.bpm, project.bpm);

        restarted.accept(candidate, &recovered)?;
        let destination = root.join("recovered.oxide.json");
        recovered.save(&destination)?;
        restarted.clear(&recovered)?;
        assert!(!snapshot.exists());
        restarted.finish()?;
        let restored = Project::load(&destination)?;
        assert_eq!(restored.bpm, 131.0);
        assert_eq!(restored.arrangement, project.arrangement);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
