//! A cross-platform interface to the filesystem.
//!
//! Heavily based on the `ggez` crate's `filesystem` module; see source for (MIT) licensing info.
//!
//! This module provides access to files in specific places:
//!
//! * The `resources/` subdirectory in the same directory as the program executable, if any,
//! * The `resources.zip` file in the same directory as the program executable, if any,
//! * The root folder of the  game's "save" directory which is in a platform-dependent location,
//!   such as `~/.local/share/<gameid>/` on Linux. Some platforms such as Windows also incorporate
//!   the `author` string into the path.
//!
//! These locations will be searched for files in the order listed, and the first file found used.
//! That allows game assets to be easily distributed as an archive file, but locally overridden for
//! testing or modding simply by putting altered copies of them in the game's `resources/`
//! directory.  It is loosely based off of the `PhysicsFS` library.
//!
//! Note that the file lookups WILL follow symlinks!  This module's directory isolation is intended
//! for convenience, not security, so don't assume it will be secure.

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

extern crate hv_vfs as vfs;

use anyhow::*;
use directories::ProjectDirs;
use hv_alchemy::TypedMetaTable;
use hv_lua::{ExternalError, ExternalResult, UserData, UserDataMethods};
use std::{
    env, fmt,
    io::{self, Read},
    path::{self, Path, PathBuf},
};
use vfs::Vfs;

pub use vfs::OpenOptions;

// const CONFIG_NAME: &str = "/conf.toml";

/// A structure that contains the filesystem state and cache.
#[derive(Debug)]
pub struct Filesystem {
    vfs: vfs::OverlayFS,
}

/// Represents a file, either in the filesystem, or in the resources zip file,
/// or whatever.
#[non_exhaustive]
pub enum File {
    /// A wrapper for a VFile trait object.
    VfsFile(Box<dyn vfs::VFile>),
}

impl fmt::Debug for File {
    // Make this more useful?
    // But we can't seem to get a filename out of a file,
    // soooooo.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            File::VfsFile(ref _file) => write!(f, "VfsFile"),
        }
    }
}

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match *self {
            File::VfsFile(ref mut f) => f.read(buf),
        }
    }
}

impl io::Seek for File {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match *self {
            File::VfsFile(ref mut f) => f.seek(pos),
        }
    }
}

impl io::Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match *self {
            File::VfsFile(ref mut f) => f.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match *self {
            File::VfsFile(ref mut f) => f.flush(),
        }
    }
}

impl Default for Filesystem {
    fn default() -> Self {
        Self::new()
    }
}

impl Filesystem {
    /// Construct a new virtual filesystem with a completely empty root set.
    pub fn new() -> Self {
        Self {
            vfs: vfs::OverlayFS::new(),
        }
    }

    /// Create a new `Filesystem` instance, using the given `id` and (on
    /// some platforms) the `author` as a portion of the user
    /// directory path.
    pub fn from_project_dirs(path_offset: &Path, id: &str, author: &str) -> Result<Filesystem> {
        let mut root_path = env::current_exe()?;

        // Ditch the filename (if any)
        if root_path.file_name().is_some() {
            let _ = root_path.pop();
        }

        root_path.push(path_offset);

        // Set up VFS to merge resource path, root path, and zip path.
        let mut overlay = vfs::OverlayFS::new();

        // overlay.push_back(Box::new(vfs::ZipFs::from_read(
        //     io::Cursor::new(include_bytes!("../resources/scripts.zip")),
        //     Some(PathBuf::from("hv-core/resources/scripts")),
        // )?));

        let mut resources_path;
        let mut resources_zip_path;
        let user_data_path;
        let user_config_path;

        let project_dirs = match ProjectDirs::from("", author, id) {
            Some(dirs) => dirs,
            None => bail!("could not determine valid project directories"),
        };

        // <game exe root>/resources/
        {
            resources_path = root_path.clone();
            resources_path.push("resources");
            log::trace!("Resources path: {:?}", resources_path);
            let physfs = vfs::PhysicalFs::new(&resources_path, true);
            overlay.push_back(Box::new(physfs));
        }

        // <cargo manifest root>/resources/
        if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            resources_path = PathBuf::from(manifest_dir);
            resources_path.push(&path_offset);
            resources_path.push("resources");
            log::trace!("Cargo manifest resources path: {:?}", resources_path);
            let physfs = vfs::PhysicalFs::new(&resources_path, true);
            overlay.push_back(Box::new(physfs));
        }

