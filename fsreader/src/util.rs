use std::fs;
use std::rc::Rc;
use std::os::unix::io::{ AsRawFd, RawFd };
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

    let mut queue = Vec::with_capacity(iter as usize * options.count);

    for _ in 0..options.count {
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
