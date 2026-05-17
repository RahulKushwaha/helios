use expr::expr::{AggFunc, Expr, Operator};
use expr::logical_plan::builder::LogicalPlanBuilder;
use expr::logical_plan::plan::{JoinType, LogicalPlan};
use expr::schema::{Field, Schema};
use expr::types::{DataType, FieldValue};
use sqlparser::ast;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub enum PlanError {
    Parse(String),
    Unsupported(String),
    TableNotFound(String),
    ColumnNotFound(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
            Self::Unsupported(msg) => write!(f, "Unsupported: {msg}"),
            Self::TableNotFound(t) => write!(f, "Table not found: {t}"),
            Self::ColumnNotFound(c) => write!(f, "Column not found: {c}"),
        }
    }
}

/// Result of planning a SQL statement.
pub enum SqlStatement {
    CreateTable {
        name: String,
        schema: Schema,
    },
    CreateIndex {
        table: String,
        column: String,
    },
    Insert {
        table: String,
        rows: Vec<Vec<FieldValue>>,
    },
    Query(LogicalPlan),
}

/// Converts SQL strings into LogicalPlan trees using an existing catalog.
pub struct SqlPlanner {
    pub catalog: HashMap<String, Schema>,
}

impl SqlPlanner {
    pub fn new(catalog: HashMap<String, Schema>) -> Self {
        Self { catalog }
    }

    pub fn plan_sql(&self, sql: &str) -> Result<SqlStatement, PlanError> {
        let dialect = GenericDialect {};
        let statements =
            Parser::parse_sql(&dialect, sql).map_err(|e| PlanError::Parse(e.to_string()))?;

        if statements.is_empty() {
            return Err(PlanError::Parse("empty SQL".into()));
        }

        self.plan_statement(&statements[0])
    }

    fn plan_statement(&self, stmt: &ast::Statement) -> Result<SqlStatement, PlanError> {
        match stmt {
            ast::Statement::CreateTable(ct) => self.plan_create_table(ct),
            ast::Statement::CreateIndex(ci) => self.plan_create_index(ci),
            ast::Statement::Insert(ins) => self.plan_insert(ins),
            ast::Statement::Query(q) => {
                let plan = self.plan_query(q)?;
                Ok(SqlStatement::Query(plan))
            }
            _ => Err(PlanError::Unsupported(format!("{:?}", stmt))),
        }
    }

    fn plan_create_table(&self, ct: &ast::CreateTable) -> Result<SqlStatement, PlanError> {
        let name = ct.name.to_string();

        // Collect inline PRIMARY KEY columns.
        let mut pk_cols: Vec<String> = Vec::new();
        let fields: Vec<Field> = ct
            .columns
            .iter()
            .enumerate()
            .map(|(pos, col)| {
                let dt = sql_type_to_datatype(&col.data_type);
                let is_pk = col.options.iter().any(|opt| {
                    matches!(
                        opt.option,
                        ast::ColumnOption::Unique {
                            is_primary: true,
                            ..
                        }
                    )
                });
                if is_pk {
                    pk_cols.push(col.name.value.clone());
                }
                Field::new(col.name.value.clone(), dt)
                    .with_pk(is_pk)
                    .with_pos(pos)
            })
            .collect();

        // Check table-level PRIMARY KEY constraint.
        for constraint in &ct.constraints {
            if let ast::TableConstraint::PrimaryKey { columns, .. } = constraint {
                for pk_ident in columns {
                    pk_cols.push(pk_ident.value.clone());
                }
            }
        }

        // Apply table-level PK to columns.
        let fields: Vec<Field> = fields
            .into_iter()
            .map(|mut f| {
                if pk_cols.contains(&f.name) {
                    f.is_pk = true;
                    f.nullable = false;
                }
                f
            })
            .collect();

        Ok(SqlStatement::CreateTable {
            name,
            schema: Schema::new(fields),
        })
    }

    fn plan_create_index(&self, ci: &ast::CreateIndex) -> Result<SqlStatement, PlanError> {
        let table = ci.table_name.to_string();
        let column = ci
            .columns
            .first()
            .ok_or_else(|| PlanError::Parse("CREATE INDEX requires a column".into()))?
            .expr
            .to_string();
        Ok(SqlStatement::CreateIndex { table, column })
    }

