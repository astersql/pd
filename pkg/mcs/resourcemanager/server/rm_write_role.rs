// Copyright 2026 AsterSQL.
/// Persistence ownership shared by classic fallback, PD metadata and RM token hosts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteRole {
    LegacyAll,
    PdMetaOnly,
    RmTokenOnly,
}
impl WriteRole {
    pub fn metadata(self) -> bool {
        matches!(self, Self::LegacyAll | Self::PdMetaOnly)
    }
    pub fn state(self) -> bool {
        matches!(self, Self::LegacyAll | Self::RmTokenOnly)
    }
}
