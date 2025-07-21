#![forbid(unsafe_code)]
//! Nyx Secure Stream layer (skeleton)

pub mod frame;
pub mod congestion;
pub mod builder;
pub mod tx;
pub mod stream_frame;
#[cfg(feature = "plugin")]
pub mod plugin;
pub mod management;
pub mod settings;
mod localized;
mod scheduler;
#[cfg(feature = "plugin")]
mod plugin_registry;
#[cfg(feature = "plugin")]
mod plugin_geostat;
pub mod capability;
mod cap_negotiator;
pub use cap_negotiator::perform_cap_negotiation;

#[cfg(feature = "mpr_experimental")]
mod mpr;
#[cfg(feature = "mpr_experimental")]
pub use mpr::MprDispatcher;

pub use frame::{FrameHeader, parse_header, parse_header_ext, FLAG_HAS_PATH_ID};
pub use builder::build_header;
pub use congestion::CongestionCtrl;
pub use tx::TxQueue;
pub use stream_frame::{StreamFrame, build_stream_frame, parse_stream_frame};
pub mod layer;
pub use layer::StreamLayer;
mod reorder;
pub use reorder::ReorderBuffer;
mod receiver;
pub use receiver::MultipathReceiver;
mod sequencer;
pub use sequencer::Sequencer;
#[cfg(feature = "plugin")]
pub use plugin::PluginHeader;
#[cfg(feature = "plugin")]
pub use plugin_registry::{PluginRegistry, PluginInfo, Permission};
#[cfg(feature = "plugin")]
pub use plugin_geostat::{GeoStat, GEO_PLUGIN_ID, plugin_info};
#[cfg(not(feature = "plugin"))]
pub struct PluginHeader;
#[cfg(not(feature = "plugin"))]
pub struct PluginInfo;
#[cfg(not(feature = "plugin"))]
pub struct PluginRegistry;
pub use capability::{Capability, FLAG_REQUIRED, encode_caps, decode_caps, negotiate, NegotiationError};

pub use management::{PingFrame, PongFrame, build_ping_frame, parse_ping_frame, build_pong_frame, parse_pong_frame,
    CloseFrame, build_close_frame, parse_close_frame,
    build_close_unsupported_cap,
    PathChallengeFrame, PathResponseFrame, build_path_challenge_frame, build_path_response_frame, parse_path_challenge_frame, parse_path_response_frame,
    Setting, SettingsFrame, build_settings_frame, parse_settings_frame};

pub use settings::{StreamSettings, settings_watch};

pub use localized::{LocalizedStringFrame, build_localized_string_frame, parse_localized_string_frame};
pub use scheduler::WeightedRrScheduler;
