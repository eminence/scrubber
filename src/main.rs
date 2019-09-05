use dirs::home_dir;
use std::env::var_os;
use std::ffi::OsString;
use std::fs::{read_dir, remove_dir, remove_file, set_permissions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

fn get_username() -> OsString {
    if cfg!(windows) {
        var_os("USERNAME").expect("Unknown username")
    } else {
        var_os("USER").expect("Unknown username")
    }
}

fn file_is_old<P: AsRef<Path>>(f: P) -> bool {
    let f: &Path = f.as_ref();
    let old = Duration::from_secs(60 * 60 * 24 * 21);
    let now = SystemTime::now();
    if let Ok(md) = f.metadata() {
        let mda = md.accessed().ok();
        let mdm = md.modified().ok();

        let keep_due_to_mtime: bool = mdm.map_or(true, |t| {
            now.duration_since(t).map(|t| t < old).unwrap_or(true)
        });
        let keep_due_to_atime: bool = mda.map_or(true, |t| {
            now.duration_since(t).map(|t| t < old).unwrap_or(true)
        });

        !(keep_due_to_mtime || keep_due_to_atime)
    } else {
        println!("Warning: unable to get metadata for entry {:?}", f);
        false
    }
}

enum Removable {
    /// A directory is always removable if it is empty
    Always,
    True,
    /// If this path can't be removed, include why
    False(PathBuf),
}

impl Removable {
    fn and(&mut self, other: Removable) {
        if let Removable::False(_) = *self {
            return;
        }
        match other {
            Removable::False(thing) => {
                *self = Removable::False(thing);
            }
            _ => *self = Removable::True,
        }
    }
}

fn can_be_removed<P: AsRef<Path>>(dir: P) -> Result<Removable, std::io::Error> {
    let dir = dir.as_ref();

    if dir.is_file() {
        return if file_is_old(dir) {
            Ok(Removable::True)
        } else {
            Ok(Removable::False(dir.to_owned()))
        };
    } // else is_dir

    let mut dirs = read_dir(dir)?.peekable();
    if dirs.peek().is_none() {
        // there are no entries in this directory, and we can delete it without prompting
        return Ok(Removable::Always);
    }

    let mut remove = Removable::True;
    for entry in dirs {
        let entry = entry?.path();
        if entry.is_dir() {
            remove.and(can_be_removed(entry)?);
        } else if !file_is_old(&entry) {
            remove = Removable::False(entry.to_owned());
        }
        if let Removable::True = remove {
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

fn main() {
    use clap::{App, Arg};

    let matches = App::new("scrubber")
        .version("0.0.1")
        .about("Removes unused folders from a temp directory")
        .arg(
            Arg::with_name("rm")
                .long("rm")
                .help("Actually remove directories"),
        )
        .arg(
            Arg::with_name("tmpdir")
                .index(1)
                .required(false)
                .help("Path to tempdir.  Defaults to $HOME/tmp or $TMPDIR/$USERNAME"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
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

            match can_be_removed(&entry_path) {
                Ok(Removable::Always) => {
                    println!("{} is empty and will be removed", entry_path.display());
                    if let Err(e) = remove_dir(&entry_path) {
                        println!("Error removing {}: {}", entry_path.display(), e);
                    }
                }
                Ok(Removable::True) => {
                    println!("{} can be removed", entry_path.display());
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
