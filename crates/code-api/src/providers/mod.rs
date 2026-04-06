//! Multi-provider LLM client implementations.
//!
//! Each sub-module implements `LlmProvider` for a specific API:
//!   - `anthropic` — Anthropic Messages API (existing client, adapted)
//!   - `openai` — OpenAI Chat Completions API
//!   - `gemini` — Google Gemini API
//!   - `openai_compat` — Generic OpenAI-compatible (DeepSeek, Kimi, Minimax, etc.)

pub mod anthropic;
pub mod openai;
pub mod gemini;
pub mod openai_compat;
pub mod registry;
