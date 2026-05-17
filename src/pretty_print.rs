//! Tree pretty-printer for `LogicalPlan` and `PhysicalPlan`.
//!
//! Renders with Unicode box-drawing characters (├──, │, └──) and ANSI colors
//! when stdout is a TTY. Color is automatically disabled if stdout is piped or
//! `NO_COLOR` is set (handled by the `colored` crate).

use colored::Colorize;
use expr::logical_plan::plan::{JoinType, LogicalPlan};
use physical_plan::plan::PhysicalPlan;
use std::time::Duration;

/// Format a duration with an auto-selected unit. Tuned for OLTP latencies:
///   < 1µs  → integer ns         ("850ns")
///   < 1ms  → 1-decimal µs       ("12.3µs")
///   < 1s   → 1-decimal ms       ("1.2ms")
///   ≥ 1s   → 2-decimal seconds  ("1.50s")
pub fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{}ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.1}µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.1}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

const BRANCH: &str = "├── ";
const LAST: &str = "└── ";
const VBAR: &str = "│   ";
const SPACE: &str = "    ";

pub fn print_logical(plan: &LogicalPlan) {
    print_logical_inner(plan, "", true, true);
}

pub fn print_physical(plan: &PhysicalPlan) {
    print_physical_inner(plan, "", true, true);
}

fn print_logical_inner(plan: &LogicalPlan, prefix: &str, is_last: bool, is_root: bool) {
    print_node(prefix, is_last, is_root, &label_logical(plan));
    let children = children_logical(plan);
    let next = next_prefix(prefix, is_last, is_root);
    let n = children.len();
    for (i, child) in children.into_iter().enumerate() {
        print_logical_inner(child, &next, i + 1 == n, false);
    }
}

fn print_physical_inner(plan: &PhysicalPlan, prefix: &str, is_last: bool, is_root: bool) {
    print_node(prefix, is_last, is_root, &label_physical(plan));
    let children = children_physical(plan);
    let next = next_prefix(prefix, is_last, is_root);
    let n = children.len();
    for (i, child) in children.into_iter().enumerate() {
        print_physical_inner(child, &next, i + 1 == n, false);
    }
}

fn print_node(prefix: &str, is_last: bool, is_root: bool, label: &str) {
    let connector = if is_root {
        ""
    } else if is_last {
        LAST
    } else {
        BRANCH
    };
    println!(
        "{}{}{}",
        prefix.bright_black(),
        connector.bright_black(),
        label
    );
}

fn next_prefix(prefix: &str, is_last: bool, is_root: bool) -> String {
    if is_root {
        String::new()
    } else if is_last {
        format!("{}{}", prefix, SPACE)
    } else {
        format!("{}{}", prefix, VBAR)
    }
}

// ---------- LogicalPlan ----------

fn children_logical(plan: &LogicalPlan) -> Vec<&LogicalPlan> {
    match plan {
        LogicalPlan::Scan { .. } => vec![],
        LogicalPlan::Filter { input, .. }
        | LogicalPlan::Projection { input, .. }
        | LogicalPlan::Sort { input, .. }
        | LogicalPlan::Aggregate { input, .. }
        | LogicalPlan::Limit { input, .. } => vec![input],
        LogicalPlan::Join { left, right, .. } => vec![left, right],
    }
}

fn label_logical(plan: &LogicalPlan) -> String {
    match plan {
        LogicalPlan::Scan { table_name, .. } => {
            format!("{} {}", op("Scan"), table_name.blue().bold())
        }
        LogicalPlan::Filter { predicate, .. } => {
            format!("{} {}", op("Filter"), expr_str(&predicate.to_string()))
        }
        LogicalPlan::Projection { exprs, .. } => {
            format!("{} {}", op("Projection"), expr_list(exprs))
        }
        LogicalPlan::Join {
            on, join_type, ..
        } => {
            format!(
                "{} {} ON {}",
                op("Join"),
                join_type_str(join_type),
                expr_str(&on.to_string())
            )
        }
        LogicalPlan::Sort { exprs, .. } => {
            format!("{} {}", op("Sort"), expr_list(exprs))
        }
        LogicalPlan::Aggregate {
            group_by,
            aggr_exprs,
            ..
        } => format!(
            "{} group_by={} aggs={}",
            op("Aggregate"),
            expr_list(group_by),
            expr_list(aggr_exprs)
        ),
        LogicalPlan::Limit { skip, fetch, .. } => format!(
            "{} {}{}",
            op("Limit"),
            kv("fetch", &fetch_str(*fetch)),
            if *skip > 0 {
                format!(" {}", kv("skip", &skip.to_string()))
            } else {
                String::new()
            }
        ),
    }
}

