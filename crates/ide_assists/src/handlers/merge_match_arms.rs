use hir::TypeInfo;
use itertools::Itertools;
use std::iter::successors;
use syntax::{
    algo::neighbor,
    ast::{self, AstNode},
    Direction,
};

use crate::{AssistContext, AssistId, AssistKind, Assists, TextRange};

// Assist: merge_match_arms
//
// Merges the current match arm with the following if their bodies are identical.
//
// ```
// enum Action { Move { distance: u32 }, Stop }
//
// fn handle(action: Action) {
//     match action {
//         $0Action::Move(..) => foo(),
//         Action::Stop => foo(),
//     }
// }
// ```
// ->
// ```
// enum Action { Move { distance: u32 }, Stop }
//
// fn handle(action: Action) {
//     match action {
//         Action::Move(..) | Action::Stop => foo(),
//     }
// }
// ```
pub(crate) fn merge_match_arms(acc: &mut Assists, ctx: &AssistContext) -> Option<()> {
    let current_arm = ctx.find_node_at_offset::<ast::MatchArm>()?;
    // Don't try to handle arms with guards for now - can add support for this later
    if current_arm.guard().is_some() {
        return None;
    }
    let current_expr = current_arm.expr()?;
    let current_text_range = current_arm.syntax().text_range();
    let current_arm_types = get_arm_types(&ctx, &current_arm);

    // We check if the following match arms match this one. We could, but don't,
    // compare to the previous match arm as well.
    let arms_to_merge = successors(Some(current_arm), |it| neighbor(it, Direction::Next))
        .take_while(|arm| match arm.expr() {
            Some(expr) if arm.guard().is_none() => {
                let same_text = expr.syntax().text() == current_expr.syntax().text();
                if !same_text {
                    return false;
                }

                let arm_types = get_arm_types(&ctx, &arm);
                for i in 0..arm_types.len() {
                    let other_arm_type = &arm_types[i].as_ref();
                    let current_arm_type = current_arm_types[i].as_ref();
                    if let (Some(other_arm_type), Some(current_arm_type)) =
                        (other_arm_type, current_arm_type)
                    {
                        return &other_arm_type.original == &current_arm_type.original;
                    }
                }

                true
            }
            _ => false,
        })
        .collect::<Vec<_>>();

    if arms_to_merge.len() <= 1 {
        return None;
    }

    acc.add(
        AssistId("merge_match_arms", AssistKind::RefactorRewrite),
        "Merge match arms",
        current_text_range,
        |edit| {
            let pats = if arms_to_merge.iter().any(contains_placeholder) {
                "_".into()
            } else {
                arms_to_merge
                    .iter()
                    .filter_map(ast::MatchArm::pat)
                    .map(|x| x.syntax().to_string())
                    .collect::<Vec<String>>()
                    .join(" | ")
            };

            let arm = format!("{} => {},", pats, current_expr.syntax().text());

            if let [first, .., last] = &*arms_to_merge {
                let start = first.syntax().text_range().start();
                let end = last.syntax().text_range().end();

                edit.replace(TextRange::new(start, end), arm);
            }
        },
    )
}

fn contains_placeholder(a: &ast::MatchArm) -> bool {
    matches!(a.pat(), Some(ast::Pat::WildcardPat(..)))
}