    fn plan_insert(&self, ins: &ast::Insert) -> Result<SqlStatement, PlanError> {
        let table = ins.table_name.to_string();
        let schema = self
            .catalog
            .get(&table)
            .ok_or_else(|| PlanError::TableNotFound(table.clone()))?;

        let source = ins
            .source
            .as_ref()
            .ok_or_else(|| PlanError::Parse("INSERT requires VALUES".into()))?;

        let rows = match source.body.as_ref() {
            ast::SetExpr::Values(values) => {
                let mut result = Vec::new();
                for row_exprs in &values.rows {
                    let mut row = Vec::new();
                    for (i, val_expr) in row_exprs.iter().enumerate() {
                        let dt = schema.fields.get(i).map(|f| &f.data_type);
                        row.push(sql_value_expr_to_value(val_expr, dt)?);
                    }
                    result.push(row);
                }
                result
            }
            _ => {
                return Err(PlanError::Unsupported(
                    "only VALUES inserts supported".into(),
                ))
            }
        };

        Ok(SqlStatement::Insert { table, rows })
    }

    fn plan_query(&self, query: &ast::Query) -> Result<LogicalPlan, PlanError> {
        let select = match query.body.as_ref() {
            ast::SetExpr::Select(s) => s,
            _ => {
                return Err(PlanError::Unsupported(
                    "only SELECT queries supported".into(),
                ))
            }
        };

        // FROM clause — build base scan(s) with joins.
        let mut plan = self.plan_from(&select.from)?;

        // WHERE clause.
        if let Some(selection) = &select.selection {
            let predicate = self.convert_expr(selection)?;
            plan = LogicalPlan::Filter {
                predicate,
                input: Box::new(plan),
            };
        }

        // GROUP BY + aggregates.
        let has_agg = select.projection.iter().any(|p| match p {
            ast::SelectItem::UnnamedExpr(e) | ast::SelectItem::ExprWithAlias { expr: e, .. } => {
                contains_aggregate(e)
            }
            _ => false,
        });

        let group_by_exprs = extract_group_by(&select.group_by)?;

        if has_agg || !group_by_exprs.is_empty() {
            let group_by: Vec<Expr> = group_by_exprs
                .iter()
                .map(|e| self.convert_expr(e))
                .collect::<Result<_, _>>()?;

            let aggr_exprs: Vec<Expr> = select
                .projection
                .iter()
                .filter_map(|p| {
                    let e = match p {
                        ast::SelectItem::UnnamedExpr(e)
                        | ast::SelectItem::ExprWithAlias { expr: e, .. } => e,
                        _ => return None,
                    };
                    if contains_aggregate(e) {
                        Some(self.convert_expr(e).ok()?)
                    } else {
                        None
                    }
                })
                .collect();

            plan = LogicalPlan::Aggregate {
                group_by,
                aggr_exprs,
                input: Box::new(plan),
            };
        } else {
            // Projection.
            let proj_exprs = self.plan_projection(&select.projection)?;
            if !proj_exprs.is_empty() {
                plan = LogicalPlan::Projection {
                    exprs: proj_exprs,
                    input: Box::new(plan),
                };
            }
        }

        // ORDER BY.
        if let Some(ast::OrderBy { exprs, .. }) = &query.order_by {
            if !exprs.is_empty() {
                let sort_exprs: Vec<Expr> = exprs
                    .iter()
                    .map(|o| self.convert_expr(&o.expr))
                    .collect::<Result<_, _>>()?;
                plan = LogicalPlan::Sort {
                    exprs: sort_exprs,
                    input: Box::new(plan),
                };
            }
        }

        // LIMIT / OFFSET. fetch=usize::MAX means "no upper bound" (OFFSET-only
        // queries). skip=0 means no OFFSET. We only wrap the plan when at
        // least one of the two is actually set.
        let skip = match &query.offset {
            Some(off) => expr_to_usize(&off.value)?,
            None => 0,
        };
        let fetch = match &query.limit {
            Some(e) => expr_to_usize(e)?,
            None => usize::MAX,
        };
        if skip > 0 || fetch != usize::MAX {
            plan = LogicalPlan::Limit {
                skip,
                fetch,
                input: Box::new(plan),
            };
        }

        Ok(plan)
    }

