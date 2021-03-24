use std::fs;
use std::rc::Rc;
use std::os::unix::io::{ AsRawFd, RawFd };
use bytes::{ BufMut, buf::UninitSlice };
use ritsu::actions::io::TrustedAsRawFd;
use crate::{ Options, AccessMode };


#[derive(Clone)]
pub struct RcFile(pub Rc<fs::File>);

impl AsRawFd for RcFile {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

unsafe impl TrustedAsRawFd for RcFile {}

pub fn plan(total: u64, options: &Options) -> Vec<u64> {
    let bufsize = options.bufsize as u64;
    let iter = (total / bufsize) + (total % bufsize != 0) as u64;

    let mut queue = Vec::with_capacity(iter as usize * options.repeat);

    for _ in 0..options.repeat {
        queue.extend((0..iter).map(|p| p * bufsize));
    }

    if let AccessMode::Random = options.access {
        use rand::SeedableRng;
        use rand::seq::SliceRandom;
        use rand_chacha::ChaCha20Rng;

        let mut rng = if let Some(seed) = options.seed {
            ChaCha20Rng::seed_from_u64(seed)
        } else {
            ChaCha20Rng::from_entropy()
        };

        queue.shuffle(&mut rng);
    }

    queue
}

pub struct AlignedBuffer {
    ptr: *mut u8,
    cap: usize,
    len: usize
}

impl AlignedBuffer {
    pub fn with_capacity(cap: usize) -> AlignedBuffer {
        unsafe {
            let layout = std::alloc::Layout::from_size_align(cap, 4096).unwrap();
            let ptr = std::alloc::alloc(layout);

            AlignedBuffer { ptr, cap, len: 0 }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

unsafe impl BufMut for AlignedBuffer {
    fn remaining_mut(&self) -> usize {
        self.cap - self.len
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.len += cnt;
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        unsafe {
            UninitSlice::from_raw_parts_mut(
                self.ptr.add(self.len),
                self.cap - self.len
            )
        }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        unsafe {
            let layout = std::alloc::Layout::from_size_align(self.cap, 4096).unwrap();
            std::alloc::dealloc(self.ptr, layout);
        }
    }
}
