#![forbid(unsafe_code)]
//! Nyx Secure Stream layer (skeleton)

pub mod frame;
pub mod congestion;
pub mod builder;
pub mod tx;
pub mod stream_frame;
pub mod plugin;
pub mod management;
pub mod settings;
mod localized;

pub use frame::{FrameHeader, parse_header};
pub use builder::build_header;
pub use congestion::CongestionCtrl;
pub use tx::TxQueue;
pub use stream_frame::{StreamFrame, build_stream_frame, parse_stream_frame};
pub mod layer;
pub use layer::StreamLayer;
mod reorder;
pub use reorder::ReorderBuffer;
pub use plugin::PluginHeader;

pub use management::{PingFrame, PongFrame, build_ping_frame, parse_ping_frame, build_pong_frame, parse_pong_frame,
    CloseFrame, build_close_frame, parse_close_frame,
    PathChallengeFrame, PathResponseFrame, build_path_challenge_frame, build_path_response_frame, parse_path_challenge_frame, parse_path_response_frame,
    Setting, SettingsFrame, build_settings_frame, parse_settings_frame};

pub use settings::{StreamSettings, settings_watch};

pub use localized::{LocalizedStringFrame, build_localized_string_frame, parse_localized_string_frame};