fn get_arm_types(ctx: &AssistContext, arm: &ast::MatchArm) -> Vec<Option<TypeInfo>> {
    match arm.pat() {
        Some(ast::Pat::TupleStructPat(tp)) => tp
            .fields()
            .into_iter()
            .map(|field| {
                let pat_type = ctx.sema.type_of_pat(&field);
                pat_type
            })
            .collect_vec(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{check_assist, check_assist_not_applicable};

    use super::*;

    #[test]
    fn merge_match_arms_single_patterns() {
        check_assist(
            merge_match_arms,
            r#"
#[derive(Debug)]
enum X { A, B, C }

fn main() {
    let x = X::A;
    let y = match x {
        X::A => { 1i32$0 }
        X::B => { 1i32 }
        X::C => { 2i32 }
    }
}
"#,
            r#"
#[derive(Debug)]
enum X { A, B, C }

fn main() {
    let x = X::A;
    let y = match x {
        X::A | X::B => { 1i32 },
        X::C => { 2i32 }
    }
}
"#,
        );
    }

    #[test]
    fn merge_match_arms_multiple_patterns() {
        check_assist(
            merge_match_arms,
            r#"
#[derive(Debug)]
enum X { A, B, C, D, E }

fn main() {
    let x = X::A;
    let y = match x {
        X::A | X::B => {$0 1i32 },
        X::C | X::D => { 1i32 },
        X::E => { 2i32 },
    }
}
"#,
            r#"
#[derive(Debug)]
enum X { A, B, C, D, E }

fn main() {
    let x = X::A;
    let y = match x {
        X::A | X::B | X::C | X::D => { 1i32 },
        X::E => { 2i32 },
    }
}
"#,
        );
    }

    #[test]
    fn merge_match_arms_placeholder_pattern() {
        check_assist(
            merge_match_arms,
            r#"
#[derive(Debug)]
enum X { A, B, C, D, E }

fn main() {
    let x = X::A;
    let y = match x {
        X::A => { 1i32 },
        X::B => { 2i$032 },
        _ => { 2i32 }
    }
}
"#,
            r#"
#[derive(Debug)]
enum X { A, B, C, D, E }

fn main() {
    let x = X::A;
    let y = match x {
        X::A => { 1i32 },
        _ => { 2i32 },
    }
}
"#,
        );
    }

    #[test]
    fn merges_all_subsequent_arms() {
        check_assist(
            merge_match_arms,
            r#"
enum X { A, B, C, D, E }

fn main() {
    match X::A {
        X::A$0 => 92,
        X::B => 92,
        X::C => 92,
        X::D => 62,
        _ => panic!(),
    }
}
"#,
            r#"
enum X { A, B, C, D, E }

fn main() {
    match X::A {
        X::A | X::B | X::C => 92,
        X::D => 62,
        _ => panic!(),
    }
}
"#,
        )
    }

    #[test]
    fn merge_match_arms_rejects_guards() {
        check_assist_not_applicable(
            merge_match_arms,
            r#"
#[derive(Debug)]
enum X {
    A(i32),
    B,
    C
}

fn main() {
    let x = X::A;
    let y = match x {
        X::A(a) if a > 5 => { $01i32 },
        X::B => { 1i32 },
        X::C => { 2i32 }
    }
}
"#,
        );
    }

    #[test]
    fn merge_match_arms_different_type() {
        check_assist_not_applicable(
            merge_match_arms,
            r#"//- minicore: result
fn func() {
    match Result::<f64, f32>::Ok(0f64) {
        Ok(x) => $0x.classify(),
        Err(x) => x.classify()
    };
}
"#,
        );
    }

    #[test]
    fn merge_match_arms_different_type_multiple_fields() {
        check_assist_not_applicable(
            merge_match_arms,
            r#"//- minicore: result
fn func() {
    match Result::<(f64, f64), (f32, f32)>::Ok((0f64, 0f64)) {
        Ok(x) => $0x.1.classify(),
        Err(x) => x.1.classify()
    };
}
"#,
        );
    }

    #[test]
    fn merge_match_arms_same_type_multiple_fields() {
        check_assist(
            merge_match_arms,
            r#"//- minicore: result
fn func() {
    match Result::<(f64, f64), (f64, f64)>::Ok((0f64, 0f64)) {
        Ok(x) => $0x.1.classify(),
        Err(x) => x.1.classify()
    };
}
"#,
            r#"
fn func() {
    match Result::<(f64, f64), (f64, f64)>::Ok((0f64, 0f64)) {
        Ok(x) | Err(x) => x.1.classify(),
    };
}
"#,
        );
    }
}
