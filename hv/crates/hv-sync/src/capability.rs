use crate::cell::{ArcRef, ArcRefMut};

#[derive(Debug)]
pub struct BorrowError {
    _private: (),
}

#[derive(Debug)]
pub struct BorrowMutError {
    _private: (),
}

pub enum Capability<T> {
    Empty,
    Read(ArcRef<T>),
    Write(ArcRefMut<T>),
}

impl<T> Default for Capability<T> {
    fn default() -> Self {
        Self::Empty
    }
}

impl<T> Capability<T> {
    pub fn read(&mut self, provider: ArcRef<T>) {
        *self = Capability::Read(provider);
    }

    pub fn write(&mut self, provider: ArcRefMut<T>) {
        *self = Capability::Write(provider);
    }

    pub fn take(&mut self) -> Self {
        core::mem::take(self)
    }

    pub fn get(&self) -> &T {
        self.try_get().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.try_get_mut().unwrap()
    }

    pub fn try_get(&self) -> Result<&T, BorrowError> {
        match self {
            Self::Empty => Err(BorrowError { _private: () }),
            Self::Read(read) => Ok(read),
            Self::Write(write) => Ok(write),
        }
    }

    pub fn try_get_mut(&mut self) -> Result<&mut T, BorrowError> {
        match self {
            Self::Empty | Self::Read(_) => Err(BorrowError { _private: () }),
            Self::Write(write) => Ok(write),
        }
    }

    pub fn is_readable(&self) -> bool {
        match self {
            Self::Read(_) | Self::Write(_) => true,
            Self::Empty => false,
        }
    }

    pub fn is_writable(&self) -> bool {
        match self {
            Self::Write(_) => true,
            Self::Read(_) | Self::Empty => false,
        }
    }
}
