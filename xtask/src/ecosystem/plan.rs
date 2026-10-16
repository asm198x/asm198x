//! Reviewed build data extends schema-1 admission without changing fetch.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BuildPlan {
    pub tools: Vec<Tool>,
    pub reference: Vec<Step>,
    pub assembler: Vec<Step>,
    pub compare: Vec<Comparison>,
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub adapters: Vec<String>,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tool {
    pub name: String,
    pub probe: Vec<String>,
    pub versions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    pub stage: String,
    pub cwd: String,
    pub argv: Vec<String>,
    pub timeout_seconds: u64,
    pub outputs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Comparison {
    pub reference: String,
    pub assembler: String,
}

impl BuildPlan {
    pub(super) fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut tools = BTreeSet::new();
        for tool in &self.tools {
            if ![
                "acme",
                "ca65",
                "ld65",
                "rgbasm",
                "rgblink",
                "rgbfix",
                "sjasmplus",
                "vasmm68k_mot",
            ]
            .contains(&tool.name.as_str())
            {
                problems.push(format!("unreviewed build tool `{}`", tool.name));
            }

            if tool.name.is_empty()
                || tool.name.contains(['/', '\\'])
                || tool.name == "asm198x"
                || !tools.insert(tool.name.as_str())
            {
                problems.push(
                    "tool names must be unique executable basenames other than asm198x".into(),
                );
            }
            if tool.probe.iter().any(|s| s.is_empty())
                || tool.versions.is_empty()
                || tool.versions.iter().any(|s| s.trim().is_empty())
            {
                problems.push(format!("{} needs explicit version identities", tool.name));
            }
        }
        for (side, steps) in [
            ("reference", &self.reference),
            ("assembler", &self.assembler),
        ] {
            if steps.is_empty() {
                problems.push(format!("{side} has no ordered build steps"));
            }
            let mut outputs = BTreeSet::new();
            for step in steps {
                if !["assemble", "link", "finalize", "control"].contains(&step.stage.as_str())
                    || !super::safe_relative(&step.cwd)
                    || !(1..=300).contains(&step.timeout_seconds)
                    || step.argv.is_empty()
                    || step.argv.iter().any(|s| s.is_empty())
                {
                    problems.push(format!("invalid {side} build step"));
                }
                if let Some(name) = step.argv.first()
                    && !(tools.contains(name.as_str()) || side == "assembler" && name == "asm198x")
                {
                    problems.push(format!("unreviewed {side} tool `{name}`"));
                }
                for path in &step.outputs {
                    if !super::safe_relative(path) || path == "." {
                        problems.push(format!("unsafe output `{path}`"));
                    }
                    outputs.insert(path.as_str());
                }
            }
            for pair in &self.compare {
                let path = if side == "reference" {
                    &pair.reference
                } else {
                    &pair.assembler
                };
                if !outputs.contains(path.as_str()) {
                    problems.push(format!(
                        "{side} comparison `{path}` is not a declared build output"
                    ));
                }
            }
        }
        if !self
            .assembler
            .iter()
            .any(|s| s.argv.first().is_some_and(|s| s == "asm198x"))
        {
            problems.push("assembler build does not invoke Asm198x".into());
        }
        if self.compare.is_empty() {
            problems.push("no exact comparison outputs".into());
        }
        let mut pairs = BTreeSet::new();
        for pair in &self.compare {
            if !super::safe_relative(&pair.reference)
                || !super::safe_relative(&pair.assembler)
                || !pairs.insert((&pair.reference, &pair.assembler))
            {
                problems.push("comparison paths must be safe and unique".into());
            }
        }
        for path in self.directories.iter().chain(&self.adapters) {
            if !super::safe_relative(path) || path == "." {
                problems.push(format!("unsafe build path `{path}`"));
            }
        }
        for issue in &self.issues {
            if !issue.starts_with("https://github.com/") || !issue.contains("/issues/") {
                problems.push(format!("invalid issue link `{issue}`"));
            }
        }
        problems
    }
}
