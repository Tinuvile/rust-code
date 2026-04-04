//! Built-in skills bundled with the binary.
//!
//! Ref: src/skills/bundled/ (batch, debug, loop, verify, simplify, etc.)

use crate::skill::{Skill, SkillContext, SkillSource};

// ── Individual skills ─────────────────────────────────────────────────────────

pub fn batch_skill() -> Skill {
    Skill {
        name: "batch".into(),
        description: "Process a list of items one by one, applying the same operation to each.".into(),
        aliases: vec![],
        when_to_use: "Use this skill when the user asks you to apply the same task to a list of files, items, or cases.".into(),
        content: "You will be given a list of items and a task to perform on each. Process them \
one at a time. For each item, perform the requested operation, confirm it succeeded, then move on \
to the next. Report a summary when done.".into(),
        allowed_tools: vec![],
        model: None,
        user_invocable: true,
        context: SkillContext::Inline,
        source: SkillSource::Bundled,
        argument_hint: Some("<task> on <items>".into()),
    }
}

pub fn debug_skill() -> Skill {
    Skill {
        name: "debug".into(),
        description: "Systematically debug a failing test, error, or unexpected behaviour.".into(),
        aliases: vec!["fix".into()],
        when_to_use: "Use this skill when you need to diagnose and fix a bug, failing test, or error message.".into(),
        content: "Follow this debugging process:\n\
1. Read the error message carefully. Identify the file and line number.\n\
2. Read the relevant source file(s).\n\
3. Form a hypothesis about the root cause.\n\
4. Make the minimal change needed to fix it.\n\
5. Run the test/command again to confirm the fix.\n\
6. If still failing, re-read the error and repeat from step 2.\n\
Do NOT make speculative changes or refactor unrelated code.".into(),
        allowed_tools: vec![],
        model: None,
        user_invocable: true,
        context: SkillContext::Inline,
        source: SkillSource::Bundled,
        argument_hint: Some("<error or test name>".into()),
    }
}

pub fn loop_skill() -> Skill {
    Skill {
        name: "loop".into(),
        description: "Repeat a task until a condition is met or a limit is reached.".into(),
        aliases: vec![],
        when_to_use: "Use when the user wants to iterate on a task until the output satisfies some criterion.".into(),
        content: "You will repeat the requested task in a loop. After each iteration, check whether \
the stopping condition has been met. If yes, report the final result. If no, adjust your approach \
based on the previous attempt and try again. Stop after at most 10 iterations to prevent runaway loops.".into(),
        allowed_tools: vec![],
        model: None,
        user_invocable: true,
        context: SkillContext::Inline,
        source: SkillSource::Bundled,
        argument_hint: Some("<task> until <condition>".into()),
    }
}

pub fn verify_skill() -> Skill {
    Skill {
        name: "verify".into(),
        description: "Run the project's test suite and report pass/fail.".into(),
        aliases: vec!["test".into()],
        when_to_use: "Use after making code changes to confirm nothing is broken.".into(),
        content: "Run the project's test suite. Report which tests pass and which fail. \
For any failure, show the test name, failure message, and the relevant source line. \
Do not modify code — only observe and report.".into(),
        allowed_tools: vec!["Bash".into(), "Read".into(), "Glob".into()],
        model: None,
        user_invocable: true,
        context: SkillContext::Fork,
        source: SkillSource::Bundled,
        argument_hint: None,
    }
}

pub fn simplify_skill() -> Skill {
    Skill {
        name: "simplify".into(),
        description: "Refactor code to be simpler without changing behaviour.".into(),
        aliases: vec![],
        when_to_use: "Use when the user wants to reduce complexity of existing code.".into(),
        content: "Simplify the indicated code. Rules:\n\
- Do not change public API or behaviour.\n\
- Remove dead code and unnecessary abstractions.\n\
- Prefer flat over nested.\n\
- Prefer standard library functions over custom helpers.\n\
- Run tests after to confirm nothing broke.".into(),
        allowed_tools: vec![],
        model: None,
        user_invocable: true,
        context: SkillContext::Inline,
        source: SkillSource::Bundled,
        argument_hint: Some("<file or function>".into()),
    }
}

pub fn remember_skill() -> Skill {
    Skill {
        name: "remember".into(),
        description: "Save important information to the memory system.".into(),
        aliases: vec![],
        when_to_use: "Use when the user explicitly asks to remember something for future sessions.".into(),
        content: "Save the specified information to the user's memory system. Write a concise \
markdown file under `~/.claude/projects/` with the appropriate memory type frontmatter \
(user / feedback / project / reference). Update MEMORY.md to index the new entry.".into(),
        allowed_tools: vec!["Read".into(), "Write".into(), "Edit".into()],
        model: None,
        user_invocable: true,
        context: SkillContext::Inline,
        source: SkillSource::Bundled,
        argument_hint: Some("<what to remember>".into()),
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Return all built-in skills.
pub fn all_bundled_skills() -> Vec<Skill> {
    vec![
        batch_skill(),
        debug_skill(),
        loop_skill(),
        verify_skill(),
        simplify_skill(),
        remember_skill(),
    ]
}
