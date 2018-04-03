extern crate time;
extern crate clap;

use std::env::home_dir;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, remove_file, remove_dir_all};
use std::time::{SystemTime, Duration};

fn file_is_old<P: AsRef<Path>>(f: P) -> bool {
    let f: &Path = f.as_ref();
    let old = Duration::from_secs(60 * 60 * 24 * 21);
    let now = SystemTime::now();
    if let Ok(md) = f.metadata() {
        let mda = md.accessed().ok();
        let mdm = md.modified().ok();

        let keep_due_to_mtime: bool = mdm.map_or(true, |t| now.duration_since(t).map(|t| t < old).unwrap_or(true));
        let keep_due_to_atime: bool = mda.map_or(true, |t| now.duration_since(t).map(|t| t < old).unwrap_or(true));


        if keep_due_to_mtime || keep_due_to_atime {
            false
        } else {
            //println!("{:?} is old", f);
            true
        }
    } else {
        println!("Warning: unable to get metadata for entry {:?}", f);
        false
    }
}

enum Removable {
    True,
    /// If this path can't be removed, include why
    False(PathBuf)
}

impl Removable {
    fn as_bool(&self) -> bool {
        match *self {
            Removable::True => true,
            Removable::False(_) => false,
        }
    }
    fn and(&mut self, other: Removable) {
        if let Removable::False(_) = *self {
            return;
        }
        if let Removable::False(thing) = other {
            *self = Removable::False(thing);
        }
    }
}

fn can_be_removed<P: AsRef<Path>>(dir: P) -> Removable {
    let dir = dir.as_ref();

    if dir.is_file() {
        return match file_is_old(dir) {
            True => Removable::True,
            False => Removable::False(dir.to_owned())
        }
    } // else is_dir


    let mut remove = Removable::True;
    for entry in read_dir(dir).unwrap() {
        let entry = entry.unwrap().path();
        if entry.is_dir() {
            remove.and(can_be_removed(entry));
        } else {
            if !file_is_old(&entry) {
                remove = Removable::False(entry.to_owned());
//                println!("{:?} has been modified recently", entry);
            }
        }
        if !remove.as_bool() {
            break;
        }
    }

    remove
}

fn remove<P: AsRef<Path>>(path: P) {
    let path = path.as_ref();
    if path.is_dir() {
        remove_dir_all(path).unwrap();
    } else {
        remove_file(path).unwrap();
    }

}

fn main() {
    use clap::{App, Arg};

    let matches =
        App::new("scrubber")
        .version("0.0.1")
        .about("Removes unused folders from a temp directory")
        .arg(Arg::with_name("rm")
             .long("rm")
             .help("Actually remove directories"))
        .arg(Arg::with_name("tmpdir")
             .index(1)
             .required(false)
             .help("Path to tempdir.  Defaults to $HOME/tmp"))
        .get_matches();


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
        mytmp
    };


    let ok_to_remove = matches.is_present("rm");

    for entry in read_dir(&mytmp).unwrap_or_else(|_| panic!("Unable to read_dir: {:?}", mytmp)) {
        if let Ok(entry) = entry {
            let entry_path = entry.path();

            // only consider directories that seem to be a 2-digit number
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if !(name.len() == 2 &&
                 name.char_indices().all(|(idx, chr)| idx < 2 && chr.is_digit(10))) {
                println!("Will not examine {}", entry_path.display());
                continue;
            }


            match can_be_removed(&entry_path) {
                Removable::True => {
                    println!("{} can be removed", entry_path.display());
                    if ok_to_remove {
                        remove(&entry_path);
                    }
                }
                Removable::False(why) => {
                    println!("must save {} (because of {})", entry_path.display(), why.display());
                }
            }
        } else {
            println!("Warning: Unable to read {:?}", entry.err());
        }

    }



}
