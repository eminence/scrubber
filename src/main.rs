//#![feature(fs_time)]

extern crate time;

use std::env::home_dir;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, remove_file, remove_dir_all};
use std::os::unix::fs::MetadataExt;
use time::{Duration, now_utc, at_utc, Timespec};

fn file_is_old<P: AsRef<Path>>(f: P) -> bool {
    let f: &Path = f.as_ref();
    let old = Duration::weeks(3);
    let now = now_utc();
    if let Ok(md) = f.metadata() {
        let mda = at_utc(Timespec::new(md.atime() as i64, 0));
        let mdm = at_utc(Timespec::new(md.mtime() as i64, 0));

        if (now - mda < old) || (now - mdm < old) {
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


fn can_be_removed<P: AsRef<Path>>(dir: P) -> bool {
    let dir = dir.as_ref();

    if dir.is_file() {
        return file_is_old(dir);
    } // else is_dir


    let mut remove = true;
    for entry in read_dir(dir).unwrap() {
        let entry = entry.unwrap().path();
        if entry.is_dir() {
            remove = remove && can_be_removed(entry);
        } else {
            if !file_is_old(&entry) {
                remove = false;
                println!("{:?} is not old", entry);
            }
        }
        if !remove {
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
    let mut mytmp: PathBuf = home_dir().expect("Unable to determine HOME directory");
    mytmp.push("tmp");

    let ok_to_remove = std::env::args().nth(1).map_or(false, |x| x == "-go");

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


            if can_be_removed(&entry_path) {
                println!("{} can be removed", entry_path.display());
                if ok_to_remove {
                    remove(&entry_path);
                }
            } else {
                println!("must save {}", entry_path.display());
            }
        } else {
            println!("Warning: Unable to read {:?}", entry.err());
        }

    }



}
