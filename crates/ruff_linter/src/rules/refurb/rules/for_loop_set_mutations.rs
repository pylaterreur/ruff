use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprName, Stmt, StmtFor};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::{Binding, ScopeId};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::refurb::helpers::{
    IterLocation, binding_names, loop_variables_are_used_outside_loop,
};
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

use crate::rules::refurb::helpers::parenthesize_loop_iter_if_necessary;

/// ## What it does
/// Checks for code that updates a set with the contents of an iterable by
/// using a `for` loop to call `.add()` or `.discard()` on each element
/// separately.
///
/// ## Why is this bad?
/// When adding or removing a batch of elements to or from a set, it's more
/// idiomatic to use a single method call rather than adding or removing
/// elements one by one.
///
/// ## Example
/// ```python
/// s = set()
///
/// for x in (1, 2, 3):
///     s.add(x)
///
/// for x in (1, 2, 3):
///     s.discard(x)
/// ```
///
/// Use instead:
/// ```python
/// s = set()
///
/// s.update((1, 2, 3))
/// s.difference_update((1, 2, 3))
/// ```
///
/// ## Fix safety
/// The fix will be marked as unsafe if applying the fix would delete any comments.
/// Otherwise, it is marked as safe.
///
/// ## References
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.3.5", category = Category::Complexity)]
pub(crate) struct ForLoopSetMutations {
    method_name: &'static str,
    batch_method_name: &'static str,
}

impl AlwaysFixableViolation for ForLoopSetMutations {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `set.{}()` in a for loop", self.method_name)
    }

    fn fix_title(&self) -> String {
        format!("Replace with `.{}()`", self.batch_method_name)
    }
}

/// FURB142
pub(crate) fn for_loop_set_mutations_binding(checker: &Checker, binding: &Binding) {
    if !binding.kind.is_loop_var() {
        return;
    }

    let semantic = checker.semantic();

    let Some(for_stmt) = binding
        .statement(semantic)
        .and_then(|stmt| stmt.as_for_stmt())
    else {
        return;
    };

    if for_stmt.is_async {
        return;
    }

    let binding_names = binding_names(&for_stmt.target);

    if !binding_names
        .first()
        .is_some_and(|name| name.range().contains_range(binding.range))
    {
        return;
    }

    for_loop_set_mutations(checker, for_stmt, binding.scope, &binding_names);
}

/// FURB142
pub(crate) fn for_loop_set_mutations_stmt(checker: &Checker, for_stmt: &StmtFor) {
    // Loops with bindings are handled later.
    if !binding_names(&for_stmt.target).is_empty() {
        return;
    }

    let scope_id = checker.semantic().scope_id;

    for_loop_set_mutations(checker, for_stmt, scope_id, &[]);
}

/// FURB142
fn for_loop_set_mutations(
    checker: &Checker,
    for_stmt: &StmtFor,
    scope_id: ScopeId,
    binding_names: &[&ExprName],
) {
    if !for_stmt.orelse.is_empty() {
        return;
    }
    let [Stmt::Expr(stmt_expr)] = for_stmt.body.as_slice() else {
        return;
    };
    let Expr::Call(expr_call) = stmt_expr.value.as_ref() else {
        return;
    };
    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return;
    };
    if !expr_call.arguments.keywords.is_empty() {
        return;
    }

    let (method_name, batch_method_name) = match expr_attr.attr.as_str() {
        "add" => ("add", "update"),
        "discard" => ("discard", "difference_update"),
        _ => {
            return;
        }
    };

    let Expr::Name(set) = expr_attr.value.as_ref() else {
        return;
    };

    if !checker
        .semantic()
        .resolve_name(set)
        .is_some_and(|s| typing::is_set(checker.semantic().binding(s), checker.semantic()))
    {
        return;
    }
    let [arg] = expr_call.arguments.args.as_ref() else {
        return;
    };

    if loop_variables_are_used_outside_loop(
        binding_names,
        for_stmt.range,
        checker.semantic(),
        scope_id,
    ) {
        return;
    }

    let locator = checker.locator();
    let content = match (for_stmt.target.as_ref(), arg) {
        (Expr::Name(for_target), Expr::Name(arg)) if for_target.id == arg.id => {
            format!(
                "{}.{batch_method_name}({})",
                set.id,
                parenthesize_loop_iter_if_necessary(for_stmt, checker, IterLocation::Call),
            )
        }
        (for_target, arg) => {
            let arg_content = match arg {
                Expr::Generator(generator) if !generator.parenthesized => {
                    format!("({})", locator.slice(arg))
                }
                _ => locator.slice(arg).to_string(),
            };
            format!(
                "{}.{batch_method_name}({} for {} in {})",
                set.id,
                arg_content,
                locator.slice(for_target),
                parenthesize_loop_iter_if_necessary(for_stmt, checker, IterLocation::Comprehension),
            )
        }
    };

    let applicability = if checker.comment_ranges().intersects(for_stmt.range) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };
    let fix = Fix::applicable_edit(
        Edit::range_replacement(content, for_stmt.range),
        applicability,
    );

    checker
        .report_diagnostic(
            ForLoopSetMutations {
                method_name,
                batch_method_name,
            },
            for_stmt.range,
        )
        .set_fix(fix);
}