        // <root>/resources.zip
        {
            resources_zip_path = root_path;
            resources_zip_path.push("resources.zip");
            if resources_zip_path.exists() {
                log::trace!("Resources zip file: {:?}", resources_zip_path);
                let zipfs = vfs::ZipFs::new(&resources_zip_path)?;
                overlay.push_back(Box::new(zipfs));
            } else {
                log::trace!("No resources zip file found");
            }
        }

        // Per-user data dir,
        // ~/.local/share/whatever/
        {
            user_data_path = project_dirs.data_local_dir();
            log::trace!("User-local data path: {:?}", user_data_path);
            let physfs = vfs::PhysicalFs::new(user_data_path, true);
            overlay.push_back(Box::new(physfs));
        }

        // Writeable local dir, ~/.config/whatever/
        // Save game dir is read-write
        {
            user_config_path = project_dirs.config_dir();
            log::trace!("User-local configuration path: {:?}", user_config_path);
            let physfs = vfs::PhysicalFs::new(user_config_path, false);
            overlay.push_back(Box::new(physfs));
        }

        let fs = Filesystem { vfs: overlay };

        Ok(fs)
    }

    /// Opens the given `path` and returns the resulting `File`
    /// in read-only mode.
    pub fn open<P: AsRef<path::Path>>(&mut self, path: P) -> Result<File> {
        self.vfs.open(path.as_ref()).map(|f| File::VfsFile(f))
    }

    /// Opens a file in the user directory with the given
    /// [`filesystem::OpenOptions`](struct.OpenOptions.html).
    /// Note that even if you open a file read-write, it can only
    /// write to files in the "user" directory.
    pub fn open_options<P: AsRef<path::Path>>(
        &mut self,
        path: P,
        options: OpenOptions,
    ) -> Result<File> {
        self.vfs
            .open_options(path.as_ref(), options)
            .map(|f| File::VfsFile(f))
            .map_err(|e| anyhow!("Tried to open {:?} but got error: {:?}", path.as_ref(), e))
    }

    /// Creates a new file in the user directory and opens it
    /// to be written to, truncating it if it already exists.
    pub fn create<P: AsRef<path::Path>>(&mut self, path: P) -> Result<File> {
        self.vfs.create(path.as_ref()).map(|f| File::VfsFile(f))
    }

    /// Create an empty directory in the user dir
    /// with the given name.  Any parents to that directory
    /// that do not exist will be created.
    pub fn create_dir<P: AsRef<path::Path>>(&mut self, path: P) -> Result<()> {
        self.vfs.mkdir(path.as_ref())
    }

    /// Deletes the specified file in the user dir.
    pub fn delete<P: AsRef<path::Path>>(&mut self, path: P) -> Result<()> {
        self.vfs.rm(path.as_ref())
    }

    /// Deletes the specified directory in the user dir,
    /// and all its contents!
    pub fn delete_dir<P: AsRef<path::Path>>(&mut self, path: P) -> Result<()> {
        self.vfs.rmrf(path.as_ref())
    }

    /// Check whether a file or directory exists.
    pub fn exists<P: AsRef<path::Path>>(&self, path: P) -> bool {
        self.vfs.exists(path.as_ref())
    }

    /// Check whether a path points at a file.
    pub fn is_file<P: AsRef<path::Path>>(&self, path: P) -> bool {
        self.vfs
            .metadata(path.as_ref())
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    /// Check whether a path points at a directory.
    pub fn is_dir<P: AsRef<path::Path>>(&self, path: P) -> bool {
        self.vfs
            .metadata(path.as_ref())
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    /// Returns a list of all files and directories in the resource directory,
    /// in no particular order.
    ///
    /// Lists the base directory if an empty path is given.
    pub fn read_dir<P: AsRef<path::Path>>(
        &mut self,
        path: P,
    ) -> Result<Box<dyn Iterator<Item = path::PathBuf>>> {
        let itr = self.vfs.read_dir(path.as_ref())?.map(|fname| {
            fname.expect("Could not read file in read_dir()?  Should never happen, I hope!")
        });
        Ok(Box::new(itr))
    }

    fn write_to_string(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        for vfs in self.vfs.roots() {
            write!(s, "Source {:?}", vfs).expect("Could not write to string; should never happen?");
            match vfs.read_dir(path::Path::new("/")) {
                Ok(files) => {
                    for itm in files {
                        write!(s, "  {:?}", itm)
                            .expect("Could not write to string; should never happen?");
                    }
                }
                Err(e) => write!(s, " Could not read source: {:?}", e)
                    .expect("Could not write to string; should never happen?"),
            }
        }
        s
    }

    /// Prints the contents of all data directories
    /// to standard output.  Useful for debugging.
    pub fn print_all(&self) {
        println!("{}", self.write_to_string());
    }

    /// Outputs the contents of all data directories,
    /// using the "info" log level of the [`log`](https://docs.rs/log/) crate.
    /// Useful for debugging.
    pub fn log_all(&self) {
        log::info!("{}", self.write_to_string());
    }

    /// Adds the given (absolute) path to the list of directories
    /// it will search to look for resources.
    ///
    /// You probably shouldn't use this in the general case, since it is
    /// harder than it looks to make it bulletproof across platforms.
    /// But it can be very nice for debugging and dev purposes, such as
    /// by pushing `$CARGO_MANIFEST_DIR/resources` to it
    pub fn mount(&mut self, path: &path::Path, readonly: bool) {
        let physfs = vfs::PhysicalFs::new(path, readonly);
        log::trace!("Mounting new path: {:?}", physfs);
        self.vfs.push_back(Box::new(physfs));
    }

    /// Adds any object that implements Read + Seek as a zip file.
    ///
    /// Note: This is not intended for system files for the same reasons as
    /// for `.mount()`. Rather, it can be used to read zip files from sources
    /// such as `std::io::Cursor::new(includes_bytes!(...))` in order to embed
    /// resources into the game's executable.
    pub fn add_zip_file<R: io::Read + io::Seek + Send + Sync + 'static>(
        &mut self,
        reader: R,
        path: Option<PathBuf>,
    ) -> Result<()> {
        let zipfs = vfs::ZipFs::from_read(reader, path)?;
        log::trace!("Adding zip file from reader");
        self.vfs.push_back(Box::new(zipfs));
        Ok(())
    }

    // /// Looks for a file named `/conf.toml` in any resource directory and
    // /// loads it if it finds it.
    // /// If it can't read it for some reason, returns an error.
    // pub fn read_config(&mut self) -> Result<conf::Conf> {
    //     let conf_path = path::Path::new(CONFIG_NAME);
    //     if self.is_file(conf_path) {
    //         let mut file = self.open(conf_path)?;
    //         let c = conf::Conf::from_toml_file(&mut file)?;
    //         Ok(c)
    //     } else {
    //         Err(GameError::ConfigError(String::from(
    //             "Config file not found",
    //         )))
    //     }
    // }

    // /// Takes a `Conf` object and saves it to the user directory,
    // /// overwriting any file already there.
    // pub fn write_config(&mut self, conf: &conf::Conf) -> Result<()> {
    //     let conf_path = path::Path::new(CONFIG_NAME);
    //     let mut file = self.create(conf_path)?;
    //     conf.to_toml_file(&mut file)?;
    //     if self.is_file(conf_path) {
    //         Ok(())
    //     } else {
    //         Err(GameError::ConfigError(format!(
    //             "Failed to write config file at {}",
    //             conf_path.to_string_lossy()
    //         )))
    //     }
    // }
}