    fn plan_from(&self, from: &[ast::TableWithJoins]) -> Result<LogicalPlan, PlanError> {
        if from.is_empty() {
            return Err(PlanError::Parse("no FROM clause".into()));
        }

        let first = &from[0];
        let mut plan = self.plan_table_factor(&first.relation)?;

        for join in &first.joins {
            let right = self.plan_table_factor(&join.relation)?;
            let (join_type, on_expr) = match &join.join_operator {
                ast::JoinOperator::Inner(constraint) => {
                    (JoinType::Inner, self.extract_join_constraint(constraint)?)
                }
                ast::JoinOperator::LeftOuter(constraint) => {
                    (JoinType::Left, self.extract_join_constraint(constraint)?)
                }
                ast::JoinOperator::RightOuter(constraint) => {
                    (JoinType::Right, self.extract_join_constraint(constraint)?)
                }
                ast::JoinOperator::FullOuter(constraint) => {
                    (JoinType::Full, self.extract_join_constraint(constraint)?)
                }
                ast::JoinOperator::CrossJoin => {
                    (JoinType::Inner, Expr::Literal(FieldValue::Bool(true)))
                }
                _ => {
                    return Err(PlanError::Unsupported("unsupported join type".into()));
                }
            };
            plan = LogicalPlan::Join {
                left: Box::new(plan),
                right: Box::new(right),
                on: on_expr,
                join_type,
            };
        }

        // Multiple tables in FROM (implicit cross join).
        for twj in from.iter().skip(1) {
            let right = self.plan_table_factor(&twj.relation)?;
            plan = LogicalPlan::Join {
                left: Box::new(plan),
                right: Box::new(right),
                on: Expr::Literal(FieldValue::Bool(true)),
                join_type: JoinType::Inner,
            };
        }

        Ok(plan)
    }

    fn plan_table_factor(&self, tf: &ast::TableFactor) -> Result<LogicalPlan, PlanError> {
        match tf {
            ast::TableFactor::Table { name, .. } => {
                let table_name = name.to_string();
                let schema = self
                    .catalog
                    .get(&table_name)
                    .ok_or_else(|| PlanError::TableNotFound(table_name.clone()))?;
                Ok(LogicalPlan::Scan {
                    table_name,
                    schema: schema.clone(),
                })
            }
            _ => Err(PlanError::Unsupported(
                "only table references supported in FROM".into(),
            )),
        }
    }

    fn extract_join_constraint(&self, constraint: &ast::JoinConstraint) -> Result<Expr, PlanError> {
        match constraint {
            ast::JoinConstraint::On(e) => self.convert_expr(e),
            _ => Err(PlanError::Unsupported(
                "only ON join constraints supported".into(),
            )),
        }
    }

    fn plan_projection(&self, items: &[ast::SelectItem]) -> Result<Vec<Expr>, PlanError> {
        let mut exprs = Vec::new();
        for item in items {
            match item {
                ast::SelectItem::UnnamedExpr(e) => exprs.push(self.convert_expr(e)?),
                ast::SelectItem::ExprWithAlias { expr, .. } => exprs.push(self.convert_expr(expr)?),
                ast::SelectItem::Wildcard(_) => {
                    // Wildcard means no explicit projection needed.
                    return Ok(Vec::new());
                }
                _ => return Err(PlanError::Unsupported(format!("select item: {:?}", item))),
            }
        }
        Ok(exprs)
    }

