// [File 15] Optimizer struct, OptimizerRule trait, rule runner
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 6 of 15                       │
// │ Prerequisites: expr crate complete (steps 1-5)      │
// │ Next: optimizer/src/constant_folding.rs (step 7)    │
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/optimizer.rs

use expr::logical_plan::LogicalPlan;
use std::fmt;
use std::sync::Arc;

/// Error type for optimization failures.
#[derive(Debug)]
pub enum OptimizerError {
    /// A rule encountered an invalid plan structure.
    InvalidPlan(String),
    /// Generic internal error.
    Internal(String),
}

impl fmt::Display for OptimizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan(msg) => write!(f, "Invalid plan: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

/// A single optimization rule that rewrites a LogicalPlan.
///
/// DataFusion ref: `OptimizerRule` trait in datafusion/optimizer/src/optimizer.rs
pub trait OptimizerRule {
    /// Human-readable name for logging/debugging.
    fn name(&self) -> &str;

    /// Attempt to rewrite the plan. Return the original plan unchanged if the rule doesn't apply.
    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError>;
}

/// The top-level optimizer that applies a sequence of rules in multiple passes.
pub struct Optimizer {
    pub rules: Vec<Arc<dyn OptimizerRule>>,
    pub max_passes: usize,
}

impl Optimizer {
    pub fn new(rules: Vec<Arc<dyn OptimizerRule>>) -> Self {
        Self {
            rules,
            max_passes: 16,
        }
    }

    pub fn with_max_passes(mut self, max_passes: usize) -> Self {
        self.max_passes = max_passes;
        self
    }

    /// Run all rules repeatedly until the plan stabilizes or max_passes is reached.
    pub fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Loop up to self.max_passes times. In each pass, apply every rule
        // in sequence to the plan. If the plan didn't change during a full pass,
        // break early (it has stabilized). Return the final plan.
        //
        // Hint: you'll need PartialEq on LogicalPlan to detect stabilization,
        // or track a "changed" flag returned from each rule.
        Ok(plan)
    }
}
