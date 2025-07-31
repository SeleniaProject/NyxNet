#![forbid(unsafe_code)]
//! Nyx Secure Stream layer (skeleton)

// pub mod frame_handler; // Temporarily disabled due to compilation issues
pub mod simple_frame_handler;
pub mod flow_controller;
//pub mod integrated_frame_processor; // Temporarily disabled for realistic benchmarks
pub mod errors;

pub mod frame;
pub mod congestion;
pub mod builder;
pub mod tx;
pub mod stream_frame;
pub mod ack;
pub mod state;
//pub mod async_stream;  // Disabled during debugging  
pub mod error_handler;
pub mod resource_manager;
#[cfg(feature = "plugin")]
pub mod plugin;
#[cfg(feature = "plugin")]
mod plugin_ipc;
#[cfg(feature = "dynamic_plugin")]
#[cfg_attr(target_os = "linux",   path = "plugin_sandbox.rs")]
#[cfg_attr(target_os = "windows", path = "plugin_sandbox_windows.rs")]
#[cfg_attr(target_os = "macos",   path = "plugin_sandbox_macos.rs")]
pub mod plugin_sandbox;
pub mod management;
pub mod settings;
mod localized;
mod scheduler;
mod capability;
mod cap_negotiator;
#[cfg(feature = "plugin")]
mod plugin_registry;
#[cfg(feature = "plugin")]
mod plugin_geostat;
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
pub use ack::{AckFrame, build_ack_frame, parse_ack_frame, AckGenerator};
pub use state::{Stream, StreamState};
pub mod layer;
pub use layer::StreamLayer;
mod reorder;
pub use reorder::ReorderBuffer;
mod receiver;
pub use receiver::MultipathReceiver;
mod sequencer;
pub use sequencer::Sequencer;
//pub use async_stream::{NyxAsyncStream, StreamError, StreamState as AsyncStreamState, CleanupConfig, StreamStats};  // Disabled during debugging
// pub use frame_handler::{FrameHandler, FrameHandlerError, FrameValidation, ReassembledData, FrameHandlerStats}; // Temporarily disabled
//pub use integrated_frame_processor::{IntegratedFrameProcessor, StreamContext}; // Temporarily disabled
pub use flow_controller::{FlowController, FlowControlError, FlowControlStats, CongestionState, RttEstimator};
pub use error_handler::{StreamErrorHandler, ErrorContext, ErrorCategory, ErrorSeverity, RecoveryStrategy, RecoveryAction, ErrorHandlerStats};
pub use resource_manager::{ResourceManager, ResourceInfo, ResourceType, ResourceError, ResourceLimits, ResourceStats};
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

pub use capability::{Capability, FLAG_REQUIRED, encode_caps, decode_caps, negotiate, NegotiationError, LOCAL_CAP_IDS};

pub use management::{PingFrame, PongFrame, build_ping_frame, parse_ping_frame, build_pong_frame, parse_pong_frame,
    CloseFrame, build_close_frame, parse_close_frame,
    build_close_unsupported_cap,
    PathChallengeFrame, PathResponseFrame, build_path_challenge_frame, build_path_response_frame, parse_path_challenge_frame, parse_path_response_frame,
    Setting, SettingsFrame, build_settings_frame, parse_settings_frame};

pub use settings::{StreamSettings, settings_watch};

pub use localized::{LocalizedStringFrame, build_localized_string_frame, parse_localized_string_frame};
pub use scheduler::WeightedRrScheduler;

#[cfg(feature = "hpke")]
pub mod hpke_rekey;
#[cfg(feature = "hpke")]
pub use hpke_rekey::{RekeyFrame, build_rekey_frame, parse_rekey_frame, seal_for_rekey, open_rekey};

#[cfg(feature = "plugin")]
pub mod plugin_dispatch;

#[cfg(test)]
mod tests;
