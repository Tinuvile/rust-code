//! Permission system: modes, rule evaluation, dangerous pattern detection.
//!
//! Ref: src/utils/permissions/permissions.ts, src/utils/permissions/PermissionMode.ts,
//!      src/tools/BashTool/bashSecurity.ts

pub mod evaluator;
pub mod mode;
pub mod rule;
pub mod rule_parser;
pub mod bash_classifier;
pub mod dangerous_patterns;
pub mod path_validation;
pub mod denial_tracking;
pub mod persistence;
pub mod auto_classifier;
