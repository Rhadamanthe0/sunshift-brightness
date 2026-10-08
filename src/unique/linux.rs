use crate::event_watcher::linux::{get_ipc_path, MSG_OPEN_WINDOW};
use crate::unique::Error;
use nix::errno::Errno;
use nix::fcntl::{flock, FlockArg};
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

// Keep both inodes on disk: unlinking either during another process's open
// can create independent locks or disconnect the IPC watcher.
pub struct SsbUniqueInstance {
    _reader: File,
    _lock: File,
}

impl SsbUniqueInstance {
    pub fn try_acquire() -> Result<Self, Error> {
        Self::acquire_at(&get_ipc_path())
    }

    fn acquire_at(ipc_path: &Path) -> Result<Self, Error> {
        let acquire = || -> std::io::Result<Result<Self, ExistingInstance>> {
            std::fs::create_dir_all(ipc_path.parent().expect("IPC path must have parent"))?;
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
                .open(ipc_path.with_extension("lock"))?;
            match flock(lock.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
                Ok(()) => {}
                Err(Errno::EWOULDBLOCK) => return Ok(Err(ExistingInstance(ipc_path.to_owned()))),
                Err(e) => return Err(e.into()),
            }
            match mkfifo(ipc_path, Mode::S_IWUSR | Mode::S_IRUSR) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(e) => return Err(e.into()),
            }
            if !std::fs::symlink_metadata(ipc_path)?.file_type().is_fifo() {
                return Err(std::io::Error::other("IPC path is not a FIFO"));
            }
            let reader = OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
                .open(ipc_path)?;
            Ok(Ok(Self {
                _reader: reader,
                _lock: lock,
            }))
        };
        match acquire() {
            Ok(Ok(instance)) => Ok(instance),
            Ok(Err(existing)) => Err(Error::AlreadyRunning(existing)),
            Err(e) => Err(Error::PlatformError(Box::new(e))),
        }
    }
}

#[derive(Debug)]
pub struct ExistingInstance(PathBuf);

impl ExistingInstance {
    pub fn wakeup(&self) -> anyhow::Result<()> {
        OpenOptions::new()
            .write(true)
            .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
            .open(&self.0)?
            .write_all(&[MSG_OPEN_WINDOW])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn simultaneous_starts_have_one_owner_and_can_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ipc");
        let barrier = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let acquired = SsbUniqueInstance::acquire_at(&path);
                    barrier.wait();
                    (
                        acquired.is_ok(),
                        matches!(acquired, Err(Error::AlreadyRunning(_))),
                    )
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.0).count(), 1);
        assert_eq!(results.iter().filter(|r| r.1).count(), 7);
        assert!(SsbUniqueInstance::acquire_at(&path).is_ok());
    }

    #[test]
    fn refuses_a_regular_file_or_symlink_as_ipc() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ipc");
        std::fs::write(&path, "keep").unwrap();
        assert!(matches!(
            SsbUniqueInstance::acquire_at(&path),
            Err(Error::PlatformError(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
        let symlink = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &symlink).unwrap();
        assert!(matches!(
            SsbUniqueInstance::acquire_at(&symlink),
            Err(Error::PlatformError(_))
        ));
    }
}
