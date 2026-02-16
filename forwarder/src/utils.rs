use std::{mem::MaybeUninit, slice};

pub unsafe fn slice_sub(buffer: &mut [u8], count: usize) -> &mut [u8] {
    slice::from_raw_parts_mut(buffer.as_mut_ptr().sub(count), count + buffer.len())
}

pub fn cast_maybe_uninit(buffer: &mut [u8]) -> &mut [MaybeUninit<u8>] {
    // fucking rust with its bullshits
    unsafe { &mut *(buffer as *mut [u8] as *mut [MaybeUninit<u8>]) }
}
