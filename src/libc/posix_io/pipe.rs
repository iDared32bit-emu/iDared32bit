/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Pipes (`pipe()`), kept inside the emulator, as games only use them to
//! talk between their own threads.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{Error, ErrorKind, Result};
use std::rc::Rc;

/// How much a pipe holds before writes have to wait, as on iOS.
const PIPE_SIZE: usize = 16384;

#[derive(Debug, Default)]
struct Pipe {
    data: VecDeque<u8>,
    read_ends: usize,
    write_ends: usize,
}

/// One end of a pipe. Dropping it closes it.
#[derive(Debug)]
pub struct PipeEnd {
    pipe: Rc<RefCell<Pipe>>,
    is_write_end: bool,
}

impl PipeEnd {
    /// Makes a pipe, returning its read end and its write end.
    pub fn new_pair() -> (PipeEnd, PipeEnd) {
        let pipe = Rc::new(RefCell::new(Pipe {
            data: VecDeque::new(),
            read_ends: 1,
            write_ends: 1,
        }));
        (
            PipeEnd {
                pipe: pipe.clone(),
                is_write_end: false,
            },
            PipeEnd {
                pipe,
                is_write_end: true,
            },
        )
    }

    /// Whether a read or write wouldn't have to wait (for `select()`). A read
    /// gets EOF once there are no write ends, and a write fails once there
    /// are no read ends, without waiting.
    pub fn is_ready(&self) -> bool {
        let pipe = self.pipe.borrow();
        if self.is_write_end {
            pipe.read_ends == 0 || pipe.data.len() < PIPE_SIZE
        } else {
            pipe.write_ends == 0 || !pipe.data.is_empty()
        }
    }

    /// Fails with [ErrorKind::WouldBlock] if it has to wait for a write.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        assert!(!self.is_write_end);
        let mut pipe = self.pipe.borrow_mut();
        if pipe.data.is_empty() && !buf.is_empty() {
            return if pipe.write_ends == 0 {
                Ok(0)
            } else {
                Err(ErrorKind::WouldBlock.into())
            };
        }
        let count = buf.len().min(pipe.data.len());
        for (byte, value) in buf.iter_mut().zip(pipe.data.drain(..count)) {
            *byte = value;
        }
        Ok(count)
    }

    /// Writes what fits. Fails with [ErrorKind::WouldBlock] if nothing fits,
    /// or [ErrorKind::BrokenPipe] if there's no read end.
    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        assert!(self.is_write_end);
        let mut pipe = self.pipe.borrow_mut();
        if pipe.read_ends == 0 {
            return Err(Error::from(ErrorKind::BrokenPipe));
        }
        let count = buf.len().min(PIPE_SIZE - pipe.data.len());
        if count == 0 && !buf.is_empty() {
            return Err(ErrorKind::WouldBlock.into());
        }
        pipe.data.extend(&buf[..count]);
        Ok(count)
    }
}

impl Drop for PipeEnd {
    fn drop(&mut self) {
        let mut pipe = self.pipe.borrow_mut();
        if self.is_write_end {
            pipe.write_ends -= 1;
        } else {
            pipe.read_ends -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_and_close() {
        let (mut read_end, mut write_end) = PipeEnd::new_pair();
        let mut buf = [0; 4];
        assert!(!read_end.is_ready());
        assert_eq!(
            read_end.read(&mut buf).unwrap_err().kind(),
            ErrorKind::WouldBlock,
        );
        assert_eq!(write_end.write(b"abc").unwrap(), 3);
        assert!(read_end.is_ready());
        assert_eq!(read_end.read(&mut buf[..2]).unwrap(), 2);
        assert_eq!(&buf[..2], b"ab");
        drop(write_end);
        assert_eq!(read_end.read(&mut buf).unwrap(), 1);
        assert_eq!(buf[0], b'c');
        // EOF once the write end is closed and it's empty.
        assert!(read_end.is_ready());
        assert_eq!(read_end.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn full_and_broken() {
        let (read_end, mut write_end) = PipeEnd::new_pair();
        let big = vec![1; PIPE_SIZE + 10];
        assert_eq!(write_end.write(&big).unwrap(), PIPE_SIZE);
        assert!(!write_end.is_ready());
        assert_eq!(
            write_end.write(&big).unwrap_err().kind(),
            ErrorKind::WouldBlock,
        );
        drop(read_end);
        assert!(write_end.is_ready());
        assert_eq!(
            write_end.write(&big).unwrap_err().kind(),
            ErrorKind::BrokenPipe,
        );
    }
}