    fn convert_expr(&self, sql_expr: &ast::Expr) -> Result<Expr, PlanError> {
        match sql_expr {
            ast::Expr::Identifier(ident) => Ok(Expr::Column(ident.value.clone())),

            ast::Expr::CompoundIdentifier(parts) => {
                // e.g., t1.x — use the last part as column name.
                let name = parts
                    .iter()
                    .map(|p| p.value.clone())
                    .collect::<Vec<_>>()
                    .join(".");
                Ok(Expr::Column(name))
            }

            ast::Expr::Value(v) => Ok(Expr::Literal(sql_ast_value_to_value(v)?)),

            ast::Expr::BinaryOp { left, op, right } => {
                let l = self.convert_expr(left)?;
                let r = self.convert_expr(right)?;
                let operator = convert_binop(op)?;
                Ok(Expr::BinaryExpr {
                    left: Box::new(l),
                    op: operator,
                    right: Box::new(r),
                })
            }

            ast::Expr::Nested(inner) => self.convert_expr(inner),

            ast::Expr::Function(func) => {
                let name = func.name.to_string().to_uppercase();
                let agg = match name.as_str() {
                    "COUNT" => AggFunc::Count,
                    "SUM" => AggFunc::Sum,
                    "MIN" => AggFunc::Min,
                    "MAX" => AggFunc::Max,
                    "AVG" => AggFunc::Avg,
                    _ => return Err(PlanError::Unsupported(format!("function: {}", name))),
                };
                let args = match &func.args {
                    ast::FunctionArguments::List(arg_list) => arg_list
                        .args
                        .iter()
                        .map(|a| match a {
                            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(e)) => {
                                self.convert_expr(e)
                            }
                            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Wildcard) => {
                                Ok(Expr::Literal(FieldValue::Int(1)))
                            }
                            _ => Err(PlanError::Unsupported("function arg".into())),
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    ast::FunctionArguments::None => Vec::new(),
                    _ => return Err(PlanError::Unsupported("function args".into())),
                };
                Ok(Expr::AggregateFunction { fun: agg, args })
            }

            ast::Expr::UnaryOp {
                op: ast::UnaryOperator::Minus,
                expr,
            } => {
                let inner = self.convert_expr(expr)?;
                Ok(Expr::BinaryExpr {
                    left: Box::new(Expr::Literal(FieldValue::Int(0))),
                    op: Operator::Minus,
                    right: Box::new(inner),
                })
            }

            _ => Err(PlanError::Unsupported(format!(
                "expression: {:?}",
                sql_expr
            ))),
        }
    }
}

fn sql_type_to_datatype(sql_type: &ast::DataType) -> DataType {
    match sql_type {
        ast::DataType::Int(_)
        | ast::DataType::Integer(_)
        | ast::DataType::BigInt(_)
        | ast::DataType::SmallInt(_)
        | ast::DataType::TinyInt(_) => DataType::Int,
        ast::DataType::Float(_)
        | ast::DataType::Double
        | ast::DataType::DoublePrecision
        | ast::DataType::Real => DataType::Float,
        ast::DataType::Boolean => DataType::Bool,
        _ => DataType::Str, // VARCHAR, TEXT, etc.
    }
}

fn sql_ast_value_to_value(v: &ast::Value) -> Result<FieldValue, PlanError> {
    match v {
        ast::Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(FieldValue::Int(i))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(FieldValue::Float(f))
            } else {
                Err(PlanError::Parse(format!("invalid number: {}", n)))
            }
        }
        ast::Value::SingleQuotedString(s) | ast::Value::DoubleQuotedString(s) => {
            Ok(FieldValue::Str(s.clone()))
        }
        ast::Value::Boolean(b) => Ok(FieldValue::Bool(*b)),
        ast::Value::Null => Ok(FieldValue::Null),
        _ => Err(PlanError::Unsupported(format!("value: {:?}", v))),
    }
}

fn sql_value_expr_to_value(
    expr: &ast::Expr,
    expected_type: Option<&DataType>,
) -> Result<FieldValue, PlanError> {
    match expr {
        ast::Expr::Value(v) => sql_ast_value_to_value(v),
        ast::Expr::UnaryOp {
            op: ast::UnaryOperator::Minus,
            expr: inner,
        } => match inner.as_ref() {
            ast::Expr::Value(ast::Value::Number(n, _)) => {
                if let Ok(i) = n.parse::<i64>() {
                    Ok(FieldValue::Int(-i))
                } else if let Ok(f) = n.parse::<f64>() {
                    Ok(FieldValue::Float(-f))
                } else {
                    Err(PlanError::Parse(format!("invalid number: {}", n)))
                }
            }
            _ => Err(PlanError::Unsupported("complex INSERT value".into())),
        },
        _ => Err(PlanError::Unsupported(format!(
            "INSERT value expression: {:?}",
            expr
        ))),
    }
}

