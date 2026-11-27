//! The agent chat panel: transcript, permission prompts and the composer.

mod agent_setup;
mod agent_state;
mod assets;
mod backends;
mod chat_view;
mod clock;
mod code_block;
mod entry_view;
mod hook_list;
mod kept_notes;
mod last_models;
mod literal;
mod look;
mod permission_prompt;
mod pictures;
mod processes;
mod prompt_cache;
mod question_form;
mod question_prompt;
pub mod scale;
mod skill_menu;
mod slash;
mod spent;
mod spinner;
mod subagent_view;
mod tool_format;
mod transcript;

pub use agent_setup::{AgentSetup, Setup};
pub use assets::Assets;
pub use backends::{Backends, title as agent_title};
pub use chat_view::{ChatEvent, ChatView, Stored, asking::Question, attachments::Attachment};
pub use last_models::LastModels;
pub use look::{ChatLook, Item as ChatItem, Spot as ChatSpot, UsageShown};
pub use processes::{
    AgentProcesses, ProcessMeter, ProcessMeterEvent, Processes, ProcessesDialog,
    ProcessesDialogEvent, format_cpu, format_memory,
};
pub use spinner::Brand;
pub use transcript::Status;
