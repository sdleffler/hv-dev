//! This is a lightly modified version of the `ggez` crate's `vfs` module;
//! see source for (MIT) licensing info.
//!
//! A virtual file system layer that lets us define multiple
//! "file systems" with various backing stores, then merge them
//! together.
//!
//! Basically a re-implementation of the C library `PhysFS`.  The
//! `vfs` crate does something similar but has a couple design
//! decisions that make it kind of incompatible with this use case:
//! the relevant trait for it has generic methods so we can't use it
//! as a trait object, and its path abstraction is not the most
//! convenient.

/*
 * The MIT License (MIT)
 *
 * Copyright (c) 2016-2017 ggez-dev
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use anyhow::*;
use hv_alchemy::TypedMetaTable;
use hv_lua::{AnyUserData, UserData, UserDataMethods};
use std::{
    collections::VecDeque,
    fmt::{self, Debug, Display},
    fs,
    io::{self, Read, Seek, Write},
    path::{self, Path, PathBuf},
    sync::RwLock,
};

mod path_clean;

use crate::path_clean::PathClean;

fn convenient_path_to_str(path: &path::Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("Invalid path format for resource: {:?}", path))
}

pub trait VFile: Read + Write + Seek + Debug + Send + Sync {}

impl<T> VFile for T where T: Read + Write + Seek + Debug + Send + Sync {}

/// Options for opening files
///
/// We need our own version of this structure because the one in
/// `std` annoyingly doesn't let you read the read/write/create/etc
/// state out of it.
#[must_use]
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    append: bool,
    truncate: bool,
}

impl OpenOptions {
    /// Create a new instance
    pub fn new() -> OpenOptions {
        Default::default()
    }

    /// Open for reading
    pub fn read(mut self, read: bool) -> OpenOptions {
        self.read = read;
        self
    }

    /// Open for writing
    pub fn write(mut self, write: bool) -> OpenOptions {
        self.write = write;
        self
    }

    /// Create the file if it does not exist yet
    pub fn create(mut self, create: bool) -> OpenOptions {
        self.create = create;
        self
    }

    /// Append at the end of the file
    pub fn append(mut self, append: bool) -> OpenOptions {
        self.append = append;
        self
    }

    /// Truncate the file to 0 bytes after opening
    pub fn truncate(mut self, truncate: bool) -> OpenOptions {
        self.truncate = truncate;
        self
    }

    fn to_fs_openoptions(self) -> fs::OpenOptions {
        let mut opt = fs::OpenOptions::new();
        let _ = opt
            .read(self.read)
            .write(self.write)
            .create(self.create)
            .append(self.append)
            .truncate(self.truncate)
            .create(self.create);
        opt
    }
}

impl UserData for OpenOptions {
    fn on_metatable_init(table: TypedMetaTable<Self>) {
        table
            .mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_function("read", |_, (this, read): (AnyUserData, bool)| {
            this.borrow_mut::<Self>()?.read = read;
            Ok(this)
        });

        methods.add_function("write", |_, (this, write): (AnyUserData, bool)| {
            this.borrow_mut::<Self>()?.write = write;
            Ok(this)
        });

        methods.add_function("create", |_, (this, create): (AnyUserData, bool)| {
            this.borrow_mut::<Self>()?.create = create;
            Ok(this)
        });

        methods.add_function("append", |_, (this, append): (AnyUserData, bool)| {
            this.borrow_mut::<Self>()?.append = append;
            Ok(this)
        });

        methods.add_function("truncate", |_, (this, truncate): (AnyUserData, bool)| {
            this.borrow_mut::<Self>()?.truncate = truncate;
            Ok(this)
        });
    }
}

pub trait Vfs: Debug + Display + Send + Sync {
    /// Open the file at this path with the given options
    fn open_options(&self, path: &Path, open_options: OpenOptions) -> Result<Box<dyn VFile>>;
    /// Open the file at this path for reading
    fn open(&self, path: &Path) -> Result<Box<dyn VFile>> {
        self.open_options(path, OpenOptions::new().read(true))
    }
    /// Open the file at this path for writing, truncating it if it exists already
    fn create(&self, path: &Path) -> Result<Box<dyn VFile>> {
        self.open_options(
            path,
            OpenOptions::new().write(true).create(true).truncate(true),
        )
    }
    /// Open the file at this path for appending, creating it if necessary
    fn append(&self, path: &Path) -> Result<Box<dyn VFile>> {
        self.open_options(
            path,
            OpenOptions::new().write(true).create(true).append(true),
        )
    }
    /// Create a directory at the location by this path
    fn mkdir(&self, path: &Path) -> Result<()>;

    /// Remove a file or an empty directory.
    fn rm(&self, path: &Path) -> Result<()>;

    /// Remove a file or directory and all its contents
    fn rmrf(&self, path: &Path) -> Result<()>;

    /// Check if the file exists
    fn exists(&self, path: &Path) -> bool;

    /// Get the file's metadata
    fn metadata(&self, path: &Path) -> Result<Box<dyn VMetadata>>;

    /// Retrieve all file and directory entries in the given directory.
    fn read_dir(&self, path: &Path) -> Result<Box<dyn Iterator<Item = Result<PathBuf>>>>;

    /// Retrieve the actual location of the VFS root, if available.
    fn to_path_buf(&self) -> Option<PathBuf>;
}

pub trait VMetadata {
    /// Returns whether or not it is a directory.
    /// Note that zip files don't actually have directories, awkwardly,
    /// just files with very long names.
    fn is_dir(&self) -> bool;
    /// Returns whether or not it is a file.
    fn is_file(&self) -> bool;
    /// Returns the length of the thing.  If it is a directory,
    /// the result of this is undefined/platform dependent.
    fn len(&self) -> u64;
    /// Returns true if `len` is zero.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A VFS that points to a directory and uses it as the root of its
/// file hierarchy.
///
/// It IS allowed to have symlinks in it!  They're surprisingly
/// difficult to get rid of.
#[derive(Clone)]
pub struct PhysicalFs {
    root: PathBuf,
    readonly: bool,
}

#[derive(Debug, Clone)]
pub struct PhysicalMetadata(fs::Metadata);

impl VMetadata for PhysicalMetadata {
    fn is_dir(&self) -> bool {
        self.0.is_dir()
    }
    fn is_file(&self) -> bool {
        self.0.is_file()
    }
    fn len(&self) -> u64 {
        self.0.len()
    }
}

/// This takes an absolute path and returns either a sanitized relative
/// version of it, or None if there's something bad in it.
///
/// What we want is an absolute path with no `..`'s in it, so, something
/// like "/foo" or "/foo/bar.txt".  This means a path with components
/// starting with a `RootDir`, and zero or more `Normal` components.
///
/// We gotta return a new path because there's apparently no real good way
/// to turn an absolute path into a relative path with the same
/// components (other than the first), and pushing an absolute `Path`
/// onto a `PathBuf` just completely nukes its existing contents.
fn sanitize_path(path: &path::Path) -> Option<PathBuf> {
    // FIXME: stop relying on `path_clean` and make our own implementation that
    // doesn't need this backslash-to-forward-slash hack. The hack is here because
    // `path_clean` is a port of a routine for UNIX systems, and doesn't know about
    // backslashes.
    let cleaned = path.clean();
    let mut c = cleaned.components();
    match c.next() {
        Some(path::Component::RootDir) => (),
        _ => return None,
    }

    fn is_normal_component(comp: path::Component) -> Option<&str> {
        match comp {
            path::Component::Normal(s) => s.to_str(),
            _ => None,
        }
    }

    // This could be done more cleverly but meh
    let mut accm = PathBuf::new();
    for component in c {
        if let Some(s) = is_normal_component(component) {
            accm.push(s)
        } else {
            return None;
        }
    }
    Some(accm)
}

impl PhysicalFs {
    pub fn new(root: &Path, readonly: bool) -> Self {
        PhysicalFs {
            root: root.into(),
            readonly,
        }
    }

    /// Takes a given path (&str) and returns
    /// a new PathBuf containing the canonical
    /// absolute path you get when appending it
    /// to this filesystem's root.
    fn to_absolute(&self, p: &Path) -> Result<PathBuf> {
        if let Some(safe_path) = sanitize_path(p) {
            let mut root_path = self.root.clone();
            root_path.push(safe_path);
            Ok(root_path)
        } else {
            bail!(
                "Path {:?} is not valid: must be an absolute path with no \
                 references to parent directories",
                p
            );
        }
    }

    /// Creates the PhysicalFS's root directory if necessary.
    /// Idempotent.
    /// This way we can not create the directory until it's
    /// actually used, though it IS a tiny bit of a performance
    /// malus.
    fn create_root(&self) -> Result<()> {
        if !self.root.exists() {
            Ok(fs::create_dir_all(&self.root)?)
        } else {
            Ok(())
        }
    }
}

impl Debug for PhysicalFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "<PhysicalFS root: {}>", self.root.display())
    }
}

impl Display for PhysicalFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root.display())
    }
}

impl Vfs for PhysicalFs {
    /// Open the file at this path with the given options
    fn open_options(&self, path: &Path, open_options: OpenOptions) -> Result<Box<dyn VFile>> {
        if self.readonly
            && (open_options.write
                || open_options.create
                || open_options.append
                || open_options.truncate)
        {
            bail!(
                "Cannot alter file {:?} in root {:?}, filesystem read-only",
                path,
                self
            );
        }
        self.create_root()?;
        let p = self.to_absolute(path)?;
        open_options
            .to_fs_openoptions()
            .open(p)
            .map(|x| Box::new(x) as Box<dyn VFile>)
            .map_err(Error::from)
    }

    /// Create a directory at the location by this path
    fn mkdir(&self, path: &Path) -> Result<()> {
        if self.readonly {
            bail!(
                "Tried to make directory {} but FS is \
                 read-only"
            );
        }
        self.create_root()?;
        let p = self.to_absolute(path)?;
        //println!("Creating {:?}", p);
        fs::DirBuilder::new()
            .recursive(true)
            .create(p)
            .map_err(Error::from)
    }

    /// Remove a file
    fn rm(&self, path: &Path) -> Result<()> {
        if self.readonly {
            bail!("Tried to remove file {} but FS is read-only");
        }

        self.create_root()?;
        let p = self.to_absolute(path)?;
        if p.is_dir() {
            fs::remove_dir(p).map_err(Error::from)
        } else {
            fs::remove_file(p).map_err(Error::from)
        }
    }

    /// Remove a file or directory and all its contents
    fn rmrf(&self, path: &Path) -> Result<()> {
        if self.readonly {
            bail!(
                "Tried to remove file/dir {} but FS is \
                 read-only"
            );
        }

        self.create_root()?;
        let p = self.to_absolute(path)?;
        if p.is_dir() {
            fs::remove_dir_all(p).map_err(Error::from)
        } else {
            fs::remove_file(p).map_err(Error::from)
        }
    }

    /// Check if the file exists
    fn exists(&self, path: &Path) -> bool {
        match self.to_absolute(path) {
            Ok(p) => p.exists(),
            _ => false,
        }
    }

    /// Get the file's metadata
    fn metadata(&self, path: &Path) -> Result<Box<dyn VMetadata>> {
        self.create_root()?;
        let p = self.to_absolute(path)?;
        p.metadata()
            .map(|m| Box::new(PhysicalMetadata(m)) as Box<dyn VMetadata>)
            .map_err(Error::from)
    }

    /// Retrieve the path entries in this path
    fn read_dir(&self, path: &Path) -> Result<Box<dyn Iterator<Item = Result<PathBuf>>>> {
        self.create_root()?;
        let p = self.to_absolute(path)?;
        // This is inconvenient because path() returns the full absolute
        // path of the bloody file, which is NOT what we want!
        // But if we use file_name() to just get the name then it is ALSO not what we want!
        // what we WANT is the full absolute file path, *relative to the resources dir*.
        // So that we can do read_dir("/foobar/"), and for each file, open it and query
        // it and such by name.
        // So we build the paths ourself.
        let direntry_to_path = |entry: &fs::DirEntry| -> Result<PathBuf> {
            let fname = entry
                .file_name()
                .into_string()
                .expect("Non-unicode char in file path?  Should never happen, I hope!");
            let mut pathbuf = PathBuf::from(path);
            pathbuf.push(fname);
            Ok(pathbuf)
        };
        let itr = fs::read_dir(p)?
            .map(|entry| direntry_to_path(&entry?))
            .collect::<Vec<_>>()
            .into_iter();
        Ok(Box::new(itr))
    }

    /// Retrieve the actual location of the VFS root, if available.
    fn to_path_buf(&self) -> Option<PathBuf> {
        Some(self.root.clone())
    }
}

/// A structure that joins several VFS's together in order.
#[derive(Debug)]
pub struct OverlayFS {
    roots: VecDeque<Box<dyn Vfs>>,
}

impl Display for OverlayFS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Overlay:")?;
        for root in &self.roots {
            writeln!(f, "\t{}", root)?;
        }

        Ok(())
    }
}

impl Default for OverlayFS {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayFS {
    pub fn new() -> Self {
        Self {
            roots: VecDeque::new(),
        }
    }

    /// Adds a new VFS to the front of the list.
    /// Currently unused, I suppose, but good to
    /// have at least for tests.
    #[allow(dead_code)]
    pub fn push_front(&mut self, fs: Box<dyn Vfs>) {
        self.roots.push_front(fs);
    }

    /// Adds a new VFS to the end of the list.
    pub fn push_back(&mut self, fs: Box<dyn Vfs>) {
        self.roots.push_back(fs);
    }

    pub fn roots(&self) -> &VecDeque<Box<dyn Vfs>> {
        &self.roots
    }
}

impl Vfs for OverlayFS {
    /// Open the file at this path with the given options
    fn open_options(&self, path: &Path, open_options: OpenOptions) -> Result<Box<dyn VFile>> {
        use std::fmt::Write;

        let mut tried: Vec<(&Box<dyn Vfs>, Error)> = vec![];

        for vfs in &self.roots {
            match vfs.open_options(path, open_options) {
                Err(e) => tried.push((vfs, e)),
                f => return f,
            }
        }

        let string_path = String::from(convenient_path_to_str(path)?);
        let mut tried_buf = String::new();
        for (vfs, err) in tried {
            writeln!(&mut tried_buf, "\t{}: {}", vfs, err)?;
        }

        bail!("could not open {}:\n{}", string_path, tried_buf);
    }

    /// Create a directory at the location by this path
    fn mkdir(&self, path: &Path) -> Result<()> {
        for vfs in &self.roots {
            match vfs.mkdir(path) {
                Err(_) => (),
                f => return f,
            }
        }
        bail!("Could not find anywhere writeable to make dir {:?}", path);
    }

    /// Remove a file
    fn rm(&self, path: &Path) -> Result<()> {
        for vfs in &self.roots {
            match vfs.rm(path) {
                Err(_) => (),
                f => return f,
            }
        }
        bail!("Could not remove file {:?}", path);
    }

    /// Remove a file or directory and all its contents
    fn rmrf(&self, path: &Path) -> Result<()> {
        for vfs in &self.roots {
            match vfs.rmrf(path) {
                Err(_) => (),
                f => return f,
            }
        }
        bail!("Could not remove file/dir {:?}", path);
    }

    /// Check if the file exists
    fn exists(&self, path: &Path) -> bool {
        for vfs in &self.roots {
            if vfs.exists(path) {
                return true;
            }
        }

        false
    }

    /// Get the file's metadata
    fn metadata(&self, path: &Path) -> Result<Box<dyn VMetadata>> {
        for vfs in &self.roots {
            match vfs.metadata(path) {
                Err(_) => (),
                f => return f,
            }
        }
        bail!("Could not get metadata for file/dir {:?}", path);
    }

    /// Retrieve the path entries in this path
    fn read_dir(&self, path: &Path) -> Result<Box<dyn Iterator<Item = Result<PathBuf>>>> {
        // This is tricky 'cause we have to actually merge iterators together...
        // Doing it the simple and stupid way works though.
        let mut v = Vec::new();
        for fs in &self.roots {
            if let Ok(rddir) = fs.read_dir(path) {
                v.extend(rddir)
            }
        }
        Ok(Box::new(v.into_iter()))
    }

    /// Retrieve the actual location of the VFS root, if available.
    fn to_path_buf(&self) -> Option<PathBuf> {
        None
    }
}

trait ZipArchiveAccess: Send + Sync {
    fn by_name(&'_ mut self, name: &str) -> zip::result::ZipResult<zip::read::ZipFile<'_>>;
    fn by_index(&'_ mut self, file_number: usize)
        -> zip::result::ZipResult<zip::read::ZipFile<'_>>;
    fn len(&self) -> usize;
}

impl<T: Read + Seek + Send + Sync> ZipArchiveAccess for zip::ZipArchive<T> {
    fn by_name(&mut self, name: &str) -> zip::result::ZipResult<zip::read::ZipFile> {
        // FIXME(sleffy): the following is an ultra-hack to get zip files to behave properly.
        // Ordinarily not a single filename which goes through this method would actually function
        // properly, as the vast majority of zip files don't contain absolute paths, but we reject
        // non-absolute paths, etc. And then in addition ZIP files use forward slashes rather than
        // backslashes so just in case we need to substitute out all the backslashes at the same
        // time as we rip off any potential preceding root slash. Ugh.
        //
        // Also, this doesn't fix the same issue with any of the other parts of the zip vfs...

        //let filename = sanitize_path(Path::new(name)).unwrap_or(PathBuf::from(&name));
        let filename = sanitize_path(Path::new(name)).ok_or(zip::result::ZipError::FileNotFound)?;
        let str_name = filename.to_str().unwrap_or(name);
        let stripped_name = str_name
            .strip_prefix('/')
            .unwrap_or(str_name)
            .replace("\\", "/");
        self.by_name(&stripped_name)
    }

    fn by_index(&mut self, file_number: usize) -> zip::result::ZipResult<zip::read::ZipFile> {
        self.by_index(file_number)
    }

    fn len(&self) -> usize {
        self.len()
    }
}

impl Debug for dyn ZipArchiveAccess {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        // Hide the contents; for an io::Cursor, this would print what is
        // likely to be megabytes of data.
        write!(f, "<ZipArchiveAccess>")
    }
}

/// A filesystem backed by a zip file.
#[derive(Debug)]
pub struct ZipFs {
    // It's... a bit jankity.
    // Zip files aren't really designed to be virtual filesystems,
    // and the structure of the `zip` crate doesn't help.  See the various
    // issues that have been filed on it by icefoxen.
    //
    // ALSO THE SEMANTICS OF ZIPARCHIVE AND HAVING ZIPFILES BORROW IT IS
    // HORRIFICALLY BROKEN BY DESIGN SO WE'RE JUST GONNA REFCELL IT AND COPY
    // ALL CONTENTS OUT OF IT AAAAA.
    source: Option<PathBuf>,
    archive: RwLock<Box<dyn ZipArchiveAccess>>,
    // We keep an index of what files are in the zip file
    // because trying to read it lazily is a pain in the butt.
    index: Vec<String>,
}

impl Display for ZipFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.source {
            write!(f, "<ZipFs({})>", source.display())
        } else {
            write!(f, "<ZipFs>")
        }
    }
}

impl ZipFs {
    pub fn new(filename: &Path) -> Result<Self> {
        let f = fs::File::open(filename)?;
        let archive = Box::new(zip::ZipArchive::new(f)?);
        ZipFs::from_boxed_archive(archive, Some(filename.into()))
    }

    /// Creates a `ZipFS` from any `Read+Seek` object, most useful with an
    /// in-memory `std::io::Cursor`. The provided path is an optional debugging tool which will be
    /// displayed with any errors to do with this `ZipFs`.
    pub fn from_read<R>(reader: R, path: Option<PathBuf>) -> Result<Self>
    where
        R: Read + Seek + Send + Sync + 'static,
    {
        let archive = Box::new(zip::ZipArchive::new(reader)?);
        ZipFs::from_boxed_archive(archive, path)
    }

    fn from_boxed_archive(
        mut archive: Box<dyn ZipArchiveAccess>,
        source: Option<PathBuf>,
    ) -> Result<Self> {
        let idx = (0..archive.len())
            .map(|i| {
                archive
                    .by_index(i)
                    .expect("Should never happen!")
                    .name()
                    .to_string()
            })
            .collect();
        Ok(Self {
            source,
            archive: RwLock::new(archive),
            index: idx,
        })
    }
}

/// A wrapper to contain a zipfile so we can implement
/// (janky) Seek on it and such.
///
/// We're going to do it the *really* janky way and just read
/// the whole `ZipFile` into a buffer, which is kind of awful but means
/// we don't have to deal with lifetimes, self-borrowing structs,
/// rental, re-implementing Seek on compressed data, making multiple zip
/// zip file objects share a single file handle, or any of that
/// other nonsense.
#[derive(Clone)]
pub struct ZipFileWrapper {
    buffer: io::Cursor<Vec<u8>>,
}

impl ZipFileWrapper {
    fn new(z: &mut zip::read::ZipFile) -> Result<Self> {
        let mut b = Vec::new();
        let _ = z.read_to_end(&mut b)?;
        Ok(Self {
            buffer: io::Cursor::new(b),
        })
    }
}

impl io::Read for ZipFileWrapper {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.buffer.read(buf)
    }
}

impl io::Write for ZipFileWrapper {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        panic!("Cannot write to a zip file!")
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Seek for ZipFileWrapper {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.buffer.seek(pos)
    }
}

impl Debug for ZipFileWrapper {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "<Zipfile>")
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct ZipMetadata {
    len: u64,
    is_dir: bool,
    is_file: bool,
}

impl ZipMetadata {
    /// Returns a ZipMetadata, or None if the file does not exist or such.
    /// This is not QUITE correct; since zip archives don't actually have
    /// directories (just long filenames), we can't get a directory's metadata
    /// this way without basically just faking it.
    ///
    /// This does make listing a directory rather screwy.
    fn new(name: &str, archive: &mut dyn ZipArchiveAccess) -> Option<Self> {
        match archive.by_name(name) {
            Err(_) => None,
            Ok(zipfile) => {
                let len = zipfile.size();
                Some(ZipMetadata {
                    len,
                    is_file: true,
                    is_dir: false, // mu
                })
            }
        }
    }
}

impl VMetadata for ZipMetadata {
    fn is_dir(&self) -> bool {
        self.is_dir
    }
    fn is_file(&self) -> bool {
        self.is_file
    }
    fn len(&self) -> u64 {
        self.len
    }
}

impl Vfs for ZipFs {
    fn open_options(&self, path: &Path, open_options: OpenOptions) -> Result<Box<dyn VFile>> {
        // Zip is readonly
        let path = convenient_path_to_str(path)?;
        if open_options.write || open_options.create || open_options.append || open_options.truncate
        {
            bail!(
                "Cannot alter file {:?} in zipfile {:?}, filesystem read-only",
                path,
                self
            );
        }
        let mut stupid_archive_borrow = self
            .archive
            .try_write()
            .expect("Couldn't borrow ZipArchive in ZipFS::open_options(); should never happen!");
        let mut f = stupid_archive_borrow.by_name(path)?;
        let zipfile = ZipFileWrapper::new(&mut f)?;
        Ok(Box::new(zipfile) as Box<dyn VFile>)
    }

    fn mkdir(&self, path: &Path) -> Result<()> {
        bail!(
            "Cannot mkdir {:?} in zipfile {:?}, filesystem read-only",
            path,
            self
        );
    }

    fn rm(&self, path: &Path) -> Result<()> {
        bail!(
            "Cannot rm {:?} in zipfile {:?}, filesystem read-only",
            path,
            self
        );
    }

    fn rmrf(&self, path: &Path) -> Result<()> {
        bail!(
            "Cannot rmrf {:?} in zipfile {:?}, filesystem read-only",
            path,
            self
        );
    }

    fn exists(&self, path: &Path) -> bool {
        let mut stupid_archive_borrow = self
            .archive
            .try_write()
            .expect("Couldn't borrow ZipArchive in ZipFS::exists(); should never happen!");
        if let Ok(path) = convenient_path_to_str(path) {
            stupid_archive_borrow.by_name(path).is_ok()
        } else {
            false
        }
    }

    fn metadata(&self, path: &Path) -> Result<Box<dyn VMetadata>> {
        let path = convenient_path_to_str(path)?;
        let mut stupid_archive_borrow = self
            .archive
            .try_write()
            .expect("Couldn't borrow ZipArchive in ZipFS::metadata(); should never happen!");
        match ZipMetadata::new(path, &mut **stupid_archive_borrow) {
            None => bail!("Metadata not found in zip file for {}", path),
            Some(md) => Ok(Box::new(md) as Box<dyn VMetadata>),
        }
    }

    /// Zip files don't have real directories, so we (incorrectly) hack it by
    /// just looking for a path prefix for now.
    #[allow(clippy::needless_collect)]
    fn read_dir(&self, path: &Path) -> Result<Box<dyn Iterator<Item = Result<PathBuf>>>> {
        let path = convenient_path_to_str(path)?;
        let itr = self
            .index
            .iter()
            .filter(|s| s.starts_with(path))
            .map(|s| Ok(PathBuf::from(s)))
            .collect::<Vec<_>>();
        Ok(Box::new(itr.into_iter()))
    }

    fn to_path_buf(&self) -> Option<PathBuf> {
        self.source.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, BufRead};

    #[test]
    fn headless_test_path_filtering() {
        // Valid pahts
        let p = path::Path::new("/foo");
        assert!(sanitize_path(p).is_some());

        let p = path::Path::new("/foo/");
        assert!(sanitize_path(p).is_some());

        let p = path::Path::new("/foo/bar.txt");
        assert!(sanitize_path(p).is_some());

        let p = path::Path::new("/");
        assert!(sanitize_path(p).is_some());

        let p = path::Path::new("/foo/../bop");
        assert!(sanitize_path(p).is_some());

        // Invalid paths
        let p = path::Path::new("../foo");
        assert!(sanitize_path(p).is_none());

        let p = path::Path::new("foo");
        assert!(sanitize_path(p).is_none());

        let p = path::Path::new("/foo/../../");
        assert!(sanitize_path(p).is_none());

        let p = path::Path::new("/../bar");
        assert!(sanitize_path(p).is_none());

        let p = path::Path::new("");
        assert!(sanitize_path(p).is_none());
    }

    #[test]
    fn headless_test_read() {
        let cargo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let fs = PhysicalFs::new(cargo_path, true);
        let f = fs.open(Path::new("/Cargo.toml")).unwrap();
        let mut bf = io::BufReader::new(f);
        let mut s = String::new();
        let _ = bf.read_line(&mut s).unwrap();
        // Trim whitespace from string 'cause it will
        // potentially be different on Windows and Unix.
        let trimmed_string = s.trim();
        assert_eq!(trimmed_string, "[package]");
    }

    #[test]
    fn headless_test_read_overlay() {
        let cargo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let fs1 = PhysicalFs::new(cargo_path, true);
        let mut f2path = PathBuf::from(cargo_path);
        f2path.push("src");
        let fs2 = PhysicalFs::new(&f2path, true);
        let mut ofs = OverlayFS::new();
        ofs.push_back(Box::new(fs1));
        ofs.push_back(Box::new(fs2));

        assert!(ofs.exists(Path::new("/Cargo.toml")));
        assert!(ofs.exists(Path::new("/lib.rs")));
        assert!(!ofs.exists(Path::new("/foobaz.rs")));
    }

    #[test]
    fn headless_test_physical_all() {
        let cargo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let fs = PhysicalFs::new(cargo_path, false);
        let testdir = Path::new("/testdir");
        let f1 = Path::new("/testdir/file1.txt");

        // Delete testdir if it is still lying around
        if fs.exists(testdir) {
            fs.rmrf(testdir).unwrap();
        }
        assert!(!fs.exists(testdir));

        // Create and delete test dir
        fs.mkdir(testdir).unwrap();
        assert!(fs.exists(testdir));
        fs.rm(testdir).unwrap();
        assert!(!fs.exists(testdir));

        let test_string = "Foo!";
        fs.mkdir(testdir).unwrap();
        {
            let mut f = fs.append(f1).unwrap();
            let _ = f.write(test_string.as_bytes()).unwrap();
        }
        {
            let mut buf = Vec::new();
            let mut f = fs.open(f1).unwrap();
            let _ = f.read_to_end(&mut buf).unwrap();
            assert_eq!(&buf[..], test_string.as_bytes());
        }

        {
            // Test metadata()
            let m = fs.metadata(f1).unwrap();
            assert!(m.is_file());
            assert!(!m.is_dir());
            assert_eq!(m.len(), 4);

            let m = fs.metadata(testdir).unwrap();
            assert!(!m.is_file());
            assert!(m.is_dir());
            // Not exactly sure what the "length" of a directory is, buuuuuut...
            // It appears to vary based on the platform in fact.
            // On my desktop, it's 18.
            // On Travis's VM, it's 4096.
            // On Appveyor's VM, it's 0.
            // So, it's meaningless.
            //assert_eq!(m.len(), 18);
        }

        {
            // Test read_dir()
            let r = fs.read_dir(testdir).unwrap();
            assert_eq!(r.count(), 1);
            let r = fs.read_dir(testdir).unwrap();
            for f in r {
                let fname = f.unwrap();
                assert!(fs.exists(&fname));
            }
        }

        {
            assert!(fs.exists(f1));
            fs.rm(f1).unwrap();
            assert!(!fs.exists(f1));
        }

        fs.rmrf(testdir).unwrap();
        assert!(!fs.exists(testdir));
    }

    #[test]
    fn headless_test_zip_files() {
        let mut finished_zip_bytes: io::Cursor<_> = {
            let zip_bytes = io::Cursor::new(vec![]);
            let mut zip_archive = zip::ZipWriter::new(zip_bytes);

            zip_archive
                .start_file("fake_file_name.txt", zip::write::FileOptions::default())
                .unwrap();
            let _bytes = zip_archive.write(b"Zip contents!").unwrap();
            zip_archive.finish().unwrap()
        };

        let _bytes = finished_zip_bytes.seek(io::SeekFrom::Start(0)).unwrap();
        let zfs = ZipFs::from_read(finished_zip_bytes, None).unwrap();

        assert!(zfs.exists(Path::new("/fake_file_name.txt")));
        assert!(!zfs.exists(Path::new("fake_file_name.txt")));

        let mut contents = String::new();
        let _bytes = zfs
            .open(Path::new("/fake_file_name.txt"))
            .unwrap()
            .read_to_string(&mut contents);
        assert_eq!(contents, "Zip contents!");
    }

    // BUGGO: TODO: Make sure all functions are tested for OverlayFS and ZipFS!!
}
