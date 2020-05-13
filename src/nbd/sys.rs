#![allow(dead_code)]
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use errno::Errno;
use nix::{errno, libc::ioctl, request_code_none};
use std::io::Cursor;
use std::io::{Error, ErrorKind};
use std::os::unix::io::{AsRawFd, RawFd};
use tokio::fs::File;

static CMD_MASK_COMMAND: u32 = 0x0000_ffff;
static REQUEST_MAGIC: u32 = 0x2560_9513;
static REPLY_MAGIC: u32 = 0x6744_6698;

pub static SIZE_OF_REQUEST: usize = 28;
pub static SIZE_OF_REPLY: usize = 16;

// Flags are there
static HAS_FLAGS: u64 = 1;
// Device is read-only
static READ_ONLY: u64 = 1 << 1;
// Send FLUSH
static SEND_FLUSH: u64 = 1 << 2;
// Send FUA (Force Unit Access)
static SEND_FUA: u64 = 1 << 3;
// Use elevator algorithm - rotational media
static ROTATIONAL: u64 = 1 << 4;
// Send TRIM (discard)
static SEND_TRIM: u64 = 1 << 5;
// Send NBD_CMD_WRITE_ZEROES
static SEND_WRITE_ZEROES: u64 = 1 << 6;
// Multiple connections are okay
static CAN_MULTI_CONN: u64 = 1 << 8;

pub fn set_sock(f: &File, sock: RawFd) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 0), sock) })
        .map_err(errno_to_io)
}

pub fn set_block_size(f: &File, size: u32) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 1), size) })
        .map_err(errno_to_io)
}

pub fn do_it(f: &File) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 3)) }).map_err(errno_to_io)
}

pub fn clear_sock(f: &File) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 4)) }).map_err(errno_to_io)
}

pub fn clear_queue(f: &File) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 5)) }).map_err(errno_to_io)
}

pub fn set_size_blocks(f: &File, size: u64) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 7), size) })
        .map_err(errno_to_io)
}

pub fn disconnect(f: &File) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 8)) }).map_err(errno_to_io)
}

pub fn set_timeout(f: &File, timeout: u64) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 9), timeout) })
        .map_err(errno_to_io)
}

pub fn set_flags(f: &File, flags: u64) -> Result<i32, Error> {
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 10), flags) })
        .map_err(errno_to_io)
}

fn errno_to_io(error: nix::Error) -> Error {
    match error {
        nix::Error::Sys(errno) => Error::from_raw_os_error(errno as i32),
        nix::Error::InvalidPath => Error::from(ErrorKind::InvalidInput),
        nix::Error::InvalidUtf8 => Error::from(ErrorKind::InvalidData),
        nix::Error::UnsupportedOperation => Error::new(ErrorKind::Other, "not supported"),
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Read,
    Write,
    Disc,
    Flush,
    Trim,
    WriteZeroes,
}

#[derive(Debug, Clone)]
pub struct RequestFlags {
    // Force Unit Access
    fua: bool,
    no_hole: bool,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub magic: u32,
    pub command: Command,
    pub flags: RequestFlags,
    pub handle: u64,
    pub from: u64,
    pub len: usize,
}

impl Request {
    pub fn try_from_bytes(d: &[u8]) -> Result<Self, Error> {
        let mut rdr = Cursor::new(d);
        let magic = rdr.read_u32::<NetworkEndian>()?;
        let type_f = rdr.read_u32::<NetworkEndian>()?;
        let handle = rdr.read_u64::<NetworkEndian>()?;
        let from = rdr.read_u64::<NetworkEndian>()?;
        let len = rdr.read_u32::<NetworkEndian>()? as usize;

        if magic != REQUEST_MAGIC {
            return Err(Error::new(ErrorKind::InvalidData, "invalid magic"));
        }
        let command = match type_f & CMD_MASK_COMMAND {
            0 => Command::Read,
            1 => Command::Write,
            2 => Command::Disc,
            3 => Command::Flush,
            4 => Command::Trim,
            5 => Command::WriteZeroes,
            _ => return Err(Error::new(ErrorKind::InvalidData, "invalid command")),
        };
        let flags = type_f >> 16;
        let fua = flags & 1 == 1;
        let no_hole = flags & (1 << 1) == (1 << 1);

        Ok(Self {
            magic,
            command,
            flags: RequestFlags { fua, no_hole },
            handle,
            from,
            len,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub magic: u32,
    pub error: i32,
    pub handle: u64,
}

impl Reply {
    pub fn from_request(request: &Request) -> Self {
        Self {
            magic: REPLY_MAGIC,
            handle: request.handle,
            error: 0,
        }
    }
    pub fn append_to_vec(&self, buf: &mut Vec<u8>) -> Result<(), Error> {
        buf.write_u32::<NetworkEndian>(self.magic)?;
        buf.write_i32::<NetworkEndian>(self.error)?;
        buf.write_u64::<NetworkEndian>(self.handle)?;
        Ok(())
    }
    pub fn write_to_slice(&self, mut slice: &mut [u8]) -> Result<(), Error> {
        slice.write_u32::<NetworkEndian>(self.magic)?;
        slice.write_i32::<NetworkEndian>(self.error)?;
        slice.write_u64::<NetworkEndian>(self.handle)?;
        Ok(())
    }
}