fn expr_to_usize(expr: &ast::Expr) -> Result<usize, PlanError> {
    match expr {
        ast::Expr::Value(ast::Value::Number(n, _)) => n.parse::<usize>().map_err(|_| {
            PlanError::Parse(format!(
                "LIMIT/OFFSET must be a non-negative integer, got: {n}"
            ))
        }),
        _ => Err(PlanError::Unsupported(format!(
            "LIMIT/OFFSET expression: {:?}",
            expr
        ))),
    }
}

fn convert_binop(op: &ast::BinaryOperator) -> Result<Operator, PlanError> {
    match op {
        ast::BinaryOperator::Eq => Ok(Operator::Eq),
        ast::BinaryOperator::NotEq => Ok(Operator::NotEq),
        ast::BinaryOperator::Lt => Ok(Operator::Lt),
        ast::BinaryOperator::LtEq => Ok(Operator::LtEq),
        ast::BinaryOperator::Gt => Ok(Operator::Gt),
        ast::BinaryOperator::GtEq => Ok(Operator::GtEq),
        ast::BinaryOperator::Plus => Ok(Operator::Plus),
        ast::BinaryOperator::Minus => Ok(Operator::Minus),
        ast::BinaryOperator::Multiply => Ok(Operator::Multiply),
        ast::BinaryOperator::Divide => Ok(Operator::Divide),
        ast::BinaryOperator::And => Ok(Operator::And),
        ast::BinaryOperator::Or => Ok(Operator::Or),
        _ => Err(PlanError::Unsupported(format!("operator: {:?}", op))),
    }
}

fn contains_aggregate(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Function(f) => {
            let name = f.name.to_string().to_uppercase();
            matches!(name.as_str(), "COUNT" | "SUM" | "MIN" | "MAX" | "AVG")
        }
        ast::Expr::BinaryOp { left, right, .. } => {
            contains_aggregate(left) || contains_aggregate(right)
        }
        ast::Expr::Nested(inner) => contains_aggregate(inner),
        _ => false,
    }
}