impl UserData for Filesystem {
    fn on_metatable_init(table: TypedMetaTable<Self>) {
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("open", |_, this, path: hv_lua::String| {
            this.open(Path::new(path.to_str()?)).to_lua_err()
        });

        methods.add_method_mut(
            "open_options",
            |_, this, (path, options): (hv_lua::String, OpenOptions)| {
                this.open_options(Path::new(path.to_str()?), options)
                    .to_lua_err()
            },
        );

        methods.add_method_mut("create", |_, this, path: hv_lua::String| {
            this.create(Path::new(path.to_str()?)).to_lua_err()
        });

        methods.add_method_mut("create_dir", |_, this, path: hv_lua::String| {
            this.create_dir(Path::new(path.to_str()?)).to_lua_err()
        });

        methods.add_method_mut("delete", |_, this, path: hv_lua::String| {
            this.delete(Path::new(path.to_str()?)).to_lua_err()
        });

        methods.add_method_mut("delete_dir", |_, this, path: hv_lua::String| {
            this.delete_dir(Path::new(path.to_str()?)).to_lua_err()
        });

        methods.add_method("exists", |_, this, path: hv_lua::String| {
            Ok(this.exists(Path::new(path.to_str()?)))
        });

        methods.add_method("is_file", |_, this, path: hv_lua::String| {
            Ok(this.is_file(Path::new(path.to_str()?)))
        });

        methods.add_method("is_dir", |_, this, path: hv_lua::String| {
            Ok(this.is_dir(Path::new(path.to_str()?)))
        });

        methods.add_method_mut("read_dir", |lua, this, path: hv_lua::String| {
            this.read_dir(Path::new(path.to_str()?))
                .to_lua_err()?
                .map(|pathbuf| {
                    lua.create_string(
                        pathbuf
                            .to_str()
                            .ok_or_else(|| anyhow!("bad unicode in path").to_lua_err())?,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
        });

        methods.add_method_mut(
            "mount",
            |_, this, (path, readonly): (hv_lua::String, bool)| {
                this.mount(Path::new(path.to_str()?), readonly);
                Ok(())
            },
        );
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, TypedMetaTable<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, ()| Ok(Self::new()));
        methods.add_function(
            "from_project_dirs",
            |_, (path_offset, id, author): (hv_lua::String, hv_lua::String, hv_lua::String)| {
                Self::from_project_dirs(
                    Path::new(path_offset.to_str()?),
                    id.to_str()?,
                    author.to_str()?,
                )
                .to_lua_err()
            },
        );
    }
}

impl UserData for File {
    fn on_metatable_init(table: TypedMetaTable<Self>) {
        table
            .add::<dyn Send>()
            .add::<dyn Sync>()
            .add::<dyn io::Read>()
            .add::<dyn io::Write>()
            .add::<dyn io::Seek>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("read_to_string", |_, this, ()| {
            let mut buf = String::new();
            this.read_to_string(&mut buf).to_lua_err()?;
            Ok(buf)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        path,
    };

    fn dummy_fs_for_tests() -> Filesystem {
        let mut path = path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("resources");
        let physfs = vfs::PhysicalFs::new(&path, false);
        let mut ofs = vfs::OverlayFS::new();
        ofs.push_front(Box::new(physfs));
        Filesystem { vfs: ofs }
    }

    // Currently does not work because the files aren't there.
    // #[test]
    // fn headless_test_file_exists() {
    //     let f = dummy_fs_for_tests();

    //     let tile_file = path::Path::new("/tile.png");
    //     assert!(f.exists(tile_file));
    //     assert!(f.is_file(tile_file));

    //     let tile_file = path::Path::new("/oglebog.png");
    //     assert!(!f.exists(tile_file));
    //     assert!(!f.is_file(tile_file));
    //     assert!(!f.is_dir(tile_file));
    // }

    #[test]
    fn headless_test_read_dir() {
        let mut f = dummy_fs_for_tests();

        let dir_contents_size = f.read_dir("/").unwrap().count();
        assert!(dir_contents_size > 0);
    }

    #[test]
    fn headless_test_create_delete_file() {
        let mut fs = dummy_fs_for_tests();
        let test_file = path::Path::new("/testfile.txt");
        let bytes = "test".as_bytes();

        {
            let mut file = fs.create(test_file).unwrap();
            let _ = file.write(bytes).unwrap();
        }
        {
            let mut buffer = Vec::new();
            let mut file = fs.open(test_file).unwrap();
            let _ = file.read_to_end(&mut buffer).unwrap();
            assert_eq!(bytes, buffer.as_slice());
        }

        fs.delete(test_file).unwrap();
    }

    // #[test]
    // fn headless_test_file_not_found() {
    //     let mut fs = dummy_fs_for_tests();
    //     {
    //         let rel_file = "testfile.txt";
    //         match fs.open(rel_file) {
    //             Err(GameError::ResourceNotFound(_, _)) => (),
    //             Err(e) => panic!("Invalid error for opening file with relative path: {:?}", e),
    //             Ok(f) => panic!("Should have gotten an error but instead got {:?}!", f),
    //         }
    //     }

    //     {
    //         // This absolute path should work on Windows too since we
    //         // completely remove filesystem roots.
    //         match fs.open("/ooglebooglebarg.txt") {
    //             Err(GameError::ResourceNotFound(_, _)) => (),
    //             Err(e) => panic!("Invalid error for opening nonexistent file: {}", e),
    //             Ok(f) => panic!("Should have gotten an error but instead got {:?}", f),
    //         }
    //     }
    // }

    // #[test]
    // fn headless_test_write_config() {
    //     let mut f = dummy_fs_for_tests();
    //     let conf = conf::Conf::new();
    //     // The config file should end up in
    //     // the resources directory with this
    //     match f.write_config(&conf) {
    //         Ok(_) => (),
    //         Err(e) => panic!("{:?}", e),
    //     }
    //     // Remove the config file!
    //     f.delete(CONFIG_NAME).unwrap();
    // }
}
