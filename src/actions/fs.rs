use std::io;
use std::fs::File;
use std::path::Path;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::FromRawFd;
use io_uring::{ types, opcode };
use crate::handle::Handle;
use crate::actions::{ action, PushError };


pub async fn open<H: Handle>(handle: H, path: &Path) -> io::Result<File> {
    let path = CString::new(path.as_os_str().as_bytes())?;

    let open_e = opcode::OpenAt::new(
        types::Fd(libc::AT_FDCWD),
        path.as_ptr()
    )
        .build();

    let (_, cqe) = unsafe {
        action(handle, path, open_e)
            .map_err(PushError::into_error)?.await
    };

    let ret = cqe.result();
    if ret >= 0 {
        Ok(unsafe {
            File::from_raw_fd(ret)
        })
    } else {
        Err(io::Error::from_raw_os_error(-ret))
    }
}