fn extract_group_by(group_by: &ast::GroupByExpr) -> Result<Vec<&ast::Expr>, PlanError> {
    match group_by {
        ast::GroupByExpr::All(_) => Err(PlanError::Unsupported("GROUP BY ALL".into())),
        ast::GroupByExpr::Expressions(exprs, _) => Ok(exprs.iter().collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_catalog() -> HashMap<String, Schema> {
        let mut catalog = HashMap::new();
        catalog.insert(
            "t1".to_string(),
            Schema::new(vec![
                Field::new("x", DataType::Int),
                Field::new("y", DataType::Int),
                Field::new("z", DataType::Int),
            ]),
        );
        catalog.insert(
            "t2".to_string(),
            Schema::new(vec![
                Field::new("y", DataType::Int),
                Field::new("w", DataType::Int),
            ]),
        );
        catalog
    }

    #[test]
    fn parse_simple_select() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("SELECT x, y FROM t1 WHERE x > 5");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlStatement::Query(plan) => match plan {
                LogicalPlan::Projection { input, exprs, .. } => {
                    assert_eq!(exprs.len(), 2);
                    assert!(matches!(*input, LogicalPlan::Filter { .. }));
                }
                _ => panic!("expected Projection"),
            },
            _ => panic!("expected Query"),
        }
    }

    #[test]
    fn parse_create_table() {
        let planner = SqlPlanner::new(HashMap::new());
        let result = planner.plan_sql("CREATE TABLE users (id INT, name VARCHAR, active BOOLEAN)");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlStatement::CreateTable { name, schema } => {
                assert_eq!(name, "users");
                assert_eq!(schema.fields.len(), 3);
                assert_eq!(schema.fields[0].data_type, DataType::Int);
                assert_eq!(schema.fields[1].data_type, DataType::Str);
                assert_eq!(schema.fields[2].data_type, DataType::Bool);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn parse_create_table_inline_pk() {
        let planner = SqlPlanner::new(HashMap::new());
        let result = planner.plan_sql("CREATE TABLE t (id INT PRIMARY KEY, name VARCHAR)");
        match result.unwrap() {
            SqlStatement::CreateTable { schema, .. } => {
                assert!(schema.fields[0].is_pk);
                assert!(!schema.fields[0].nullable);
                assert!(!schema.fields[1].is_pk);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn parse_create_table_constraint_pk() {
        let planner = SqlPlanner::new(HashMap::new());
        let result =
            planner.plan_sql("CREATE TABLE t (a INT, b INT, c VARCHAR, PRIMARY KEY (a, b))");
        match result.unwrap() {
            SqlStatement::CreateTable { schema, .. } => {
                assert!(schema.fields[0].is_pk);
                assert!(schema.fields[1].is_pk);
                assert!(!schema.fields[2].is_pk);
                assert_eq!(schema.fields[0].col_pos, 0);
                assert_eq!(schema.fields[1].col_pos, 1);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn parse_insert() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("INSERT INTO t1 VALUES (1, 2, 3), (4, 5, 6)");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlStatement::Insert { table, rows } => {
                assert_eq!(table, "t1");
                assert_eq!(rows.len(), 2);
                assert_eq!(
                    rows[0],
                    vec![FieldValue::Int(1), FieldValue::Int(2), FieldValue::Int(3)]
                );
            }
            _ => panic!("expected Insert"),
        }
    }

    #[test]
    fn parse_join() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("SELECT t1.x, t2.w FROM t1 JOIN t2 ON t1.y = t2.y");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlStatement::Query(LogicalPlan::Projection { input, .. }) => {
                assert!(matches!(*input, LogicalPlan::Join { .. }));
            }
            _ => panic!("expected Projection over Join"),
        }
    }

    #[test]
    fn parse_aggregate() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("SELECT x, SUM(z) FROM t1 GROUP BY x");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlStatement::Query(LogicalPlan::Aggregate {
                group_by,
                aggr_exprs,
                ..
            }) => {
                assert_eq!(group_by.len(), 1);
                assert_eq!(aggr_exprs.len(), 1);
            }
            _ => panic!("expected Aggregate"),
        }
    }

    #[test]
    fn parse_limit_only() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("SELECT x FROM t1 LIMIT 10").unwrap();
        match result {
            SqlStatement::Query(LogicalPlan::Limit { skip, fetch, .. }) => {
                assert_eq!(skip, 0);
                assert_eq!(fetch, 10);
            }
            _ => panic!("expected Limit"),
        }
    }

    #[test]
    fn parse_offset_only() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("SELECT x FROM t1 OFFSET 20").unwrap();
        match result {
            SqlStatement::Query(LogicalPlan::Limit { skip, fetch, .. }) => {
                assert_eq!(skip, 20);
                assert_eq!(fetch, usize::MAX);
            }
            _ => panic!("expected Limit"),
        }
    }

    #[test]
    fn parse_limit_and_offset() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner
            .plan_sql("SELECT x FROM t1 ORDER BY x LIMIT 10 OFFSET 20")
            .unwrap();
        match result {
            SqlStatement::Query(LogicalPlan::Limit {
                skip, fetch, input,
            }) => {
                assert_eq!(skip, 20);
                assert_eq!(fetch, 10);
                // Limit sits above Sort, which sits above the rest.
                assert!(matches!(*input, LogicalPlan::Sort { .. }));
            }
            _ => panic!("expected Limit over Sort"),
        }
    }

    #[test]
    fn parse_no_limit_no_offset() {
        let planner = SqlPlanner::new(test_catalog());
        let result = planner.plan_sql("SELECT x FROM t1").unwrap();
        match result {
            SqlStatement::Query(plan) => {
                assert!(
                    !matches!(plan, LogicalPlan::Limit { .. }),
                    "should not wrap in Limit when neither clause is present"
                );
            }
            _ => panic!("expected Query"),
        }
    }
}
