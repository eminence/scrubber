use clap::{App, Arg};
use dirs::home_dir;
use std::borrow::Cow;
use std::ffi::OsString;
use std::fs::{read_dir, remove_dir, remove_file, set_permissions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::{env::var_os, fs::symlink_metadata};
use walkdir::WalkDir;

fn get_username() -> OsString {
    if cfg!(windows) {
        var_os("USERNAME").expect("Unknown username")
    } else {
        var_os("USER").expect("Unknown username")
    }
}

/// Returns tuple: (is file old?  size of file,  timestamp)
fn file_is_old<P: AsRef<Path>>(f: P, use_atime: bool) -> (bool, u64, Option<SystemTime>) {
    let f: &Path = f.as_ref();
    let old = Duration::from_secs(60 * 60 * 24 * 21);
    let now = SystemTime::now();
    if let Ok(md) = symlink_metadata(f) {
        let mda = md.accessed().ok();
        let mdm = md.modified().ok();

        let keep_due_to_mtime: bool = mdm.map_or(true, |t| {
            now.duration_since(t).map(|t| t < old).unwrap_or(true)
        });

        if use_atime {
            let keep_due_to_atime: bool = mda.map_or(true, |t| {
                now.duration_since(t).map(|t| t < old).unwrap_or(true)
            });
            (
                !(keep_due_to_mtime || keep_due_to_atime),
                md.len(),
                md.accessed().ok(),
            )
        } else {
            (!keep_due_to_mtime, md.len(), md.modified().ok())
        }
    } else {
        println!("Warning: unable to get metadata for entry {:?}", f);
        (false, 0, None)
    }
}

enum Removable {
    /// A directory is always removable if it is empty
    Always,
    /// We can remove a folder/file, freeing up some space
    True {
        /// The total size of the thing to be removed
        size: u64,
        /// The most recent timestamp of the thing to be removed
        timestamp: Option<SystemTime>,
    },
    /// If this path can't be removed, include why
    False(PathBuf),
}

impl Removable {
    fn and(&mut self, other: Removable) {
        match (self, other) {
            (Removable::False(_), _) => {
                // i am not removable, so it doesn't matter that the `other` is
                return;
            }

            (Removable::Always, Removable::Always) => {
                return;
            }

            (me, o @ Removable::False(_)) => *me = o,

            (
                Removable::True {
                    size: my_size,
                    timestamp,
                },
                Removable::True {
                    size: other_size,
                    timestamp: other_ts,
                },
            ) => {
                match (timestamp, other_ts) {
                    (None, None) => {}
                    (a @ None, other @ Some(_)) => *a = other,
                    (Some(_), None) => {}
                    (Some(me), Some(other)) => {
                        // take the most recent ts
                        if other > *me {
                            *me = other;
                        }
                    }
                };
                *my_size += other_size;
            }

            (Removable::True { .. }, Removable::Always) => {
                return;
            }

            (Removable::Always, Removable::True { .. }) => {
                panic!()
            }
        }
    }
}

fn can_be_removed<P: AsRef<Path>>(dir: P, use_atime: bool) -> Result<Removable, std::io::Error> {
    let dir = dir.as_ref();

    if dir.is_file() {
        let (is_old, size, ts) = file_is_old(dir, use_atime);
        return if is_old {
            Ok(Removable::True {
                size,
                timestamp: ts,
            })
        } else {
            Ok(Removable::False(dir.to_owned()))
        };
    } // else is_dir

    let mut dirs = read_dir(dir)?.peekable();
    if dirs.peek().is_none() {
        // there are no entries in this directory, and we can delete it without prompting
        return Ok(Removable::Always);
    }

    let mut remove = Removable::True {
        size: 0,
        timestamp: None,
    };
    for entry in dirs {
        let entry = entry?.path();
        if entry.is_dir() {
            remove.and(can_be_removed(entry, use_atime)?);
        } else {
            let (is_old, size, this_ts) = file_is_old(&entry, use_atime);
            if !is_old {
                remove = Removable::False(entry.to_owned());
            }
            if let Removable::True { size: s, timestamp } = &mut remove {
                *s += size;
                match (timestamp, this_ts) {
                    (a @ None, other @ Some(_)) => *a = other,
                    (Some(a), Some(b)) => {
                        if b > *a {
                            *a = b;
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Removable::False(..) = remove {
            // we have at least 1 file that we can't removed, so no need to check any other files
            break;
        }
    }

    Ok(remove)
}

/// Recursively clears the read-only flag on every file in this path, and remove them
fn remove<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    for entry in WalkDir::new(path)
        .follow_links(false)
        .same_file_system(true)
        .contents_first(true)
    {
        let entry = entry?;
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                let mut perms = md.permissions();
                perms.set_readonly(false);
                set_permissions(entry.path(), perms)?;
                remove_file(entry.path())?;
            }
        } else if entry.file_type().is_dir() {
            remove_dir(entry.path())?;
        }
    }
    Ok(())
}

fn duration_to_string(ts: Option<Duration>) -> Cow<'static, str> {
    if let Some(ts) = ts {
        let s = ts.as_secs();
        if s < 100 {
            Cow::Owned(format!("{} seconds", s))
        } else if s < 6000 {
            Cow::Owned(format!("{} minutes", s / 60))
        } else if s < 6000 * 60 {
            Cow::Owned(format!("{} hours", s / 60 / 60))
        } else {
            Cow::Owned(format!("{} days", s / 60 / 60 / 24))
        }
    } else {
        Cow::Borrowed("")
    }
}

fn main() {
    let matches = App::new("scrubber")
        .version("0.0.1")
        .about("Removes unused folders from a temp directory")
        .arg(
            Arg::new("rm")
                .long("rm")
                .help("Actually remove directories"),
        )
        .arg(
            Arg::new("no-atime")
                .long("no-atime")
                .help("Don't consider atime (only look at mtime)"),
        )
        .arg(
            Arg::new("tmpdir")
                .index(1)
                .required(false)
                .help("Path to tempdir.  Defaults to $HOME/tmp or $TMPDIR/$USERNAME"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("More verbose output"),
        )
        .get_matches();

    let verbose = matches.is_present("verbose");

    let mytmp: PathBuf = if let Some(t) = matches.value_of("tmpdir") {
        let p = PathBuf::from(t);
        if !p.exists() {
            eprintln!("Tmpdir {} does not exist!", p.display());
            std::process::exit(1)
        }
        p
    } else {
        let mut mytmp: PathBuf = home_dir().expect("Unable to determine HOME directory");
        mytmp.push("tmp");

        if mytmp.exists() {
            mytmp
        } else {
            // try $TMPDIR/$USERNAME
            if let Some(tmpdir) = var_os("TMPDIR") {
                let username = get_username();
                PathBuf::from(tmpdir).join(username)
            } else {
                mytmp // even though it doesn't exist, it's still the default
            }
        }
    };

    let ok_to_remove = matches.is_present("rm");
    let use_atime = !matches.is_present("no-atime");

    if !mytmp.exists() {
        println!("{} does not exist!", mytmp.display());
        std::process::exit(1);
    }

    for entry in read_dir(&mytmp).unwrap_or_else(|_| panic!("Unable to read_dir: {:?}", mytmp)) {
        if let Ok(entry) = entry {
            let entry_path = entry.path();

            // only consider directories that seem to be a 2-digit number
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if !(name.len() == 2
                && name
                    .char_indices()
                    .all(|(idx, chr)| idx < 2 && chr.is_digit(10)))
            {
                if verbose {
                    println!("Will not examine {}", entry_path.display());
                }
                continue;
            }

            match can_be_removed(&entry_path, use_atime) {
                Ok(Removable::Always) => {
                    println!("{} is empty and will be removed", entry_path.display());
                    if let Err(e) = remove_dir(&entry_path) {
                        println!("Error removing {}: {}", entry_path.display(), e);
                    }
                }
                Ok(Removable::True { size, timestamp }) => {
                    if let Some(timestamp) = timestamp {
                        let ts_age = timestamp.elapsed();
                        println!(
                            "{} can be removed (saving {:.1} GB) -- {} old",
                            entry_path.display(),
                            size as f32 / 1000000000.0,
                            duration_to_string(ts_age.ok()),
                        );
                    } else {
                        println!(
                            "{} can be removed (saving {:.1} GB)",
                            entry_path.display(),
                            size as f32 / 1000000000.0
                        );
                    }
                    if ok_to_remove {
                        if let Err(e) = remove(&entry_path) {
                            println!("Error removing {}: {}", entry_path.display(), e);
                        }
                    }
                }
                Ok(Removable::False(why)) => {
                    println!(
                        "must save {} (because of {})",
                        entry_path.display(),
                        why.display()
                    );
                }
                Err(e) => println!("Unable to read {}: {}", entry_path.display(), e),
            }
        } else {
            println!("Warning: Unable to read {:?}", entry.err());
        }
    }
}
