mod util;
mod async_io;

use std::path::PathBuf;
use argh::FromArgs;


/// File read tester
#[derive(FromArgs)]
pub struct Options {
    /// test file
    #[argh(positional)]
    target: PathBuf,

    /// io mode
    #[argh(option, from_str_fn(parse_io_mode))]
    mode: IoMode,

    /// access mode
    #[argh(option, from_str_fn(parse_access_mode))]
    access: AccessMode,

    /// buffer size
    #[argh(option, default = "4096")]
    bufsize: usize,

    /// read count
    #[argh(option, default = "1")]
    count: usize,

    /// random seed
    #[argh(option, from_str_fn(parse_hex))]
    seed: Option<u64>
}

#[derive(Clone, Copy)]
pub enum IoMode {
    AsyncBuffered,
    SyncBuffered,
    AsyncDirect,
    SyncDirect
}

pub enum AccessMode {
    Sequence,
    Random
}

fn main() -> anyhow::Result<()> {
    let options: Options = argh::from_env();

    match options.mode {
        IoMode::AsyncBuffered | IoMode::AsyncDirect => async_io::main(&options)?,
        _ => todo!()
    }

    Ok(())
}

fn parse_io_mode(value: &str) -> Result<IoMode, String> {
    match value {
        "ab" | "async-buffered" => Ok(IoMode::AsyncBuffered),
        "sb" | "sync-buffered" => Ok(IoMode::SyncBuffered),
        "ad" | "async-direct" => Ok(IoMode::AsyncDirect),
        "sd" | "sync-direct" => Ok(IoMode::SyncDirect),
        _ => Err("bad mode".into())
    }
}

fn parse_access_mode(value: &str) -> Result<AccessMode, String> {
    match value {
        "seq" | "sequence" => Ok(AccessMode::Sequence),
        "rand" | "random" => Ok(AccessMode::Random),
        _ => Err("bad mode".into())
    }
}

fn parse_hex(value: &str) -> Result<u64, String> {
    if let Some(value) = value.strip_prefix("0x") {
        let mut buf = [0; 8];
        if data_encoding::HEXLOWER_PERMISSIVE.decode_mut(value.as_bytes(), &mut buf).is_ok() {
            Ok(u64::from_le_bytes(buf))
        } else {
            Err("bad hex".into())
        }
    } else {
        Err("bad hex".into())
    }
}