// ---------- PhysicalPlan ----------

fn children_physical(plan: &PhysicalPlan) -> Vec<&PhysicalPlan> {
    match plan {
        PhysicalPlan::TableScan { .. } => vec![],
        PhysicalPlan::Filter { input, .. }
        | PhysicalPlan::Projection { input, .. }
        | PhysicalPlan::Sort { input, .. }
        | PhysicalPlan::HashAggregate { input, .. }
        | PhysicalPlan::SortAggregate { input, .. }
        | PhysicalPlan::ScalarAggregate { input, .. }
        | PhysicalPlan::Limit { input, .. } => vec![input],
        PhysicalPlan::NestedLoopJoin { left, right, .. } => vec![left, right],
    }
}

fn label_physical(plan: &PhysicalPlan) -> String {
    match plan {
        PhysicalPlan::TableScan { table_name, .. } => {
            format!("{} {}", op("TableScan"), table_name.blue().bold())
        }
        PhysicalPlan::Filter { predicate, .. } => {
            format!("{} {}", op("Filter"), expr_str(&predicate.to_string()))
        }
        PhysicalPlan::Projection { exprs, .. } => {
            format!("{} {}", op("Projection"), expr_list(exprs))
        }
        PhysicalPlan::NestedLoopJoin {
            on, join_type, ..
        } => format!(
            "{} {} ON {}",
            op("NestedLoopJoin"),
            join_type_str(join_type),
            expr_str(&on.to_string())
        ),
        PhysicalPlan::Sort { exprs, .. } => {
            format!("{} {}", op("Sort"), expr_list(exprs))
        }
        PhysicalPlan::HashAggregate {
            group_by,
            aggr_exprs,
            ..
        } => format!(
            "{} group_by={} aggs={}",
            op("HashAggregate"),
            expr_list(group_by),
            expr_list(aggr_exprs)
        ),
        PhysicalPlan::SortAggregate {
            group_by,
            aggr_exprs,
            ..
        } => format!(
            "{} group_by={} aggs={}",
            op("SortAggregate"),
            expr_list(group_by),
            expr_list(aggr_exprs)
        ),
        PhysicalPlan::ScalarAggregate { aggr_exprs, .. } => {
            format!("{} aggs={}", op("ScalarAggregate"), expr_list(aggr_exprs))
        }
        PhysicalPlan::Limit { skip, fetch, .. } => format!(
            "{} {}{}",
            op("Limit"),
            kv("fetch", &fetch_str(*fetch)),
            if *skip > 0 {
                format!(" {}", kv("skip", &skip.to_string()))
            } else {
                String::new()
            }
        ),
    }
}

// ---------- Formatting helpers ----------

fn op(name: &str) -> String {
    name.cyan().bold().to_string()
}

fn expr_str(s: &str) -> String {
    s.green().to_string()
}

fn expr_list<E: std::fmt::Display>(exprs: &[E]) -> String {
    let inner: Vec<String> = exprs.iter().map(|e| e.to_string().green().to_string()).collect();
    format!("[{}]", inner.join(", "))
}

fn join_type_str(jt: &JoinType) -> String {
    let s = match jt {
        JoinType::Inner => "Inner",
        JoinType::Left => "Left",
        JoinType::Right => "Right",
        JoinType::Full => "Full",
    };
    s.magenta().to_string()
}

fn kv(key: &str, value: &str) -> String {
    format!("{}={}", key.bright_black(), value.yellow())
}

fn fetch_str(fetch: usize) -> String {
    if fetch == usize::MAX {
        "all".to_string()
    } else {
        fetch.to_string()
    }
}
