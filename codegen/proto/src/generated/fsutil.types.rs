#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Stat {
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub mode: u32,
    #[prost(uint32, tag = "3")]
    pub uid: u32,
    #[prost(uint32, tag = "4")]
    pub gid: u32,
    #[prost(int64, tag = "5")]
    pub size: i64,
    #[prost(int64, tag = "6")]
    pub mod_time: i64,
    /// int32 typeflag = 7;
    #[prost(string, tag = "7")]
    pub linkname: ::prost::alloc::string::String,
    #[prost(int64, tag = "8")]
    pub devmajor: i64,
    #[prost(int64, tag = "9")]
    pub devminor: i64,
    #[prost(map = "string, bytes", tag = "10")]
    pub xattrs: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        ::prost::alloc::vec::Vec<u8>,
    >,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Packet {
    #[prost(enumeration = "packet::PacketType", tag = "1")]
    pub r#type: i32,
    #[prost(message, optional, tag = "2")]
    pub stat: ::core::option::Option<Stat>,
    #[prost(uint32, tag = "3")]
    pub id: u32,
    #[prost(bytes = "vec", tag = "4")]
    pub data: ::prost::alloc::vec::Vec<u8>,
}
/// Nested message and enum types in `Packet`.
pub mod packet {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum PacketType {
        PacketStat = 0,
        PacketReq = 1,
        PacketData = 2,
        PacketFin = 3,
        PacketErr = 4,
    }
    impl PacketType {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                PacketType::PacketStat => "PACKET_STAT",
                PacketType::PacketReq => "PACKET_REQ",
                PacketType::PacketData => "PACKET_DATA",
                PacketType::PacketFin => "PACKET_FIN",
                PacketType::PacketErr => "PACKET_ERR",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "PACKET_STAT" => Some(Self::PacketStat),
                "PACKET_REQ" => Some(Self::PacketReq),
                "PACKET_DATA" => Some(Self::PacketData),
                "PACKET_FIN" => Some(Self::PacketFin),
                "PACKET_ERR" => Some(Self::PacketErr),
                _ => None,
            }
        }
    }
}
