use crate::Args;
use std::sync::Mutex;

#[derive(Clone, Copy)]
pub struct IcmpSetting {
    pub icmp_type: u8,
    pub code: u8,
    pub ignore_checksum: bool,
}

lazy_static::lazy_static! {
    pub static ref ICMP_SETTING: Mutex<Option<IcmpSetting>> = Mutex::new(None);
}

pub trait IcmpSettingSetter {
    fn set_icmp_setting(&self) -> anyhow::Result<()>;
}

impl IcmpSettingSetter for Args {
    fn set_icmp_setting(&self) -> anyhow::Result<()> {
        let mut global_setting = ICMP_SETTING
            .lock()
            .map_err(|e| anyhow::anyhow!("Cannot lock icmp setting: {e}"))?;
        let setting = IcmpSetting {
            code: self.icmp_code,
            icmp_type: self.icmp_type,
            ignore_checksum: self.icmp_ignore_checksum,
        };
        *global_setting = Some(setting);
        Ok(())
    }
}
