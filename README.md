
There was already an implement of file rotation for `slog` named
[slogger](https://docs.rs/sloggers/). While, it lost part of [slog](https://docs.rs/slog/) feartures, such as custom the log timestamp.

`slog-filerotate` is a personal log file rotate implememt for `slog`, in fact, it is a part of `slogger`, just the file roration. And most
of the features of `slog` are retained. Thanks the effictive works by sile.

Example:

```
use std::io;
extern crate chrono;

#[macro_use]
extern crate slog;
extern crate slog_filerotate;
extern crate slog_term;

use slog::Drain;
use slog::FnValue;
use slog_filerotate::{FileAppender, KB};

const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.9f";

fn main() {
    // first params means the log file path
    // second means truncate
    // third means log file size, for file rotation
    // forth means keep how many logs
    // fifth means compress the log file
    let adapter = FileAppender::new("logfiles", true, KB, 2, true);
    let decorator_file = slog_term::PlainSyncDecorator::new(adapter);
    let drain_file = slog_term::FullFormat::new(decorator_file)
        .use_custom_timestamp(move |io: &mut dyn io::Write| {
            write!(io, "{}", chrono::Local::now().format(TIMESTAMP_FORMAT))
        })
        .build();

    let logger = slog::Logger::root(
        slog::LevelFilter::new(drain_file, slog::Level::Debug).fuse(),
        o!("place" => FnValue( move |info| { format!("{}:{} {}", info.file(), info.line(), info.module())})),
    );

    debug!(logger, "hello debug");
    info!(logger, "hello info");
    warn!(logger, "hello warn");
    error!(logger, "hello error");
}
```
