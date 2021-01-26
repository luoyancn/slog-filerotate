use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use std::thread;
use std::time::{Duration, Instant};
use std::u64;

extern crate libflate;

use libflate::gzip::Encoder as GzipEncoder;

pub const BITE: u64 = 1;
pub const KB: u64 = BITE * 1024;
pub const MB: u64 = KB * 1024;
pub const GB: u64 = MB * 1024;

pub struct FileAppender {
    path: PathBuf,
    file: Option<BufWriter<File>>,
    truncate: bool,
    written_size: u64,
    rotate_size: u64,
    rotate_keep: usize,
    rotate_compress: bool,
    wait_compression: Option<mpsc::Receiver<io::Result<()>>>,
    next_reopen_check: Instant,
    reopen_check_interval: Duration,
}

impl FileAppender {
    pub fn new<P: AsRef<Path>>(
        path: P,
        truncate: bool,
        rotate_size: u64,
        rotate_keep: usize,
        rotate_compress: bool,
    ) -> Self {
        FileAppender {
            path: path.as_ref().to_path_buf(),
            file: None,
            truncate: truncate,
            written_size: 0,
            rotate_size: rotate_size,
            rotate_keep: rotate_keep,
            rotate_compress: rotate_compress,
            wait_compression: None,
            next_reopen_check: Instant::now(),
            reopen_check_interval: Duration::from_millis(1000),
        }
    }

    fn reopen_if_needed(&mut self) -> io::Result<()> {
        let now = Instant::now();
        let path_exists = if now >= self.next_reopen_check {
            self.next_reopen_check = now + self.reopen_check_interval;
            self.path.exists()
        } else {
            true
        };

        if self.file.is_none() || !path_exists {
            let mut file_builder = OpenOptions::new();
            file_builder.create(true);
            if self.truncate {
                file_builder.truncate(true);
            }
            self.file = None;
            let file = file_builder
                .append(!self.truncate)
                .write(true)
                .open(&self.path)?;
            self.written_size = file.metadata()?.len();
            self.file = Some(BufWriter::new(file));
        }
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        {
            if let Some(ref mut rx) = self.wait_compression {
                use std::sync::mpsc::TryRecvError;
                match rx.try_recv() {
                    Err(TryRecvError::Empty) => {
                        return Ok(());
                    }
                    Err(TryRecvError::Disconnected) => {
                        let e = io::Error::new(
                            io::ErrorKind::Other,
                            "Log file compression thread aborted",
                        );
                        return Err(e);
                    }
                    Ok(result) => {
                        result?;
                    }
                }
            }
            self.wait_compression = None;
        }
        let _ = self.file.take();

        for i in (1..=self.rotate_keep).rev() {
            let from = self.rotated_path(i)?;
            let to = self.rotated_path(i + 1)?;
            if from.exists() {
                fs::rename(from, to)?;
            }
        }
        if self.path.exists() {
            let rotated_path = self.rotated_path(1)?;
            {
                if self.rotate_compress {
                    let (plain_path, temp_gz_path) = self.rotated_paths_for_compression()?;
                    let (tx, rx) = mpsc::channel();

                    fs::rename(&self.path, &plain_path)?;
                    thread::spawn(move || {
                        let result = Self::compress(plain_path, temp_gz_path, rotated_path);
                        let _ = tx.send(result);
                    });

                    self.wait_compression = Some(rx);
                } else {
                    fs::rename(&self.path, rotated_path)?;
                }
            }
        }

        let delete_path = self.rotated_path(self.rotate_keep + 1)?;
        if delete_path.exists() {
            fs::remove_file(delete_path)?;
        }

        self.written_size = 0;
        self.next_reopen_check = Instant::now();
        self.reopen_if_needed()?;
        Ok(())
    }

    fn rotated_path(&self, i: usize) -> io::Result<PathBuf> {
        let path = self.path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Non UTF-8 log file path: {:?}", self.path),
            )
        })?;
        {
            if self.rotate_compress {
                Ok(PathBuf::from(format!("{}.{}.gz", path, i)))
            } else {
                Ok(PathBuf::from(format!("{}.{}", path, i)))
            }
        }
    }

    fn rotated_paths_for_compression(&self) -> io::Result<(PathBuf, PathBuf)> {
        let path = self.path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Non UTF-8 log file path: {:?}", self.path),
            )
        })?;
        Ok((
            PathBuf::from(format!("{}.1", path)),
            PathBuf::from(format!("{}.1.gz.temp", path)),
        ))
    }

    fn compress(input_path: PathBuf, temp_path: PathBuf, output_path: PathBuf) -> io::Result<()> {
        let mut input = File::open(&input_path)?;
        let mut temp = GzipEncoder::new(File::create(&temp_path)?)?;
        io::copy(&mut input, &mut temp)?;
        temp.finish().into_result()?;

        fs::rename(temp_path, output_path)?;
        fs::remove_file(input_path)?;
        Ok(())
    }
}

impl Write for FileAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.reopen_if_needed()?;
        let size = if let Some(ref mut f) = self.file {
            f.write(buf)?
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Cannot open file: {:?}", self.path),
            ));
        };

        self.written_size += size as u64;
        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(ref mut f) = self.file {
            f.flush()?;
        }
        if self.written_size >= self.rotate_size {
            self.rotate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
