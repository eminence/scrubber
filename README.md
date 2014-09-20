Scrubber
========

A simple tool to delete old file and directories in a temp directory.

You must always specify a temp dir (there is no default).
The default old age threshold is 30 days, but this is configurable via
the --age argument (`--age=2months for example`).

Both the modified time (mtime) and accessed time (atime) must be older
than the age threshold.  If your temp directory is on its own filesystem,
you might consider turning on atime if you can afford the performance 
impact.

The deletion rules are the following:

 * A directory will only be deleted if *all* of its contents will be deleted.  Thus
 if you have a sub-directory that has related files, none of the files in that directory will
 be deleted until everything goes past the oldage threshold.
 * The exception to the above rule is for files in the root temp directory.  These files may
 be deleted without requiring all the files to be expired.
 * The root temp directory is never deleted.


Example
=======


Here is an example of how the deletion behavior works.  Assume that `~/tmp/` is the root temp dir.
Files marked as "expired" mean both their mtime and atime are past the oldage threshold

    + ~/tmp
    +-- foo/
    |  +-- fileA.txt    // Expired
    |  \-- fileB.txt
    +-- bar/
    |  \-- fileC.txt    // Expired
    \-- fileD.txt       // Expired


fileC.txt, fileD.txt and the directory bar will all be deleted.
fileA.txt, fileB.exe and the directory foo will *not* be deleted.
