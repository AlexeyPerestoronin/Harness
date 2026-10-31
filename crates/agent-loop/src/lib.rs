//! Asynchronous agent loop shared by the CLI and the server.
//!
//! The crate owns vendor agents, the async [`VendorRequest`] layer built on
//! [`RequestPipeline`], agent selection, dump settings and the loop itself.
//! All user-facing I/O goes through the [`interactions::Interaction`] contract.

pub mod agent_loop;
pub mod agent_select;
pub mod agents;
pub mod dump_settings;
pub mod vendor_request;

pub use agent_loop::{
    session_dir, AgentLoop, RunObserver, RunOutcome, RunState, RunSummary,
};
pub use vendor_request::{PipelineContext, PipelineError, RequestPipeline};
