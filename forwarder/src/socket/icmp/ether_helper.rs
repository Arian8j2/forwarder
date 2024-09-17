use etherparse::{Icmpv4Slice, Icmpv6Slice};

/// // translates this
/// call_inner_method!(self, code_u8())
///
///
/// // to this
/// match self {
///     Self::V4(slice) => slice.code_u8(),
///     Self::V6(slice) => slice.code_u8(),
/// }
macro_rules! call_inner_method {
    ($self:expr, $($tokens:tt)*) => {
        match $self {
            Self::V4(slice) => slice.$($tokens)*,
            Self::V6(slice) => slice.$($tokens)*,
        }
    };
}

#[derive(Debug)]
pub enum IcmpSlice<'a> {
    V4(Icmpv4Slice<'a>),
    V6(Icmpv6Slice<'a>),
}

impl<'a> IcmpSlice<'a> {
    pub fn from_slice(is_icmp_v6: bool, slice: &'a [u8]) -> Option<Self> {
        if is_icmp_v6 {
            Icmpv6Slice::from_slice(slice).map(Self::V6).ok()
        } else {
            Icmpv4Slice::from_slice(slice).map(Self::V4).ok()
        }
    }

    pub fn bytes5to8(&self) -> [u8; 4] {
        call_inner_method!(self, bytes5to8())
    }

    pub fn code_u8(&self) -> u8 {
        call_inner_method!(self, code_u8())
    }

    pub fn type_u8(&self) -> u8 {
        call_inner_method!(self, type_u8())
    }

    pub fn payload(&self) -> &[u8] {
        call_inner_method!(self, payload())
    }
}
